//! Audio plugin — integrates the audio system with the ECS.

use quasar_core::ecs::{Entity, System, World};
use quasar_math::{Quat, Transform, Vec3};

use crate::{AudioBus, AudioListener, AudioSource, AudioSystem};

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
        if !quasar_core::simulation_active(world) { return; }
        if let Some(p) = world.resource_mut::<quasar_core::Profiler>() { p.begin_scope("audio_playback"); }
        // Pass 1: collect audio sources that need to start playing.
        let sources_to_play: Vec<(u32, String, bool, AudioBus)> = world
            .query::<AudioSource>()
            .into_iter()
            .filter(|(_, src)| src.playing_id.is_none())
            .map(|(e, src)| (e.index(), src.path.clone(), src.looping, src.bus.clone()))
            .collect();

        if sources_to_play.is_empty() {
            return;
        }

        // Pass 2: play each source through the audio resource.
        let mut play_results: Vec<(u32, Option<crate::SoundId>)> = Vec::new();

        if let Some(resource) = world.resource_mut::<AudioResource>() {
            for (entity_idx, path, looping, bus) in &sources_to_play {
                let id = if *looping {
                    resource.audio.play_looped_on_bus(path, bus)
                } else {
                    resource.audio.play_on_bus(path, bus)
                };
                play_results.push((*entity_idx, id));
            }
        }

        // Pass 3: write back the playing_id to each AudioSource component.
        for (entity_idx, sound_id) in play_results {
            world.for_each_mut(|entity: Entity, src: &mut AudioSource| {
                if entity.index() == entity_idx {
                    src.playing_id = sound_id;
                }
            });
        }
        if let Some(p) = world.resource_mut::<quasar_core::Profiler>() { p.end_scope("audio_playback"); }
    }
}

// ---------------------------------------------------------------------------
// Spatial audio
// ---------------------------------------------------------------------------

/// System that adjusts volume and stereo panning of spatial audio sources
/// based on their distance and direction relative to the [`AudioListener`].
///
/// The listener entity must carry both [`AudioListener`] and [`Transform`].
/// Each spatial [`AudioSource`] entity must also carry a [`Transform`].
///
/// **Distance model** — inverse-distance clamped (OpenAL-style).
/// **Panning** — dot product of the direction vector with the listener's
/// local right axis, mapped to kira's 0.0 (left) – 1.0 (right) range.
pub struct SpatialAudioSystem;

impl System for SpatialAudioSystem {
    fn name(&self) -> &str {
        "spatial_audio"
    }

    fn run(&mut self, world: &mut World) {
        if !quasar_core::simulation_active(world) { return; }
        if let Some(p) = world.resource_mut::<quasar_core::Profiler>() { p.begin_scope("spatial_audio"); }
        // 1. Find the listener position and orientation.
        let listener: Option<(Vec3, Quat)> = world
            .query::<AudioListener>()
            .into_iter()
            .filter_map(|(entity, _)| {
                let t = world.get::<Transform>(entity)?;
                Some((t.position, t.rotation))
            })
            .next();

        let (listener_pos, listener_rot) = match listener {
            Some(lr) => lr,
            None => return, // no listener — nothing to do
        };

        let listener_right = listener_rot * Vec3::X;

        // 2. Collect spatial sources that are currently playing.
        let spatial_sources: Vec<(u32, crate::SoundId, f32, f32, f32, f32, Vec3)> = world
            .query::<AudioSource>()
            .into_iter()
            .filter_map(|(entity, src)| {
                if !src.spatial {
                    return None;
                }
                let sound_id = src.playing_id?;
                let t = world.get::<Transform>(entity)?;
                Some((
                    entity.index(),
                    sound_id,
                    src.volume,
                    src.ref_distance,
                    src.max_distance,
                    src.rolloff_factor,
                    t.position,
                ))
            })
            .collect();

        if spatial_sources.is_empty() {
            return;
        }

        // 3. Update each spatial source through the audio resource.
        if let Some(resource) = world.resource_mut::<AudioResource>() {
            for (_entity_idx, sound_id, volume, ref_dist, max_dist, rolloff, source_pos) in
                &spatial_sources
            {
                let diff = *source_pos - listener_pos;
                let distance = diff.length();

                // Compute stereo panning from the listener-relative direction.
                let panning: f64 = if distance < 1e-4 {
                    0.5
                } else {
                    let dir = diff / distance;
                    let dot = dir.dot(listener_right);
                    (0.5 + 0.5 * dot as f64).clamp(0.0, 1.0)
                };

                // Build a temporary AudioSource-like view for the update helper.
                let tmp = AudioSource {
                    path: String::new(),
                    looping: false,
                    volume: *volume,
                    playing_id: None,
                    spatial: true,
                    ref_distance: *ref_dist,
                    max_distance: *max_dist,
                    rolloff_factor: *rolloff,
                    bus: AudioBus::Sfx,
                };

                resource
                    .audio
                    .update_spatial(*sound_id, &tmp, distance, panning);
            }
        }
        if let Some(p) = world.resource_mut::<quasar_core::Profiler>() { p.end_scope("spatial_audio"); }
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

        app.schedule.add_system(
            quasar_core::ecs::SystemStage::PostUpdate,
            Box::new(SpatialAudioSystem),
        );

        log::info!("AudioPlugin loaded — Kira audio system active (spatial enabled)");
    }
}
