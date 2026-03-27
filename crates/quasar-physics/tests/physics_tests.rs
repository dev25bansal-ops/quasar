//! Physics system unit tests

use rapier3d::prelude::*;

#[test]
fn rigid_body_builder_dynamic() {
    let body = RigidBodyBuilder::dynamic()
        .translation(vector![1.0, 2.0, 3.0])
        .build();

    assert_eq!(body.translation().x, 1.0);
    assert_eq!(body.translation().y, 2.0);
    assert_eq!(body.translation().z, 3.0);
}

#[test]
fn rigid_body_builder_static() {
    let body = RigidBodyBuilder::fixed()
        .translation(vector![0.0, 0.0, 0.0])
        .build();

    assert!(!body.is_dynamic());
}

#[test]
fn rigid_body_builder_kinematic() {
    let body = RigidBodyBuilder::kinematic_position_based()
        .translation(vector![0.0, 0.0, 0.0])
        .build();

    assert!(body.is_kinematic());
}

#[test]
fn rigid_body_set_insert() {
    let mut bodies = RigidBodySet::new();
    let body = RigidBodyBuilder::dynamic().build();
    let handle = bodies.insert(body);

    assert!(bodies.contains(handle));
}

#[test]
fn collider_builder_cuboid() {
    let collider = ColliderBuilder::cuboid(1.0, 2.0, 3.0).build();
    assert!(collider.shape().as_cuboid().is_some());
}

#[test]
fn collider_builder_sphere() {
    let collider = ColliderBuilder::ball(1.5).build();
    assert!(collider.shape().as_ball().is_some());
}

#[test]
fn collider_builder_capsule() {
    let collider = ColliderBuilder::capsule_y(2.0, 0.5).build();
    assert!(collider.shape().as_capsule().is_some());
}

#[test]
fn collider_set_insert() {
    let mut bodies = RigidBodySet::new();
    let mut colliders = ColliderSet::new();

    let body = RigidBodyBuilder::dynamic().build();
    let body_handle = bodies.insert(body);

    let collider = ColliderBuilder::cuboid(1.0, 1.0, 1.0).build();
    let collider_handle = colliders.insert_with_parent(collider, body_handle, &mut bodies);

    assert!(colliders.contains(collider_handle));
}

#[test]
fn collider_friction() {
    let collider = ColliderBuilder::cuboid(1.0, 1.0, 1.0).friction(0.5).build();
    assert!((collider.friction() - 0.5).abs() < 0.001);
}

#[test]
fn collider_restitution() {
    let collider = ColliderBuilder::cuboid(1.0, 1.0, 1.0)
        .restitution(0.8)
        .build();
    assert!((collider.restitution() - 0.8).abs() < 0.001);
}

#[test]
fn rigid_body_velocity() {
    let body = RigidBodyBuilder::dynamic()
        .linvel(vector![1.0, 2.0, 3.0])
        .angvel(vector![0.1, 0.2, 0.3])
        .build();

    let linvel = body.linvel();
    assert_eq!(linvel.x, 1.0);
    assert_eq!(linvel.y, 2.0);
    assert_eq!(linvel.z, 3.0);
}

#[test]
fn shared_shape_cuboid() {
    let shape = SharedShape::cuboid(1.0, 2.0, 3.0);
    assert!(shape.as_cuboid().is_some());
}

#[test]
fn shared_shape_ball() {
    let shape = SharedShape::ball(1.5);
    assert!(shape.as_ball().is_some());
}

#[test]
fn impulse_joint_set_new() {
    let joints = ImpulseJointSet::new();
    assert_eq!(joints.len(), 0);
}

#[test]
fn multibody_joint_set_new() {
    let _joints = MultibodyJointSet::new();
}

#[test]
fn island_manager_new() {
    let manager = IslandManager::new();
    assert!(manager.active_dynamic_bodies().is_empty());
}
