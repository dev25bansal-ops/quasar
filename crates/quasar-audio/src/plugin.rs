//! Audio plugin — integrates the audio system with the ECS.

use quasar_core::ecs::{System, World};

use crate::{AudioSource, AudioSystem};

/// Resource wrapper for the audio system as an ECS global resource.
pub struct AudioResource {
    pub audio: AudioSystem,
}

impl AudioResource {
    pub fn new() -> Self {
        Self {
            audio: AudioSystem::new(),
        }
    }
}

impl Default for AudioResource {
    fn default() -> Self {
        Self::new()
    }
}

/// System that plays/stops audio sources based on their ECS components.
pub struct AudioPlaybackSystem;

impl System for AudioPlaybackSystem {
    fn name(&self) -> &str {
        "audio_playback"
    }

    fn run(&mut self, world: &mut World) {
        // Pass 1: collect audio sources that need to start playing.
        let sources_to_play: Vec<(u32, String, bool)> = world
            .query::<AudioSource>()
            .filter(|(_, src)| src.playing_id.is_none())
            .map(|(e, src)| (e.index(), src.path.clone(), src.looping))
            .collect();

        if sources_to_play.is_empty() {
            return;
        }

        // Pass 2: play each source through the audio resource.
        let mut play_results: Vec<(u32, Option<crate::SoundId>)> = Vec::new();

        if let Some(resource) = world.resource_mut::<AudioResource>() {
            for (entity_idx, path, looping) in &sources_to_play {
                let id = if *looping {
                    resource.audio.play_looped(path)
                } else {
                    resource.audio.play(path)
                };
                play_results.push((*entity_idx, id));
            }
        }

        // Pass 3: write back the playing_id to each AudioSource component.
        for (entity_idx, sound_id) in play_results {
            for (entity, src) in world.query_mut::<AudioSource>() {
                if entity.index() == entity_idx {
                    src.playing_id = sound_id;
                    break;
                }
            }
        }
    }
}

/// Plugin that registers the audio system and playback system.
pub struct AudioPlugin;

impl quasar_core::Plugin for AudioPlugin {
    fn name(&self) -> &str {
        "AudioPlugin"
    }

    fn build(&self, app: &mut quasar_core::App) {
        app.world.insert_resource(AudioResource::new());

        app.schedule.add_system(
            quasar_core::ecs::SystemStage::PostUpdate,
            Box::new(AudioPlaybackSystem),
        );

        log::info!("AudioPlugin loaded — Kira audio system active");
    }
}
