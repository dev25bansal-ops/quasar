//! Audio plugin — integrates the audio system with the ECS.

use quasar_core::ecs::{System, World};

use crate::{AudioSource, AudioSystem};

/// Resource wrapper for the audio system as an ECS singleton.
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
        // Collect audio sources that need to start playing.
        let sources_to_play: Vec<(u32, String, bool)> = world
            .query::<AudioSource>()
            .filter(|(_, src)| src.playing_id.is_none())
            .map(|(e, src)| (e.index(), src.path.clone(), src.looping))
            .collect();

        if sources_to_play.is_empty() {
            return;
        }

        // Get the audio resource.
        let audio_ptr: Option<*mut AudioSystem> = world
            .query_mut::<AudioResource>()
            .next()
            .map(|(_, res)| &mut res.audio as *mut AudioSystem);

        let Some(audio_ptr) = audio_ptr else {
            return;
        };

        // Play each source.
        for (entity_idx, path, looping) in sources_to_play {
            // SAFETY: we're done borrowing AudioResource, now we borrow AudioSource.
            let audio = unsafe { &mut *audio_ptr };
            let id = if looping {
                audio.play_looped(&path)
            } else {
                audio.play(&path)
            };

            // Write back the playing_id.
            for (entity, src) in world.query_mut::<AudioSource>() {
                if entity.index() == entity_idx {
                    src.playing_id = id;
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
        let singleton = app.world.spawn();
        app.world.insert(singleton, AudioResource::new());

        app.schedule.add_system(
            quasar_core::ecs::SystemStage::PostUpdate,
            Box::new(AudioPlaybackSystem),
        );

        log::info!("AudioPlugin loaded — Kira audio system active");
    }
}
