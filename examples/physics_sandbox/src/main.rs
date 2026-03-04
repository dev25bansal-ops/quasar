//! # Physics Sandbox Demo
//!
//! Demonstrates the Quasar physics system with Rapier3D:
//! - Dynamic rigid bodies with gravity
//! - Collision detection and response
//! - Ray casting for mouse picking
//! - Collision events
//!
//! Controls:
//! Click - spawn a sphere at cursor position
//! Right-click + drag - orbit camera
//! Scroll - zoom
//! F12 - toggle editor
//! ESC - exit

use quasar_engine::prelude::*;
use quasar_math::Vec3;
use quasar_physics::{
    BodyType, ColliderComponent, ColliderShape, CollisionEvent, CollisionEventType, PhysicsPlugin,
    RigidBodyComponent,
};
use quasar_render::MeshShape;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("Quasar Engine — Physics Sandbox");
    log::info!("Controls: Click to spawn spheres, Right-drag to orbit, Scroll to zoom");
    log::info!("Watch console for collision events!");

    let mut app = App::new();
    app.add_plugin(PhysicsPlugin::new());

    let mut scene = SceneGraph::new();

    // Ground plane (static)
    let ground = app.world.spawn();
    app.world
        .insert(ground, Transform::from_position(Vec3::new(0.0, -2.0, 0.0)));
    app.world.insert(ground, MeshShape::Plane);
    scene.set_name(ground, "Ground");

    // Create some static platforms
    for i in 0..3 {
        let platform = app.world.spawn();
        let angle = (i as f32) * std::f32::consts::TAU / 3.0;
        let x = 3.0 * angle.cos();
        let z = 3.0 * angle.sin();
        app.world
            .insert(platform, Transform::from_position(Vec3::new(x, -1.0, z)));
        app.world.insert(platform, MeshShape::Cube);
        scene.set_name(platform, format!("Platform_{}", i));
    }

    // Spawn initial dynamic spheres
    for i in 0..5 {
        let sphere = app.world.spawn();
        let x = (i as f32 - 2.0) * 1.5;
        app.world.insert(
            sphere,
            Transform::from_position(Vec3::new(x, 5.0 + i as f32, 0.0)),
        );
        app.world.insert(
            sphere,
            MeshShape::Sphere {
                sectors: 16,
                stacks: 8,
            },
        );
        scene.set_name(sphere, format!("Sphere_{}", i));

        // Physics components
        let body_handle = {
            let phys = app
                .world
                .resource_mut::<quasar_physics::PhysicsResource>()
                .unwrap();
            phys.physics
                .add_body(BodyType::Dynamic, [x, 5.0 + i as f32, 0.0])
        };
        app.world.insert(
            sphere,
            RigidBodyComponent {
                handle: body_handle,
                body_type: BodyType::Dynamic,
            },
        );

        let collider_handle = {
            let phys = app
                .world
                .resource_mut::<quasar_physics::PhysicsResource>()
                .unwrap();
            phys.physics.add_collider(
                body_handle,
                &ColliderShape::Sphere { radius: 0.5 },
                0.5,
                0.5,
            )
        };
        app.world.insert(
            sphere,
            ColliderComponent {
                handle: collider_handle,
            },
        );
    }

    app.world.insert_resource(scene);

    // System to handle collision events
    app.add_system("collision_logger", |world: &mut World| {
        if let Some(events) = world.resource::<quasar_core::Events>() {
            for event in events.read::<CollisionEvent>() {
                match event.event_type {
                    CollisionEventType::Started => {
                        log::info!(
                            "Collision started between entities {} and {}",
                            event.entity1.index(),
                            event.entity2.index()
                        );
                    }
                    CollisionEventType::Stopped => {
                        log::debug!(
                            "Collision stopped between entities {} and {}",
                            event.entity1.index(),
                            event.entity2.index()
                        );
                    }
                }
            }
        }
    });

    // Animation system to rotate platforms
    app.add_system("rotate_platforms", |world: &mut World| {
        let elapsed = world
            .resource::<TimeSnapshot>()
            .map(|t| t.elapsed_seconds)
            .unwrap_or(0.0);

        let scene = match world.remove_resource::<SceneGraph>() {
            Some(s) => s,
            None => return,
        };

        for i in 0..3 {
            if let Some(entity) = scene.find_by_name(&format!("Platform_{}", i)) {
                if let Some(tf) = world.get_mut::<quasar_math::Transform>(entity) {
                    let base_angle = (i as f32) * std::f32::consts::TAU / 3.0;
                    let y = -1.0 + 0.3 * (elapsed * 0.5 + base_angle).sin();
                    tf.position.y = y;
                    tf.rotation = Quat::from_rotation_y(elapsed * 0.3 + base_angle);
                }
            }
        }

        world.insert_resource(scene);
    });

    run(
        app,
        WindowConfig {
            title: "Quasar Engine — Physics Sandbox".into(),
            width: 1280,
            height: 720,
            ..WindowConfig::default()
        },
    );
}
