//! Math types unit tests

use quasar_math::*;

#[test]
fn transform_identity() {
    let t = Transform::IDENTITY;
    assert_eq!(t.position, Vec3::ZERO);
    assert_eq!(t.rotation, Quat::IDENTITY);
    assert_eq!(t.scale, Vec3::ONE);
}

#[test]
fn transform_matrix() {
    let t = Transform {
        position: Vec3::new(1.0, 2.0, 3.0),
        rotation: Quat::IDENTITY,
        scale: Vec3::ONE,
    };

    let matrix = t.matrix();
    assert_eq!(matrix.col(3).x, 1.0);
    assert_eq!(matrix.col(3).y, 2.0);
    assert_eq!(matrix.col(3).z, 3.0);
}

#[test]
fn transform_from_position() {
    let t = Transform::from_position(Vec3::new(5.0, 10.0, 15.0));
    assert_eq!(t.position.x, 5.0);
    assert_eq!(t.position.y, 10.0);
    assert_eq!(t.position.z, 15.0);
}

#[test]
fn transform_from_scale() {
    let t = Transform::from_scale(2.0);
    assert_eq!(t.scale.x, 2.0);
    assert_eq!(t.scale.y, 2.0);
    assert_eq!(t.scale.z, 2.0);
}

#[test]
fn transform_forward() {
    let t = Transform::IDENTITY;
    let forward = t.forward();
    assert!((forward.z - (-1.0)).abs() < 0.001);
}

#[test]
fn color_rgb() {
    let c = Color::rgb(1.0, 0.5, 0.0);
    assert_eq!(c.r, 1.0);
    assert_eq!(c.g, 0.5);
    assert_eq!(c.b, 0.0);
    assert_eq!(c.a, 1.0);
}

#[test]
fn color_rgba() {
    let c = Color::rgba(1.0, 0.5, 0.0, 0.5);
    assert_eq!(c.a, 0.5);
}

#[test]
fn color_constants() {
    assert_eq!(Color::WHITE.r, 1.0);
    assert_eq!(Color::BLACK.r, 0.0);
    assert_eq!(Color::RED.r, 1.0);
    assert_eq!(Color::GREEN.g, 1.0);
    assert_eq!(Color::BLUE.b, 1.0);
}

#[test]
fn vec3_operations() {
    let a = Vec3::new(1.0, 2.0, 3.0);
    let b = Vec3::new(4.0, 5.0, 6.0);

    let sum = a + b;
    assert_eq!(sum.x, 5.0);
    assert_eq!(sum.y, 7.0);
    assert_eq!(sum.z, 9.0);
}

#[test]
fn vec3_length() {
    let v = Vec3::new(3.0, 4.0, 0.0);
    assert!((v.length() - 5.0).abs() < 0.001);
}

#[test]
fn vec3_normalize() {
    let v = Vec3::new(3.0, 4.0, 0.0);
    let n = v.normalize();
    assert!((n.length() - 1.0).abs() < 0.001);
}

#[test]
fn vec3_dot() {
    let a = Vec3::new(1.0, 0.0, 0.0);
    let b = Vec3::new(1.0, 0.0, 0.0);
    assert_eq!(a.dot(b), 1.0);
}

#[test]
fn vec3_cross() {
    let a = Vec3::new(1.0, 0.0, 0.0);
    let b = Vec3::new(0.0, 1.0, 0.0);
    let c = a.cross(b);
    assert_eq!(c.z, 1.0);
}

#[test]
fn quat_identity() {
    let q = Quat::IDENTITY;
    let v = q * Vec3::new(1.0, 2.0, 3.0);
    assert_eq!(v.x, 1.0);
    assert_eq!(v.y, 2.0);
    assert_eq!(v.z, 3.0);
}

#[test]
fn quat_rotation() {
    let q = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
    let v = q * Vec3::new(1.0, 0.0, 0.0);
    assert!((v.y - 1.0).abs() < 0.001);
}

#[test]
fn global_transform_default() {
    let gt = GlobalTransform::default();
    assert_eq!(gt.translation(), Vec3::ZERO);
}

#[test]
fn global_transform_from_transform() {
    let t = Transform::from_position(Vec3::new(1.0, 2.0, 3.0));
    let gt = GlobalTransform::from(t);
    assert!((gt.translation().x - 1.0).abs() < 0.001);
}
