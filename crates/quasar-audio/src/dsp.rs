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

        // Apply wet mix as a volume boost to all spatial sources.
        let spatial: Vec<SoundId> = world
            .query::<AudioSource>()
            .into_iter()
            .filter_map(|(_, src)| {
                if src.spatial { src.playing_id } else { None }
            })
            .collect();

        if let Some(resource) = world.resource_mut::<AudioResource>() {
            for sid in spatial {
                // Read the current volume would require handles — instead we
                // just bump by wet_mix. The main spatial system will reset next
                // frame so this won't accumulate.
                resource
                    .audio
                    .set_volume(sid, 1.0 + max_wet as f64);
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
