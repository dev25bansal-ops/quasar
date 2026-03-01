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
}

impl AudioSource {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            looping: false,
            volume: 1.0,
            playing_id: None,
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
}

/// ECS component for the audio listener (usually the camera/player).
#[derive(Debug, Clone, Copy)]
pub struct AudioListener;

pub use plugin::AudioPlugin;
