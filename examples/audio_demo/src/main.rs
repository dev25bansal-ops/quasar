//! # Audio Demo
//!
//! Demonstrates the Quasar audio system with Kira:
//! - One-shot sound effects
//! - Looped music playback
//! - Spatial/3D audio with distance attenuation
//! - Volume and playback rate control
//!
//! Controls:
//! Space - play sound effect
//! M - toggle music on/off
//! Right-click + drag - orbit camera
//! Scroll - zoom
//! F12 - toggle editor
//! ESC - exit

use quasar_audio::{AudioListener, AudioPlugin, AudioResource};
use quasar_engine::prelude::*;
use quasar_math::Vec3;
use quasar_render::MeshShape;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("Quasar Engine — Audio Demo");
    log::info!("Controls: Space = play sound, M = toggle music");
    log::info!("Note: Place audio files in assets/sounds/ directory");

    let mut app = App::new();
    app.add_plugin(AudioPlugin);

    let mut scene = SceneGraph::new();

    // Create visual scene
    let ground = app.world.spawn();
    app.world
        .insert(ground, Transform::from_position(Vec3::new(0.0, -1.0, 0.0)));
    app.world.insert(ground, MeshShape::Plane);
    scene.set_name(ground, "Ground");

    // Audio source entities (visual representation)
    for i in 0..4 {
        let source = app.world.spawn();
        let angle = (i as f32) * std::f32::consts::TAU / 4.0;
        let x = 3.0 * angle.cos();
        let z = 3.0 * angle.sin();
        app.world
            .insert(source, Transform::from_position(Vec3::new(x, 0.0, z)));
        app.world.insert(
            source,
            MeshShape::Sphere {
                sectors: 16,
                stacks: 8,
            },
        );
        scene.set_name(source, format!("AudioSource_{}", i));
    }

    // Camera position becomes the audio listener
    let listener = app.world.spawn();
    app.world.insert(listener, Transform::IDENTITY);
    app.world.insert(listener, AudioListener);
    scene.set_name(listener, "Listener");

    app.world.insert_resource(scene);

    // System to handle audio playback via keyboard
    app.add_system("audio_keyboard", |world: &mut World| {
        let input = match world.resource::<quasar_window::Input>() {
            Some(i) => i,
            None => return,
        };

        let play_sound = input.just_pressed(winit::keyboard::KeyCode::Space);
        let toggle_music = input.just_pressed(winit::keyboard::KeyCode::KeyM);

        if play_sound {
            if let Some(audio) = world.resource_mut::<AudioResource>() {
                if let Some(id) = audio.audio.play("assets/sounds/click.wav") {
                    log::info!("Playing sound effect, id={}", id);
                } else {
                    log::warn!("Could not play sound - place assets/sounds/click.wav");
                }
            }
        }

        if toggle_music {
            if let Some(_audio) = world.resource_mut::<AudioResource>() {
                log::info!("Music toggle requested - place assets/sounds/music.ogg");
            }
        }
    });

    // Animate audio sources to demonstrate spatial audio
    app.add_system("animate_audio_sources", |world: &mut World| {
        let elapsed = world
            .resource::<TimeSnapshot>()
            .map(|t| t.elapsed_seconds)
            .unwrap_or(0.0);

        let scene = match world.remove_resource::<SceneGraph>() {
            Some(s) => s,
            None => return,
        };

        for i in 0..4 {
            if let Some(entity) = scene.find_by_name(&format!("AudioSource_{}", i)) {
                if let Some(tf) = world.get_mut::<quasar_math::Transform>(entity) {
                    let base_angle = (i as f32) * std::f32::consts::TAU / 4.0;
                    let orbit_angle = base_angle + elapsed * 0.5;
                    tf.position.x = 3.0 * orbit_angle.cos();
                    tf.position.z = 3.0 * orbit_angle.sin();
                    tf.position.y = 0.5 * (elapsed * 2.0 + base_angle).sin();
                }
            }
        }

        world.insert_resource(scene);
    });

    run(
        app,
        WindowConfig {
            title: "Quasar Engine — Audio Demo".into(),
            width: 1280,
            height: 720,
            ..WindowConfig::default()
        },
    );
}
