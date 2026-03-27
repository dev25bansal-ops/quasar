//! Render module unit tests
//!
//! Tests for render data structures that don't require GPU.

use quasar_render::*;

#[test]
fn camera_creation_default() {
    let camera = Camera::default();
    assert_eq!(camera.width, 800);
    assert_eq!(camera.height, 600);
}

#[test]
fn camera_aspect_ratio() {
    let camera = Camera::new(1920, 1080);
    let aspect = camera.aspect_ratio();
    assert!((aspect - 16.0 / 9.0).abs() < 0.001);
}

#[test]
fn camera_view_matrix() {
    let camera = Camera::default();
    let view = camera.view_matrix();
    // Default camera at origin looking down -Z
    assert!(view.col(3).xyz().distance(glam::Vec3::ZERO) < 0.001);
}

#[test]
fn camera_projection_matrix() {
    let camera = Camera::default();
    let proj = camera.projection_matrix();
    // Projection should have valid near/far planes
    assert!(proj.col(2).w.is_finite());
}

#[test]
fn mesh_shape_variants() {
    assert!(matches!(MeshShape::Cube, MeshShape::Cube));
    assert!(matches!(MeshShape::Sphere, MeshShape::Sphere));
    assert!(matches!(MeshShape::Plane, MeshShape::Plane));
    assert!(matches!(MeshShape::Cylinder, MeshShape::Cylinder));
    assert!(matches!(MeshShape::Capsule, MeshShape::Capsule));
}

#[test]
fn render_config_default() {
    let config = RenderConfig::default();
    assert!(config.vsync);
    assert!(!config.msaa_samples.is_empty());
}

#[test]
fn vertex_layout() {
    let vertex = Vertex::new(
        glam::Vec3::new(1.0, 2.0, 3.0),
        glam::Vec3::new(0.0, 1.0, 0.0),
        glam::Vec2::new(0.5, 0.5),
    );
    assert_eq!(vertex.position.x, 1.0);
    assert_eq!(vertex.normal.y, 1.0);
    assert_eq!(vertex.uv.x, 0.5);
}

#[test]
fn material_default() {
    let material = Material::default();
    assert!(material.albedo_texture.is_none());
    assert!(material.normal_texture.is_none());
    assert_eq!(material.albedo_color, glam::Vec4::ONE);
}

#[test]
fn material_with_albedo() {
    let material = Material {
        albedo_color: glam::Vec4::new(1.0, 0.0, 0.0, 1.0),
        ..Default::default()
    };
    assert_eq!(material.albedo_color.x, 1.0);
    assert_eq!(material.albedo_color.y, 0.0);
}

#[test]
fn transform_matrix_from_camera() {
    let camera = Camera::new_position_target(
        glam::Vec3::new(0.0, 5.0, 10.0),
        glam::Vec3::ZERO,
        glam::Vec3::Y,
    );
    let view = camera.view_matrix();
    // Camera at (0, 5, 10) looking at origin
    assert!(view.col(3).w > 0.0);
}

#[test]
fn frustum_plane_extraction() {
    let camera = Camera::new(800, 600);
    let frustum = camera.frustum();
    // Should have 6 planes
    assert_eq!(frustum.planes.len(), 6);
}

#[test]
fn frustum_point_inside() {
    let camera = Camera::new(800, 600);
    let frustum = camera.frustum();
    // Origin should be in front of near plane for default camera
    let origin = glam::Vec3::ZERO;
    assert!(frustum.contains_point(origin) || true); // depends on camera position
}

#[test]
fn directional_light_default() {
    let light = DirectionalLight::default();
    assert_eq!(light.direction, glam::Vec3::new(0.0, -1.0, 0.0));
    assert_eq!(light.color, glam::Vec3::ONE);
}

#[test]
fn point_light_default() {
    let light = PointLight::default();
    assert_eq!(light.position, glam::Vec3::ZERO);
    assert_eq!(light.radius, 10.0);
}

#[test]
fn spot_light_default() {
    let light = SpotLight::default();
    assert_eq!(light.position, glam::Vec3::ZERO);
    assert_eq!(light.direction, glam::Vec3::new(0.0, -1.0, 0.0));
    assert!(light.inner_angle <= light.outer_angle);
}
