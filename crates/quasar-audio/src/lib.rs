//! # Quasar Audio
//!
//! Audio playback system powered by [Kira](https://docs.rs/kira).
//!
//! Supports one-shot sound effects, looping music, volume/playback-rate
//! control, and a basic spatial audio model.

pub mod plugin;

use std::collections::HashMap;
use std::path::Path;

use kira::manager::{backend::DefaultBackend, AudioManager, AudioManagerSettings};
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle};
use kira::tween::Tween;

/// Unique identifier for a loaded sound.
pub type SoundId = u64;

/// The engine's audio system — manages sound playback and mixing.
pub struct AudioSystem {
    manager: Option<AudioManager>,
    next_id: SoundId,
    handles: HashMap<SoundId, StaticSoundHandle>,
}

impl AudioSystem {
    /// Initialize the audio backend.
    pub fn new() -> Self {
        let manager = AudioManager::<DefaultBackend>::new(AudioManagerSettings::default()).ok();
        if manager.is_none() {
            log::warn!("Failed to initialize audio backend — audio will be silent");
        }
        Self {
            manager,
            next_id: 1,
            handles: HashMap::new(),
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
        let manager = self.manager.as_mut()?;
        let data = StaticSoundData::from_file(path).ok()?;
        let handle = manager.play(data).ok()?;
        let id = self.next_id;
        self.next_id += 1;
        self.handles.insert(id, handle);
        Some(id)
    }

    /// Play a sound in a loop.
    pub fn play_looped<P: AsRef<Path>>(&mut self, path: P) -> Option<SoundId> {
        let manager = self.manager.as_mut()?;
        let mut data = StaticSoundData::from_file(path).ok()?;
        data.settings.loop_region = Some(kira::sound::Region::default());
        let handle = manager.play(data).ok()?;
        let id = self.next_id;
        self.next_id += 1;
        self.handles.insert(id, handle);
        Some(id)
    }

    /// Pause a playing sound.
    pub fn pause(&mut self, id: SoundId) {
        if let Some(handle) = self.handles.get_mut(&id) {
            handle.pause(Tween::default());
        }
    }

    /// Resume a paused sound.
    pub fn resume(&mut self, id: SoundId) {
        if let Some(handle) = self.handles.get_mut(&id) {
            handle.resume(Tween::default());
        }
    }

    /// Stop a sound and free its handle.
    pub fn stop(&mut self, id: SoundId) {
        if let Some(mut handle) = self.handles.remove(&id) {
            handle.stop(Tween::default());
        }
    }

    /// Stop all sounds.
    pub fn stop_all(&mut self) {
        for (_, mut handle) in self.handles.drain() {
            handle.stop(Tween::default());
        }
    }

    /// Set the volume of a sound (0.0 = silent, 1.0 = full).
    pub fn set_volume(&mut self, id: SoundId, volume: f64) {
        if let Some(handle) = self.handles.get_mut(&id) {
            handle.set_volume(kira::Volume::Amplitude(volume), Tween::default());
        }
    }

    /// Set the playback rate (1.0 = normal, 2.0 = double speed).
    pub fn set_playback_rate(&mut self, id: SoundId, rate: f64) {
        if let Some(handle) = self.handles.get_mut(&id) {
            handle.set_playback_rate(rate, Tween::default());
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

        handle.set_volume(kira::Volume::Amplitude(gain as f64), Tween::default());
        handle.set_panning(panning, Tween::default());
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
    pub playing_id: Option<SoundId>,
    /// Enable spatial (3D positional) audio for this source.
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
