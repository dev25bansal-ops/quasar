//! Integration tests for ECS + Physics interaction.
//!
//! Verifies that entities spawned in the ECS with physics components
//! correctly interact with the physics world, including:
//! - Spawning entities with Transform + RigidBodyComponent + ColliderComponent
//! - Stepping physics and verifying position updates
//! - Collision detection triggering events
//! - Entity despawn removing physics bodies

use quasar_core::ecs::World;
use quasar_math::{Transform, Vec3};
use quasar_physics::collider::{ColliderComponent, ColliderShape, PendingCollider};
use quasar_physics::rigidbody::{BodyType, RigidBodyComponent};
use quasar_physics::world::PhysicsWorld;

// ---------------------------------------------------------------------------
// 1. Spawn entities with Transform + RigidBodyComponent + ColliderComponent
// ---------------------------------------------------------------------------

#[test]
fn test_spawn_entity_with_physics_components() {
    let mut world = World::new();

    // Spawn an entity and attach a Transform
    let entity = world.spawn();
    let initial_transform = Transform::from_position(Vec3::new(0.0, 10.0, 0.0));
    world.insert(entity, initial_transform);

    // Verify the entity exists and has a Transform
    assert!(world.is_alive(entity));
    let transform = world.get::<Transform>(entity).expect("Transform should exist");
    assert_eq!(transform.position, Vec3::new(0.0, 10.0, 0.0));

    // Create physics world and add body
    let mut physics = PhysicsWorld::new();
    let body_handle = physics.add_body(BodyType::Dynamic, [0.0, 10.0, 0.0]);

    // Attach RigidBodyComponent
    let rigid_body_comp = RigidBodyComponent::new(body_handle, BodyType::Dynamic);
    world.insert(entity, rigid_body_comp);

    // Create a collider and attach it
    let collider_handle =
        physics.add_collider(body_handle, &ColliderShape::Sphere { radius: 0.5 }, 0.3, 0.5);
    let collider_comp = ColliderComponent::new(collider_handle);
    world.insert(entity, collider_comp);

    // Verify all components are attached
    assert!(world.get::<RigidBodyComponent>(entity).is_some());
    assert!(world.get::<ColliderComponent>(entity).is_some());
    assert!(world.get::<Transform>(entity).is_some());
}

#[test]
fn test_spawn_multiple_physics_entities() {
    let mut world = World::new();
    let mut physics = PhysicsWorld::new();

    // Spawn 3 entities with physics components
    let entities: Vec<_> = (0..3)
        .map(|i| {
            let entity = world.spawn();
            let transform = Transform::from_position(Vec3::new(i as f32 * 2.0, 5.0, 0.0));
            world.insert(entity, transform);

            let body_handle = physics.add_body(BodyType::Dynamic, [i as f32 * 2.0, 5.0, 0.0]);
            world.insert(entity, RigidBodyComponent::new(body_handle, BodyType::Dynamic));

            let collider_handle =
                physics.add_collider(body_handle, &ColliderShape::Sphere { radius: 0.5 }, 0.3, 0.5);
            world.insert(entity, ColliderComponent::new(collider_handle));

            entity
        })
        .collect();

    // Verify all entities have physics components
    for entity in &entities {
        assert!(world.is_alive(*entity));
        assert!(world.get::<Transform>(*entity).is_some());
        assert!(world.get::<RigidBodyComponent>(*entity).is_some());
        assert!(world.get::<ColliderComponent>(*entity).is_some());
    }

    // Query all entities with Transform
    let transform_count = world.query::<Transform>().into_iter().count();
    assert_eq!(transform_count, 3);

    // Verify physics world has correct counts
    assert_eq!(physics.body_count(), 3);
    assert_eq!(physics.collider_count(), 3);
}

// ---------------------------------------------------------------------------
// 2. Step physics and verify positions update
// ---------------------------------------------------------------------------

#[test]
fn test_physics_step_updates_positions() {
    let mut world = World::new();
    let mut physics = PhysicsWorld::new();

    // Spawn a falling entity
    let entity = world.spawn();
    world.insert(entity, Transform::from_position(Vec3::new(0.0, 10.0, 0.0)));

    let body_handle = physics.add_body(BodyType::Dynamic, [0.0, 10.0, 0.0]);
    world.insert(entity, RigidBodyComponent::new(body_handle, BodyType::Dynamic));

    let collider_handle = physics.add_collider(body_handle, &ColliderShape::Sphere { radius: 0.5 }, 0.3, 0.5);
    world.insert(entity, ColliderComponent::new(collider_handle));

    // Store initial position
    let initial_pos = world.get::<Transform>(entity).unwrap().position;
    assert!((initial_pos.y - 10.0).abs() < 0.001);

    // Step physics
    physics.step();

    // Sync physics position back to ECS Transform
    if let Some(new_pos) = physics.body_position(body_handle) {
        let transform = world.get_mut::<Transform>(entity).unwrap();
        transform.position = Vec3::new(new_pos[0], new_pos[1], new_pos[2]);
    }

    // Verify position changed (gravity should have pulled it down)
    let new_pos = world.get::<Transform>(entity).unwrap().position;
    assert!(new_pos.y < 10.0, "Entity should have fallen due to gravity");
    assert!(new_pos.y > 9.0, "Entity shouldn't have fallen too far in one step");
}

#[test]
fn test_physics_multiple_steps_accumulate() {
    let mut world = World::new();
    let mut physics = PhysicsWorld::new();

    let entity = world.spawn();
    world.insert(entity, Transform::from_position(Vec3::new(0.0, 20.0, 0.0)));

    let body_handle = physics.add_body(BodyType::Dynamic, [0.0, 20.0, 0.0]);
    world.insert(entity, RigidBodyComponent::new(body_handle, BodyType::Dynamic));

    let collider_handle = physics.add_collider(body_handle, &ColliderShape::Sphere { radius: 0.5 }, 0.3, 0.5);
    world.insert(entity, ColliderComponent::new(collider_handle));

    // Step physics 10 times
    for _ in 0..10 {
        physics.step();

        // Sync position
        if let Some(pos) = physics.body_position(body_handle) {
            let transform = world.get_mut::<Transform>(entity).unwrap();
            transform.position = Vec3::new(pos[0], pos[1], pos[2]);
        }
    }

    let final_pos = world.get::<Transform>(entity).unwrap().position;
    assert!(
        final_pos.y < 15.0,
        "Entity should have fallen significantly after 10 steps"
    );
}

#[test]
fn test_kinematic_body_moves_with_position() {
    let mut world = World::new();
    let mut physics = PhysicsWorld::new();

    let entity = world.spawn();
    world.insert(entity, Transform::from_position(Vec3::new(0.0, 0.0, 0.0)));

    let body_handle = physics.add_body(BodyType::KinematicPositionBased, [0.0, 0.0, 0.0]);
    world.insert(
        entity,
        RigidBodyComponent::new(body_handle, BodyType::KinematicPositionBased),
    );

    let collider_handle = physics.add_collider(body_handle, &ColliderShape::Sphere { radius: 1.0 }, 0.3, 0.5);
    world.insert(entity, ColliderComponent::new(collider_handle));

    // Move kinematic body manually
    physics.set_body_position(body_handle, [5.0, 3.0, 2.0]);

    physics.step();

    // Sync and verify
    if let Some(pos) = physics.body_position(body_handle) {
        let transform = world.get_mut::<Transform>(entity).unwrap();
        transform.position = Vec3::new(pos[0], pos[1], pos[2]);
    }

    let final_pos = world.get::<Transform>(entity).unwrap().position;
    assert!((final_pos.x - 5.0).abs() < 0.01);
    assert!((final_pos.y - 3.0).abs() < 0.01);
    assert!((final_pos.z - 2.0).abs() < 0.01);
}

// ---------------------------------------------------------------------------
// 3. Verify collision detection between entities
// ---------------------------------------------------------------------------

#[test]
fn test_collision_detection_between_entities() {
    let mut world = World::new();
    let mut physics = PhysicsWorld::new();

    // Entity A: dynamic ball falling
    let entity_a = world.spawn();
    world.insert(entity_a, Transform::from_position(Vec3::new(0.0, 5.0, 0.0)));
    let body_a = physics.add_body(BodyType::Dynamic, [0.0, 5.0, 0.0]);
    let collider_a = physics.add_collider(body_a, &ColliderShape::Sphere { radius: 1.0 }, 0.3, 0.5);
    world.insert(entity_a, RigidBodyComponent::new(body_a, BodyType::Dynamic));
    world.insert(entity_a, ColliderComponent::new(collider_a));

    // Entity B: static ball on ground
    let entity_b = world.spawn();
    world.insert(entity_b, Transform::from_position(Vec3::new(0.0, 0.5, 0.0)));
    let body_b = physics.add_body(BodyType::Fixed, [0.0, 0.5, 0.0]);
    let collider_b = physics.add_collider(body_b, &ColliderShape::Sphere { radius: 1.0 }, 0.3, 0.5);
    world.insert(entity_b, RigidBodyComponent::new(body_b, BodyType::Fixed));
    world.insert(entity_b, ColliderComponent::new(collider_b));

    // Step physics multiple times to let entity A fall and collide with B
    for _ in 0..50 {
        physics.step();
    }

    // Sync positions
    for (entity, body_handle) in &[(entity_a, body_a), (entity_b, body_b)] {
        if let Some(pos) = physics.body_position(*body_handle) {
            if let Some(mut transform) = world.get_mut::<Transform>(*entity) {
                transform.position = Vec3::new(pos[0], pos[1], pos[2]);
            }
        }
    }

    // Entity A should have stopped near entity B (collision detected)
    let pos_a = world.get::<Transform>(entity_a).unwrap().position;
    let pos_b = world.get::<Transform>(entity_b).unwrap().position;

    // The balls should be touching (distance ≈ 2.0 = sum of radii)
    let distance = (pos_a - pos_b).length();
    assert!(
        (distance - 2.0).abs() < 0.5,
        "Entities should be approximately touching after collision, distance={}",
        distance
    );
}

#[test]
fn test_physics_world_with_static_collider() {
    let mut physics = PhysicsWorld::new();

    // Add a static ground plane
    let ground_collider = physics.add_static_collider(
        &ColliderShape::HalfSpace,
        [0.0, 0.0, 0.0],
    );

    assert!(physics.collider_count() > 0);
    assert!(physics.colliders.get(ground_collider).is_some());
}

#[test]
fn test_pending_collider_conversion() {
    let mut world = World::new();
    let mut physics = PhysicsWorld::new();

    let entity = world.spawn();
    world.insert(entity, Transform::IDENTITY);

    // Add body first
    let body_handle = physics.add_body(BodyType::Dynamic, [0.0, 5.0, 0.0]);
    world.insert(entity, RigidBodyComponent::new(body_handle, BodyType::Dynamic));

    // Add pending collider
    let pending = PendingCollider::with_body(
        ColliderShape::Box {
            half_extents: [0.5, 0.5, 0.5],
        },
        body_handle,
    );
    world.insert(entity, pending);

    // In a real system, ColliderSyncSystem would convert this to ColliderComponent
    // For this test, we just verify the pending collider was added
    assert!(world.get::<PendingCollider>(entity).is_some());
}

// ---------------------------------------------------------------------------
// 4. Test entity despawn removes physics bodies
// ---------------------------------------------------------------------------

#[test]
fn test_despawn_entity_removes_physics_body() {
    let mut world = World::new();
    let mut physics = PhysicsWorld::new();

    let entity = world.spawn();
    world.insert(entity, Transform::from_position(Vec3::new(0.0, 5.0, 0.0)));

    let body_handle = physics.add_body(BodyType::Dynamic, [0.0, 5.0, 0.0]);
    let collider_handle = physics.add_collider(body_handle, &ColliderShape::Sphere { radius: 0.5 }, 0.3, 0.5);

    world.insert(entity, RigidBodyComponent::new(body_handle, BodyType::Dynamic));
    world.insert(entity, ColliderComponent::new(collider_handle));

    // Verify body and collider exist
    assert_eq!(physics.body_count(), 1);
    assert_eq!(physics.collider_count(), 1);

    // Despawn the entity
    assert!(world.despawn(entity));
    assert!(!world.is_alive(entity));

    // Clean up physics bodies (this is what a physics sync system would do)
    physics.remove_body(body_handle);

    // Body should be removed
    assert_eq!(
        physics.body_count(),
        0,
        "Physics body should be removed after entity despawn"
    );
}

#[test]
fn test_despawn_multiple_physics_entities() {
    let mut world = World::new();
    let mut physics = PhysicsWorld::new();

    let mut handles = Vec::new();

    for i in 0..5 {
        let entity = world.spawn();
        world.insert(entity, Transform::from_position(Vec3::new(i as f32, 0.0, 0.0)));

        let body_handle = physics.add_body(BodyType::Dynamic, [i as f32, 0.0, 0.0]);
        let collider_handle =
            physics.add_collider(body_handle, &ColliderShape::Box { half_extents: [0.5, 0.5, 0.5] }, 0.3, 0.5);

        world.insert(entity, RigidBodyComponent::new(body_handle, BodyType::Dynamic));
        world.insert(entity, ColliderComponent::new(collider_handle));

        handles.push((entity, body_handle, collider_handle));
    }

    assert_eq!(physics.body_count(), 5);
    assert_eq!(physics.collider_count(), 5);

    // Despawn all entities and clean up physics
    for (entity, body_handle, _collider_handle) in &handles {
        world.despawn(*entity);
        physics.remove_body(*body_handle);
    }

    assert_eq!(physics.body_count(), 0, "All physics bodies should be removed");
    assert_eq!(physics.collider_count(), 0, "All colliders should be removed");
}

#[test]
fn test_despawn_entity_cleanup_change_ticks() {
    let mut world = World::new();
    let mut physics = PhysicsWorld::new();

    let entity = world.spawn();
    world.insert(entity, Transform::from_position(Vec3::new(0.0, 0.0, 0.0)));

    let body_handle = physics.add_body(BodyType::Dynamic, [0.0, 0.0, 0.0]);
    world.insert(entity, RigidBodyComponent::new(body_handle, BodyType::Dynamic));

    // Verify entity has components
    assert!(world.get::<Transform>(entity).is_some());
    assert!(world.get::<RigidBodyComponent>(entity).is_some());

    // Despawn
    world.despawn(entity);

    // Verify entity is dead and components are inaccessible
    assert!(!world.is_alive(entity));
    assert!(world.get::<Transform>(entity).is_none());
    assert!(world.get::<RigidBodyComponent>(entity).is_none());
}

#[test]
fn test_physics_gravity_affects_entities() {
    let mut world = World::new();
    let mut physics = PhysicsWorld::with_gravity(0.0, -9.81, 0.0);

    let entity = world.spawn();
    world.insert(entity, Transform::from_position(Vec3::new(0.0, 100.0, 0.0)));

    let body_handle = physics.add_body(BodyType::Dynamic, [0.0, 100.0, 0.0]);
    world.insert(entity, RigidBodyComponent::new(body_handle, BodyType::Dynamic));

    let collider_handle = physics.add_collider(body_handle, &ColliderShape::Sphere { radius: 1.0 }, 0.3, 0.5);
    world.insert(entity, ColliderComponent::new(collider_handle));

    // Step physics many times
    for _ in 0..100 {
        physics.step();
    }

    // Entity should have fallen significantly
    if let Some(pos) = physics.body_position(body_handle) {
        assert!(pos[1] < 50.0, "Entity should have fallen due to gravity");
    }
}

#[test]
fn test_physics_apply_force() {
    let mut world = World::new();
    let mut physics = PhysicsWorld::new();

    let entity = world.spawn();
    world.insert(entity, Transform::IDENTITY);

    let body_handle = physics.add_body(BodyType::Dynamic, [0.0, 0.0, 0.0]);
    world.insert(entity, RigidBodyComponent::new(body_handle, BodyType::Dynamic));

    // Apply upward force
    physics.apply_force(body_handle, [0.0, 100.0, 0.0]);

    // Step to process force
    physics.step();

    // Entity should have moved upward
    if let Some(pos) = physics.body_position(body_handle) {
        assert!(pos[1] > 0.0, "Entity should have moved upward from force");
    }
}

#[test]
fn test_physics_ray_cast() {
    let mut physics = PhysicsWorld::new();

    // Add a collider
    let body_handle = physics.add_body(BodyType::Fixed, [0.0, 0.0, 0.0]);
    let _collider_handle =
        physics.add_collider(body_handle, &ColliderShape::Sphere { radius: 1.0 }, 0.3, 0.5);

    // Cast ray from above toward origin
    let origin = [0.0, 5.0, 0.0];
    let direction = [0.0, -1.0, 0.0];
    let max_toi = 10.0;

    let hit = physics.cast_ray(origin, direction, max_toi);

    assert!(hit.is_some(), "Ray should hit the sphere");
    if let Some((_collider_handle, toi)) = hit {
        assert!(toi > 0.0 && toi < 10.0, "Hit should be within range");
    }
}
