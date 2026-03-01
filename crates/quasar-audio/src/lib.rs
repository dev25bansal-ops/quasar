//! # Quasar Audio
//!
//! Spatial audio system powered by [Kira](https://docs.rs/kira).
//!
//! **Status**: Scaffolded — full implementation coming in Week 2.

use kira::manager::{AudioManager, AudioManagerSettings, backend::DefaultBackend};

/// The engine's audio system — manages sound playback and spatial audio.
pub struct AudioSystem {
    manager: Option<AudioManager>,
}

impl AudioSystem {
    /// Initialize the audio system.
    pub fn new() -> Self {
        let manager = AudioManager::<DefaultBackend>::new(AudioManagerSettings::default()).ok();
        if manager.is_none() {
            log::warn!("Failed to initialize audio backend — audio will be silent");
        }
        Self { manager }
    }

    /// Whether the audio system is functional.
    pub fn is_available(&self) -> bool {
        self.manager.is_some()
    }
}

impl Default for AudioSystem {
    fn default() -> Self {
        Self::new()
    }
}
