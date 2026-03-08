//! # Quasar Audio
//!
//! Audio playback system powered by [Kira](https://docs.rs/kira).
//!
//! Supports one-shot sound effects, looping music, volume/playback-rate
//! control, and a basic spatial audio model.

pub mod dsp;
pub mod plugin;

use std::collections::HashMap;
use std::path::Path;

use kira::manager::{backend::DefaultBackend, AudioManager, AudioManagerSettings};
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle};
use kira::sound::streaming::{StreamingSoundData, StreamingSoundHandle};
use kira::track::{TrackBuilder, TrackHandle};
use kira::tween::Tween;

/// Unique identifier for a loaded sound.
pub type SoundId = u64;

// ---------------------------------------------------------------------------
// Audio Bus / Mixer
// ---------------------------------------------------------------------------

/// Named audio bus (sub-mix channel).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AudioBus {
    Master,
    Music,
    Sfx,
    Voice,
    Ambient,
    /// User-defined bus.
    Custom(String),
}

impl Default for AudioBus {
    fn default() -> Self {
        Self::Sfx
    }
}

/// Manages per-bus mixer tracks routed through Kira.
pub struct BusManager {
    tracks: HashMap<AudioBus, TrackHandle>,
}

impl BusManager {
    /// Create default buses (Master, Music, Sfx, Voice, Ambient).
    pub fn new(manager: &mut AudioManager) -> Self {
        let mut tracks = HashMap::new();
        for bus in [
            AudioBus::Master,
            AudioBus::Music,
            AudioBus::Sfx,
            AudioBus::Voice,
            AudioBus::Ambient,
        ] {
            if let Ok(handle) = manager.add_sub_track(TrackBuilder::new()) {
                tracks.insert(bus, handle);
            }
        }
        Self { tracks }
    }

    /// Get the track handle for a bus, creating it on-demand for Custom buses.
    pub fn track_for(&mut self, bus: &AudioBus, manager: &mut AudioManager) -> Option<&TrackHandle> {
        if !self.tracks.contains_key(bus) {
            if let Ok(handle) = manager.add_sub_track(TrackBuilder::new()) {
                self.tracks.insert(bus.clone(), handle);
            }
        }
        self.tracks.get(bus)
    }

    /// Set volume on a bus (0.0 = silent, 1.0 = full).
    pub fn set_bus_volume(&mut self, bus: &AudioBus, volume: f64) {
        if let Some(track) = self.tracks.get_mut(bus) {
            track.set_volume(kira::Volume::Amplitude(volume), Tween::default());
        }
    }
}

/// Handle wrapper that unifies static and streaming playback.
enum SoundHandle {
    Static(StaticSoundHandle),
    Streaming(StreamingSoundHandle<kira::sound::FromFileError>),
}

impl SoundHandle {
    fn pause(&mut self) {
        match self {
            Self::Static(h) => h.pause(Tween::default()),
            Self::Streaming(h) => h.pause(Tween::default()),
        }
    }

    fn resume(&mut self) {
        match self {
            Self::Static(h) => h.resume(Tween::default()),
            Self::Streaming(h) => h.resume(Tween::default()),
        }
    }

    fn stop(&mut self) {
        match self {
            Self::Static(h) => h.stop(Tween::default()),
            Self::Streaming(h) => h.stop(Tween::default()),
        }
    }

    fn set_volume(&mut self, volume: kira::Volume) {
        match self {
            Self::Static(h) => h.set_volume(volume, Tween::default()),
            Self::Streaming(h) => h.set_volume(volume, Tween::default()),
        }
    }

    fn set_playback_rate(&mut self, rate: f64) {
        match self {
            Self::Static(h) => h.set_playback_rate(rate, Tween::default()),
            Self::Streaming(h) => h.set_playback_rate(rate, Tween::default()),
        }
    }

    fn set_panning(&mut self, panning: f64) {
        match self {
            Self::Static(h) => h.set_panning(panning, Tween::default()),
            Self::Streaming(h) => h.set_panning(panning, Tween::default()),
        }
    }
}

/// The engine's audio system — manages sound playback and mixing.
pub struct AudioSystem {
    manager: Option<AudioManager>,
    next_id: SoundId,
    handles: HashMap<SoundId, SoundHandle>,
    sound_cache: HashMap<String, StaticSoundData>,
    pub bus_manager: Option<BusManager>,
}

impl AudioSystem {
    /// Initialize the audio backend.
    pub fn new() -> Self {
        let mut manager =
            AudioManager::<DefaultBackend>::new(AudioManagerSettings::default()).ok();
        if manager.is_none() {
            log::warn!("Failed to initialize audio backend \u{2014} audio will be silent");
        }
        let bus_manager = manager.as_mut().map(|m| BusManager::new(m));
        Self {
            manager,
            next_id: 1,
            handles: HashMap::new(),
            sound_cache: HashMap::new(),
            bus_manager,
        }
    }

    /// Whether the audio backend is functional.
    pub fn is_available(&self) -> bool {
        self.manager.is_some()
    }

    // ------------------------------------------------------------------
    // Sound loading & playback
    // ------------------------------------------------------------------

    /// Load and immediately play a sound file. Returns a [`SoundId`] handle.
    pub fn play<P: AsRef<Path>>(&mut self, path: P) -> Option<SoundId> {
        self.play_on_bus(path, &AudioBus::Sfx)
    }

    /// Play a sound routed through the specified audio bus.
    pub fn play_on_bus<P: AsRef<Path>>(&mut self, path: P, bus: &AudioBus) -> Option<SoundId> {
        let manager = self.manager.as_mut()?;
        let path_str = path.as_ref().to_string_lossy().to_string();
        let mut data = if let Some(cached) = self.sound_cache.get(&path_str) {
            cached.clone()
        } else {
            let loaded = StaticSoundData::from_file(&path_str).ok()?;
            self.sound_cache.insert(path_str.clone(), loaded.clone());
            loaded
        };
        if let Some(ref mut bus_mgr) = self.bus_manager {
            if let Some(track) = bus_mgr.track_for(bus, manager) {
                data.settings.output_destination = kira::OutputDestination::Track(track.id());
            }
        }
        let handle = manager.play(data).ok()?;
        let id = self.next_id;
        self.next_id += 1;
        self.handles.insert(id, SoundHandle::Static(handle));
        Some(id)
    }

    /// Play a sound in a loop.
    pub fn play_looped<P: AsRef<Path>>(&mut self, path: P) -> Option<SoundId> {
        self.play_looped_on_bus(path, &AudioBus::Sfx)
    }

    /// Play a looped sound routed through the specified audio bus.
    pub fn play_looped_on_bus<P: AsRef<Path>>(&mut self, path: P, bus: &AudioBus) -> Option<SoundId> {
        let manager = self.manager.as_mut()?;
        let path_str = path.as_ref().to_string_lossy().to_string();
        let mut data = if let Some(cached) = self.sound_cache.get(&path_str) {
            cached.clone()
        } else {
            let loaded = StaticSoundData::from_file(&path_str).ok()?;
            self.sound_cache.insert(path_str.clone(), loaded.clone());
            loaded
        };
        data.settings.loop_region = Some(kira::sound::Region::default());
        if let Some(ref mut bus_mgr) = self.bus_manager {
            if let Some(track) = bus_mgr.track_for(bus, manager) {
                data.settings.output_destination = kira::OutputDestination::Track(track.id());
            }
        }
        let handle = manager.play(data).ok()?;
        let id = self.next_id;
        self.next_id += 1;
        self.handles.insert(id, SoundHandle::Static(handle));
        Some(id)
    }

    /// Play a sound via streaming — reads from disk incrementally instead of
    /// loading the entire file into memory. Ideal for music and long audio.
    pub fn play_streaming<P: AsRef<Path>>(&mut self, path: P) -> Option<SoundId> {
        self.play_streaming_on_bus(path, &AudioBus::Sfx)
    }

    /// Stream a sound routed through the specified audio bus.
    pub fn play_streaming_on_bus<P: AsRef<Path>>(&mut self, path: P, bus: &AudioBus) -> Option<SoundId> {
        let manager = self.manager.as_mut()?;
        let path_str = path.as_ref().to_string_lossy().to_string();
        let mut data = StreamingSoundData::from_file(&path_str).ok()?;
        if let Some(ref mut bus_mgr) = self.bus_manager {
            if let Some(track) = bus_mgr.track_for(bus, manager) {
                data.settings.output_destination = kira::OutputDestination::Track(track.id());
            }
        }
        let handle = manager.play(data).ok()?;
        let id = self.next_id;
        self.next_id += 1;
        self.handles.insert(id, SoundHandle::Streaming(handle));
        Some(id)
    }

    /// Play a streaming sound in a loop.
    pub fn play_streaming_looped<P: AsRef<Path>>(&mut self, path: P) -> Option<SoundId> {
        self.play_streaming_looped_on_bus(path, &AudioBus::Sfx)
    }

    /// Play a looped streaming sound routed through the specified audio bus.
    pub fn play_streaming_looped_on_bus<P: AsRef<Path>>(&mut self, path: P, bus: &AudioBus) -> Option<SoundId> {
        let manager = self.manager.as_mut()?;
        let path_str = path.as_ref().to_string_lossy().to_string();
        let mut data = StreamingSoundData::from_file(&path_str).ok()?;
        data.settings.loop_region = Some(kira::sound::Region::default());
        if let Some(ref mut bus_mgr) = self.bus_manager {
            if let Some(track) = bus_mgr.track_for(bus, manager) {
                data.settings.output_destination = kira::OutputDestination::Track(track.id());
            }
        }
        let handle = manager.play(data).ok()?;
        let id = self.next_id;
        self.next_id += 1;
        self.handles.insert(id, SoundHandle::Streaming(handle));
        Some(id)
    }

    /// Pause a playing sound.
    pub fn pause(&mut self, id: SoundId) {
        if let Some(handle) = self.handles.get_mut(&id) {
            handle.pause();
        }
    }

    /// Resume a paused sound.
    pub fn resume(&mut self, id: SoundId) {
        if let Some(handle) = self.handles.get_mut(&id) {
            handle.resume();
        }
    }

    /// Stop a sound and free its handle.
    pub fn stop(&mut self, id: SoundId) {
        if let Some(mut handle) = self.handles.remove(&id) {
            handle.stop();
        }
    }

    /// Stop all sounds.
    pub fn stop_all(&mut self) {
        for (_, mut handle) in self.handles.drain() {
            handle.stop();
        }
    }

    /// Set the volume of a sound (0.0 = silent, 1.0 = full).
    pub fn set_volume(&mut self, id: SoundId, volume: f64) {
        if let Some(handle) = self.handles.get_mut(&id) {
            handle.set_volume(kira::Volume::Amplitude(volume));
        }
    }

    /// Set the playback rate (1.0 = normal, 2.0 = double speed).
    pub fn set_playback_rate(&mut self, id: SoundId, rate: f64) {
        if let Some(handle) = self.handles.get_mut(&id) {
            handle.set_playback_rate(rate);
        }
    }

    /// Returns the number of active sound handles.
    pub fn active_sounds(&self) -> usize {
        self.handles.len()
    }

    /// Update volume and panning of a spatial sound.
    ///
    /// `distance` — world-space distance between source and listener.
    /// `panning`  — stereo pan in \[0.0 (left) .. 1.0 (right)\], 0.5 = centre.
    /// The `source` component provides base volume, ref_distance, max_distance,
    /// and rolloff_factor.
    pub fn update_spatial(
        &mut self,
        id: SoundId,
        source: &AudioSource,
        distance: f32,
        panning: f64,
    ) {
        let handle = match self.handles.get_mut(&id) {
            Some(h) => h,
            None => return,
        };

        // Inverse-distance clamped attenuation.
        let gain = if distance >= source.max_distance {
            0.0
        } else {
            let d = distance.max(source.ref_distance);
            let g = source.ref_distance
                / (source.ref_distance + source.rolloff_factor * (d - source.ref_distance));
            (g * source.volume).clamp(0.0, 1.0)
        };

        handle.set_volume(kira::Volume::Amplitude(gain as f64));
        handle.set_panning(panning);
    }
}

impl Default for AudioSystem {
    fn default() -> Self {
        Self::new()
    }
}

/// ECS component marking an entity as an audio source.
#[derive(Debug, Clone)]
pub struct AudioSource {
    /// The file path to the sound asset.
    pub path: String,
    /// Whether this source should loop.
    pub looping: bool,
    /// Volume (0.0 – 1.0).
    pub volume: f32,
    /// If Some, the sound is currently playing with this id.
    pub playing_id: Option<SoundId>,    /// Audio bus to route this source through.
    pub bus: AudioBus,    /// Enable spatial (3D positional) audio for this source.
    pub spatial: bool,
    /// Reference distance — distance at which volume is unattenuated (default 1.0).
    pub ref_distance: f32,
    /// Maximum distance beyond which the source is silent (default 50.0).
    pub max_distance: f32,
    /// Rolloff factor controlling how quickly volume falls off (default 1.0).
    pub rolloff_factor: f32,
}

impl AudioSource {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            looping: false,
            volume: 1.0,
            playing_id: None,
            bus: AudioBus::Sfx,
            spatial: false,
            ref_distance: 1.0,
            max_distance: 50.0,
            rolloff_factor: 1.0,
        }
    }

    pub fn looped(mut self) -> Self {
        self.looping = true;
        self
    }

    pub fn with_volume(mut self, volume: f32) -> Self {
        self.volume = volume;
        self
    }

    /// Set the audio bus for this source.
    pub fn with_bus(mut self, bus: AudioBus) -> Self {
        self.bus = bus;
        self
    }

    /// Enable spatial audio with default spatial parameters.
    pub fn spatial(mut self) -> Self {
        self.spatial = true;
        self
    }

    /// Set the reference distance for spatial fall-off.
    pub fn with_ref_distance(mut self, d: f32) -> Self {
        self.ref_distance = d;
        self
    }

    /// Set the maximum audible distance.
    pub fn with_max_distance(mut self, d: f32) -> Self {
        self.max_distance = d;
        self
    }

    /// Set the rolloff factor.
    pub fn with_rolloff(mut self, r: f32) -> Self {
        self.rolloff_factor = r;
        self
    }
}

/// ECS component for the audio listener (usually the camera/player).
///
/// Attach to the entity whose [`Transform`] represents the player / camera.
#[derive(Debug, Clone, Copy)]
pub struct AudioListener;

pub use plugin::{AudioPlugin, AudioResource, SpatialAudioSystem};
pub use dsp::{
    AudioChannel, AudioMixer, AudioMixerSystem, DopplerSystem, DopplerTracker, ReverbZone,
    ReverbZoneSystem, ConvolutionImpulseResponse, ConvolutionReverb, ConvolutionReverbZone,
    StreamingAudioSource, StreamingAudioSystem, StreamingBuffer, StreamingMode,
    HrtfDatabase, HrtfEntry, HrtfIrPair, HrtfProcessor, HrtfSource, HrtfSystem,
};
