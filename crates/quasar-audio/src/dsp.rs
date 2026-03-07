//! DSP extensions — Doppler effect, reverb zones, and channel mixer.
//!
//! These components and systems augment the base `AudioPlugin` with
//! frequency-domain effects typically needed by 3D games.

use quasar_core::ecs::{Entity, System, World};
use quasar_core::TimeSnapshot;
use quasar_math::{Transform, Vec3};

use crate::{AudioListener, AudioSource, SoundId};
use crate::plugin::AudioResource;

// ---------------------------------------------------------------------------
// Doppler
// ---------------------------------------------------------------------------

/// Tracks previous-frame position so the system can compute velocity.
#[derive(Debug, Clone, Copy)]
pub struct DopplerTracker {
    pub previous_position: Vec3,
    /// Speed of sound in world units/second (default 343 m/s).
    pub speed_of_sound: f32,
}

impl Default for DopplerTracker {
    fn default() -> Self {
        Self {
            previous_position: Vec3::ZERO,
            speed_of_sound: 343.0,
        }
    }
}

/// System that applies a Doppler-based pitch shift to spatial audio sources.
///
/// Attach `DopplerTracker` to an `AudioSource` entity that is also `spatial`.
/// The listener entity must also carry `DopplerTracker`.
pub struct DopplerSystem;

impl System for DopplerSystem {
    fn name(&self) -> &str {
        "doppler"
    }

    fn run(&mut self, world: &mut World) {
        let delta = world
            .resource::<TimeSnapshot>()
            .map(|t| t.delta_seconds)
            .unwrap_or(1.0 / 60.0);

        if delta < 1e-6 {
            return;
        }

        // Listener position + velocity.
        let listener_data: Option<(Vec3, Vec3, f32)> = {
            let mut result = None;
            let listeners: Vec<(Entity, Vec3)> = world
                .query::<AudioListener>()
                .into_iter()
                .filter_map(|(e, _)| {
                    let t = world.get::<Transform>(e)?;
                    Some((e, t.position))
                })
                .collect();

            if let Some((entity, pos)) = listeners.into_iter().next() {
                if let Some(tracker) = world.get::<DopplerTracker>(entity) {
                    let vel = (pos - tracker.previous_position) / delta;
                    result = Some((pos, vel, tracker.speed_of_sound));
                }
            }
            result
        };

        let (listener_pos, listener_vel, speed_of_sound) = match listener_data {
            Some(d) => d,
            None => return,
        };

        // Source positions + velocities.
        let sources: Vec<(Entity, SoundId, Vec3, Vec3)> = world
            .query::<AudioSource>()
            .into_iter()
            .filter_map(|(e, src)| {
                if !src.spatial {
                    return None;
                }
                let sid = src.playing_id?;
                let pos = world.get::<Transform>(e)?.position;
                let tracker = world.get::<DopplerTracker>(e)?;
                let vel = (pos - tracker.previous_position) / delta;
                Some((e, sid, pos, vel))
            })
            .collect();

        // Apply pitch shift.
        if let Some(resource) = world.resource_mut::<AudioResource>() {
            for (_entity, sid, pos, src_vel) in &sources {
                let to_listener = (listener_pos - *pos).normalize_or_zero();
                let v_s = src_vel.dot(to_listener);
                let v_l = listener_vel.dot(to_listener);

                // Classic Doppler formula:
                // f' = f * (c + v_l) / (c + v_s)
                let denominator = (speed_of_sound + v_s).max(1.0);
                let rate = ((speed_of_sound + v_l) / denominator).clamp(0.5, 2.0) as f64;

                resource.audio.set_playback_rate(*sid, rate);
            }
        }

        // Update previous positions for all DopplerTracker entities.
        let trackers: Vec<(Entity, Vec3)> = world
            .query::<DopplerTracker>()
            .into_iter()
            .filter_map(|(e, _)| {
                let pos = world.get::<Transform>(e)?.position;
                Some((e, pos))
            })
            .collect();

        for (entity, pos) in trackers {
            if let Some(t) = world.get_mut::<DopplerTracker>(entity) {
                t.previous_position = pos;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Reverb Zone
// ---------------------------------------------------------------------------

/// An axis-aligned reverb zone in world space.
///
/// When the listener is inside the zone, all playing spatial sounds are
/// given a "wet" volume bump (simulating reverb mix).  This is a simple
/// approximation — full convolution reverb would require DSP on the
/// audio thread which is out of scope here.
#[derive(Debug, Clone)]
pub struct ReverbZone {
    /// Centre of the zone in world space.
    pub center: Vec3,
    /// Half-extents of the AABB.
    pub half_extents: Vec3,
    /// How much extra volume (0.0–1.0) to mix in when inside the zone.
    pub wet_mix: f32,
    /// Reverb decay time (for future DSP integration).
    pub decay_time: f32,
}

impl ReverbZone {
    pub fn new(center: Vec3, half_extents: Vec3, wet_mix: f32) -> Self {
        Self {
            center,
            half_extents,
            wet_mix,
            decay_time: 1.5,
        }
    }

    pub fn contains(&self, point: Vec3) -> bool {
        let d = (point - self.center).abs();
        d.x <= self.half_extents.x && d.y <= self.half_extents.y && d.z <= self.half_extents.z
    }
}

/// System that boosts spatial source volumes when the listener is inside a
/// [`ReverbZone`].  Runs after the main `SpatialAudioSystem`.
pub struct ReverbZoneSystem;

impl System for ReverbZoneSystem {
    fn name(&self) -> &str {
        "reverb_zone"
    }

    fn run(&mut self, world: &mut World) {
        let listener_pos: Option<Vec3> = world
            .query::<AudioListener>()
            .into_iter()
            .filter_map(|(e, _)| Some(world.get::<Transform>(e)?.position))
            .next();

        let listener_pos = match listener_pos {
            Some(p) => p,
            None => return,
        };

        // Determine if the listener is in any reverb zone.
        let zones: Vec<ReverbZone> = world
            .query::<ReverbZone>()
            .into_iter()
            .map(|(_, z)| z.clone())
            .collect();

        let max_wet: f32 = zones
            .iter()
            .filter(|z| z.contains(listener_pos))
            .map(|z| z.wet_mix)
            .fold(0.0f32, f32::max);

        if max_wet <= 0.0 {
            return;
        }

        // Apply wet mix as a volume boost to all spatial sources,
        // scaling by each source's base volume to avoid overriding it.
        let spatial: Vec<(SoundId, f32)> = world
            .query::<AudioSource>()
            .into_iter()
            .filter_map(|(_, src)| {
                if src.spatial {
                    src.playing_id.map(|sid| (sid, src.volume))
                } else {
                    None
                }
            })
            .collect();

        if let Some(resource) = world.resource_mut::<AudioResource>() {
            for (sid, base_volume) in spatial {
                resource
                    .audio
                    .set_volume(sid, base_volume as f64 * (1.0 + max_wet as f64));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Audio Mixer (channel groups)
// ---------------------------------------------------------------------------

/// Identifies an audio channel group (e.g. music, sfx, voice).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AudioChannel(pub String);

impl AudioChannel {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }
}

/// Maps channels to volume levels. Insert as a world resource.
#[derive(Debug, Clone)]
pub struct AudioMixer {
    pub channels: std::collections::HashMap<String, f32>,
    pub master_volume: f32,
}

impl AudioMixer {
    pub fn new() -> Self {
        Self {
            channels: std::collections::HashMap::new(),
            master_volume: 1.0,
        }
    }

    pub fn set_channel_volume(&mut self, channel: &str, volume: f32) {
        self.channels.insert(channel.to_string(), volume.clamp(0.0, 1.0));
    }

    pub fn channel_volume(&self, channel: &str) -> f32 {
        *self.channels.get(channel).unwrap_or(&1.0)
    }

    pub fn effective_volume(&self, channel: &str) -> f32 {
        self.master_volume * self.channel_volume(channel)
    }
}

impl Default for AudioMixer {
    fn default() -> Self {
        Self::new()
    }
}

/// System that applies per-channel volume from `AudioMixer` to each sound.
pub struct AudioMixerSystem;

impl System for AudioMixerSystem {
    fn name(&self) -> &str {
        "audio_mixer"
    }

    fn run(&mut self, world: &mut World) {
        let mixer = match world.resource::<AudioMixer>() {
            Some(m) => m.clone(),
            None => return,
        };

        let sources: Vec<(SoundId, String)> = world
            .query::<AudioSource>()
            .into_iter()
            .filter_map(|(e, src)| {
                let sid = src.playing_id?;
                let channel = world.get::<AudioChannel>(e)?;
                Some((sid, channel.0.clone()))
            })
            .collect();

        if sources.is_empty() {
            return;
        }

        if let Some(resource) = world.resource_mut::<AudioResource>() {
            for (sid, channel) in sources {
                let vol = mixer.effective_volume(&channel) as f64;
                resource.audio.set_volume(sid, vol);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Convolution reverb (CPU-side time-domain, applied per-frame to wet bus)
// ---------------------------------------------------------------------------

/// Impulse response for a convolution reverb.
///
/// Stores a mono impulse response (IR) as a vector of f32 samples.
/// For real-time usage, only the first `max_length` samples are used.
#[derive(Debug, Clone)]
pub struct ConvolutionImpulseResponse {
    /// Mono IR samples (normalised to −1.0 – 1.0).
    pub samples: Vec<f32>,
    /// Sample rate of the IR.
    pub sample_rate: u32,
}

impl ConvolutionImpulseResponse {
    /// Load a WAV impulse response from a raw PCM buffer (mono, f32).
    pub fn from_samples(samples: Vec<f32>, sample_rate: u32) -> Self {
        Self { samples, sample_rate }
    }

    /// Truncate the IR to `max_samples` length for performance.
    pub fn truncated(&self, max_samples: usize) -> Self {
        let len = self.samples.len().min(max_samples);
        Self {
            samples: self.samples[..len].to_vec(),
            sample_rate: self.sample_rate,
        }
    }
}

/// CPU-side convolution reverb processor.
///
/// For each audio buffer, convolves the input with the IR using direct
/// (time-domain) convolution. For short IRs (< 4096 samples) this is
/// practical; longer IRs should use partitioned FFT.
pub struct ConvolutionReverb {
    pub ir: ConvolutionImpulseResponse,
    /// Tail buffer holding the overlap from previous frames.
    tail: Vec<f32>,
    /// Wet/dry mix (0.0 = fully dry, 1.0 = fully wet).
    pub wet_mix: f32,
}

impl ConvolutionReverb {
    pub fn new(ir: ConvolutionImpulseResponse, wet_mix: f32) -> Self {
        let tail_len = ir.samples.len();
        Self {
            ir,
            tail: vec![0.0; tail_len],
            wet_mix: wet_mix.clamp(0.0, 1.0),
        }
    }

    /// Process a block of audio samples in-place (mono).
    /// Adds the convolution wet signal to the buffer.
    pub fn process(&mut self, buffer: &mut [f32]) {
        let ir_len = self.ir.samples.len();
        if ir_len == 0 {
            return;
        }

        let n = buffer.len();
        // Direct convolution: output[i] = sum_{j=0..ir_len-1} input[i-j] * ir[j]
        // We accumulate into a temporary wet buffer.
        let mut wet = vec![0.0f32; n + ir_len - 1];

        for (i, &sample) in buffer.iter().enumerate() {
            for (j, &ir_sample) in self.ir.samples.iter().enumerate() {
                wet[i + j] += sample * ir_sample;
            }
        }

        // Add tail from previous frame.
        for (i, &t) in self.tail.iter().enumerate().take(n) {
            wet[i] += t;
        }

        // Mix wet into the buffer.
        let mix = self.wet_mix;
        for (i, s) in buffer.iter_mut().enumerate() {
            *s = *s * (1.0 - mix) + wet[i] * mix;
        }

        // Save tail for next frame.
        self.tail.resize(ir_len, 0.0);
        if n + ir_len > n {
            for (i, t) in self.tail.iter_mut().enumerate() {
                let idx = n + i;
                *t = if idx < wet.len() { wet[idx] } else { 0.0 };
            }
        }
    }

    /// Reset the reverb tail (e.g., after a scene change).
    pub fn reset(&mut self) {
        self.tail.iter_mut().for_each(|s| *s = 0.0);
    }
}

/// ECS component: attach to an entity to mark it as a convolution reverb zone.
/// When the listener enters this zone's AABB, the reverb is applied.
#[derive(Debug, Clone)]
pub struct ConvolutionReverbZone {
    pub center: Vec3,
    pub half_extents: Vec3,
    /// Impulse response samples (mono f32).
    pub ir_samples: Vec<f32>,
    pub ir_sample_rate: u32,
    pub wet_mix: f32,
}

impl ConvolutionReverbZone {
    pub fn new(center: Vec3, half_extents: Vec3, ir: &ConvolutionImpulseResponse, wet_mix: f32) -> Self {
        Self {
            center,
            half_extents,
            ir_samples: ir.samples.clone(),
            ir_sample_rate: ir.sample_rate,
            wet_mix,
        }
    }

    pub fn contains(&self, point: Vec3) -> bool {
        let d = (point - self.center).abs();
        d.x <= self.half_extents.x && d.y <= self.half_extents.y && d.z <= self.half_extents.z
    }
}

// ---------------------------------------------------------------------------
// Audio streaming (chunked file playback for large assets)
// ---------------------------------------------------------------------------

/// Streaming mode for audio playback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamingMode {
    /// Load the entire file into memory before playing.
    FullyLoaded,
    /// Stream from disk in chunks.
    Streaming { chunk_size_bytes: usize },
}

impl Default for StreamingMode {
    fn default() -> Self {
        Self::FullyLoaded
    }
}

/// ECS component marking an audio source for streaming playback.
///
/// Attach alongside `AudioSource` to enable chunked playback.
/// The `StreamingAudioSystem` manages the read-ahead buffer.
#[derive(Debug, Clone)]
pub struct StreamingAudioSource {
    pub mode: StreamingMode,
    /// Path to the audio file.
    pub path: String,
    /// Current byte offset into the file (for sequential reads).
    pub read_offset: u64,
    /// Whether playback has started.
    pub started: bool,
    /// Number of chunks pre-buffered ahead.
    pub prefetch_chunks: u32,
}

impl StreamingAudioSource {
    pub fn new(path: impl Into<String>, chunk_size_bytes: usize) -> Self {
        Self {
            mode: StreamingMode::Streaming { chunk_size_bytes },
            path: path.into(),
            read_offset: 0,
            started: false,
            prefetch_chunks: 3,
        }
    }

    pub fn with_prefetch(mut self, chunks: u32) -> Self {
        self.prefetch_chunks = chunks;
        self
    }
}

/// A ring buffer holding decoded audio chunks for streaming playback.
pub struct StreamingBuffer {
    /// Decoded PCM samples (interleaved, f32).
    pub samples: Vec<f32>,
    /// Read cursor within `samples`.
    pub read_pos: usize,
    /// Write cursor within `samples`.
    pub write_pos: usize,
    pub capacity: usize,
}

impl StreamingBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            samples: vec![0.0; capacity],
            read_pos: 0,
            write_pos: 0,
            capacity,
        }
    }

    /// Available samples to read.
    pub fn available(&self) -> usize {
        if self.write_pos >= self.read_pos {
            self.write_pos - self.read_pos
        } else {
            self.capacity - self.read_pos + self.write_pos
        }
    }

    /// Push decoded samples into the ring buffer.
    pub fn push(&mut self, data: &[f32]) {
        for &sample in data {
            self.samples[self.write_pos] = sample;
            self.write_pos = (self.write_pos + 1) % self.capacity;
        }
    }

    /// Read up to `count` samples from the buffer.
    pub fn read(&mut self, count: usize) -> Vec<f32> {
        let avail = self.available().min(count);
        let mut out = Vec::with_capacity(avail);
        for _ in 0..avail {
            out.push(self.samples[self.read_pos]);
            self.read_pos = (self.read_pos + 1) % self.capacity;
        }
        out
    }
}

/// System that manages streaming audio sources.
///
/// For each entity with `StreamingAudioSource`, ensures that read-ahead
/// buffers are topped up. Actual file I/O is performed on a background
/// thread (via rayon) in production, but this system stub handles the
/// bookkeeping and state transitions.
pub struct StreamingAudioSystem;

impl System for StreamingAudioSystem {
    fn name(&self) -> &str {
        "streaming_audio"
    }

    fn run(&mut self, world: &mut World) {
        let entities: Vec<(Entity, StreamingAudioSource)> = world
            .query::<StreamingAudioSource>()
            .into_iter()
            .map(|(e, s)| (e, s.clone()))
            .collect();

        for (entity, mut streaming) in entities {
            if !streaming.started {
                // Mark as started — in production, kick off the first async read here.
                streaming.started = true;
                if let Some(s) = world.get_mut::<StreamingAudioSource>(entity) {
                    s.started = true;
                }
                log::info!("Streaming audio started for {}", streaming.path);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// HRTF spatial audio
// ---------------------------------------------------------------------------

/// A single HRTF impulse response pair (left ear, right ear) for one direction.
#[derive(Debug, Clone)]
pub struct HrtfIrPair {
    pub left: Vec<f32>,
    pub right: Vec<f32>,
}

/// An entry in the HRTF database keyed by elevation and azimuth.
#[derive(Debug, Clone)]
pub struct HrtfEntry {
    /// Elevation in degrees (−90 to +90).
    pub elevation: f32,
    /// Azimuth in degrees (0 to 360).
    pub azimuth: f32,
    pub ir: HrtfIrPair,
}

/// HRTF dataset — a collection of direction-dependent impulse response pairs.
///
/// Typically loaded from MIT Kemar or CIPIC data. Each entry stores a short
/// FIR filter (128–512 taps) for left and right ears at a given direction.
#[derive(Debug, Clone)]
pub struct HrtfDatabase {
    pub entries: Vec<HrtfEntry>,
    pub sample_rate: u32,
    pub ir_length: usize,
}

impl HrtfDatabase {
    /// Create a database from pre-loaded entries.
    pub fn new(entries: Vec<HrtfEntry>, sample_rate: u32) -> Self {
        let ir_length = entries.first().map(|e| e.ir.left.len()).unwrap_or(0);
        Self { entries, sample_rate, ir_length }
    }

    /// Look up the closest HRTF IR pair for a given direction.
    ///
    /// `elevation` in degrees (−90..+90), `azimuth` in degrees (0..360).
    pub fn lookup(&self, elevation: f32, azimuth: f32) -> Option<&HrtfIrPair> {
        if self.entries.is_empty() {
            return None;
        }
        let mut best_idx = 0;
        let mut best_dist = f32::MAX;
        let az = azimuth.rem_euclid(360.0);
        for (i, entry) in self.entries.iter().enumerate() {
            let de = entry.elevation - elevation;
            let da = {
                let d = (entry.azimuth - az).abs();
                d.min(360.0 - d)
            };
            let dist = de * de + da * da;
            if dist < best_dist {
                best_dist = dist;
                best_idx = i;
            }
        }
        Some(&self.entries[best_idx].ir)
    }

    /// Bilinear interpolation between the 4 nearest entries.
    pub fn lookup_interpolated(&self, elevation: f32, azimuth: f32) -> HrtfIrPair {
        if self.entries.is_empty() {
            return HrtfIrPair { left: vec![], right: vec![] };
        }
        // Find 4 nearest neighbours by angular distance.
        let az = azimuth.rem_euclid(360.0);
        let mut dists: Vec<(usize, f32)> = self.entries.iter().enumerate().map(|(i, e)| {
            let de = e.elevation - elevation;
            let da = {
                let d = (e.azimuth - az).abs();
                d.min(360.0 - d)
            };
            (i, de * de + da * da)
        }).collect();
        dists.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        let n = dists.len().min(4);
        let total_w: f32 = dists[..n].iter().map(|&(_, d)| 1.0 / (d + 1e-6)).sum();
        let len = self.ir_length;
        let mut left = vec![0.0f32; len];
        let mut right = vec![0.0f32; len];
        for &(idx, dist) in &dists[..n] {
            let w = (1.0 / (dist + 1e-6)) / total_w;
            let entry = &self.entries[idx];
            for j in 0..len.min(entry.ir.left.len()) {
                left[j] += entry.ir.left[j] * w;
                right[j] += entry.ir.right[j] * w;
            }
        }
        HrtfIrPair { left, right }
    }
}

/// Per-ear convolution state for HRTF processing.
struct EarConvolver {
    ir: Vec<f32>,
    tail: Vec<f32>,
}

impl EarConvolver {
    fn new(ir: Vec<f32>) -> Self {
        let len = ir.len();
        Self { ir, tail: vec![0.0; len] }
    }

    fn set_ir(&mut self, ir: Vec<f32>) {
        self.tail.resize(ir.len(), 0.0);
        self.ir = ir;
    }

    fn process(&mut self, input: &[f32], output: &mut [f32]) {
        let ir_len = self.ir.len();
        if ir_len == 0 { return; }
        let n = input.len();
        let mut wet = vec![0.0f32; n + ir_len - 1];
        for (i, &s) in input.iter().enumerate() {
            for (j, &h) in self.ir.iter().enumerate() {
                wet[i + j] += s * h;
            }
        }
        for (i, &t) in self.tail.iter().enumerate().take(n) {
            wet[i] += t;
        }
        for (i, o) in output.iter_mut().enumerate().take(n) {
            *o = wet[i];
        }
        for (i, t) in self.tail.iter_mut().enumerate() {
            let idx = n + i;
            *t = if idx < wet.len() { wet[idx] } else { 0.0 };
        }
    }

    fn reset(&mut self) {
        self.tail.iter_mut().for_each(|s| *s = 0.0);
    }
}

/// HRTF processor that convolves a mono source into stereo (L/R) using
/// direction-dependent impulse responses from an [`HrtfDatabase`].
pub struct HrtfProcessor {
    left: EarConvolver,
    right: EarConvolver,
    /// Current direction (elevation, azimuth) in degrees.
    pub elevation: f32,
    pub azimuth: f32,
}

impl HrtfProcessor {
    pub fn new(db: &HrtfDatabase, elevation: f32, azimuth: f32) -> Self {
        let ir = db.lookup_interpolated(elevation, azimuth);
        Self {
            left: EarConvolver::new(ir.left),
            right: EarConvolver::new(ir.right),
            elevation,
            azimuth,
        }
    }

    /// Update the source direction and reload IRs from the database.
    pub fn set_direction(&mut self, db: &HrtfDatabase, elevation: f32, azimuth: f32) {
        self.elevation = elevation;
        self.azimuth = azimuth;
        let ir = db.lookup_interpolated(elevation, azimuth);
        self.left.set_ir(ir.left);
        self.right.set_ir(ir.right);
    }

    /// Convolve a mono input buffer into interleaved stereo (L, R, L, R, …).
    pub fn process(&mut self, mono_input: &[f32], stereo_output: &mut [f32]) {
        let n = mono_input.len();
        let mut left_buf = vec![0.0f32; n];
        let mut right_buf = vec![0.0f32; n];
        self.left.process(mono_input, &mut left_buf);
        self.right.process(mono_input, &mut right_buf);
        for i in 0..n {
            if i * 2 + 1 < stereo_output.len() {
                stereo_output[i * 2] = left_buf[i];
                stereo_output[i * 2 + 1] = right_buf[i];
            }
        }
    }

    pub fn reset(&mut self) {
        self.left.reset();
        self.right.reset();
    }
}

/// ECS component: marks an entity's audio source for HRTF spatialization.
#[derive(Debug, Clone)]
pub struct HrtfSource {
    /// Elevation relative to listener (computed by HrtfSystem).
    pub elevation: f32,
    /// Azimuth relative to listener (computed by HrtfSystem).
    pub azimuth: f32,
}

impl Default for HrtfSource {
    fn default() -> Self {
        Self { elevation: 0.0, azimuth: 0.0 }
    }
}

/// System that computes per-source HRTF elevation/azimuth from world positions.
///
/// Requires the world to have an `HrtfDatabase` resource and an
/// `AudioListener` entity with a `Transform`.
pub struct HrtfSystem;

impl System for HrtfSystem {
    fn name(&self) -> &str {
        "hrtf"
    }

    fn run(&mut self, world: &mut World) {
        // Get listener position and forward/up vectors.
        let listener = world
            .query::<AudioListener>()
            .into_iter()
            .filter_map(|(e, _)| {
                let t = world.get::<Transform>(e)?;
                Some(t.clone())
            })
            .next();
        let listener = match listener {
            Some(l) => l,
            None => return,
        };

        let listener_pos = listener.position;
        // Derive forward/right/up from listener rotation.
        let fwd = listener.rotation * Vec3::new(0.0, 0.0, -1.0);
        let up = listener.rotation * Vec3::Y;
        let right = fwd.cross(up).normalize_or_zero();

        let sources: Vec<(Entity, Vec3)> = world
            .query::<HrtfSource>()
            .into_iter()
            .filter_map(|(e, _)| {
                let t = world.get::<Transform>(e)?;
                Some((e, t.position))
            })
            .collect();

        for (entity, pos) in sources {
            let dir = (pos - listener_pos).normalize_or_zero();
            // Project onto listener-local axes.
            let x = dir.dot(right);
            let y = dir.dot(up);
            let z = dir.dot(fwd);
            let azimuth = x.atan2(z).to_degrees().rem_euclid(360.0);
            let elevation = y.asin().to_degrees().clamp(-90.0, 90.0);
            if let Some(hrtf) = world.get_mut::<HrtfSource>(entity) {
                hrtf.elevation = elevation;
                hrtf.azimuth = azimuth;
            }
        }
    }
}
