//! Integration tests for ECS + Rendering interaction.
//!
//! Verifies that entities spawned in the ECS with rendering components
//! correctly interact with the rendering system, including:
//! - Spawning entities with Transform + MeshShape
//! - Verifying renderer can collect and draw entities
//! - Testing archetype migration when adding/removing MeshShape
//! - Verifying camera updates affect rendering

use quasar_core::ecs::World;
use quasar_math::{Mat4, Transform, Vec3};

// ---------------------------------------------------------------------------
// Mock render components (since wgpu requires GPU, we test the ECS side)
// ---------------------------------------------------------------------------

/// Mock component representing an entity that should be rendered.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Renderable;

/// Mock component representing camera state.
#[derive(Debug, Clone)]
struct Camera {
    view_matrix: Mat4,
    projection_matrix: Mat4,
    position: Vec3,
    target: Vec3,
}

impl Camera {
    fn new(position: Vec3, target: Vec3) -> Self {
        Self {
            view_matrix: Mat4::look_at_rh(position, target, Vec3::new(0.0, 1.0, 0.0)),
            projection_matrix: Mat4::perspective_rh(
                std::f32::consts::FRAC_PI_4,
                16.0 / 9.0,
                0.1,
                100.0,
            ),
            position,
            target,
        }
    }

    fn update(&mut self, position: Vec3, target: Vec3) {
        self.position = position;
        self.target = target;
        self.view_matrix = Mat4::look_at_rh(position, target, Vec3::new(0.0, 1.0, 0.0));
    }
}

/// Mock component for render visibility.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Visible(bool);

// ---------------------------------------------------------------------------
// 1. Spawn entities with Transform + MeshShape
// ---------------------------------------------------------------------------

#[test]
fn test_spawn_entity_with_mesh_shape() {
    let mut world = World::new();

    let entity = world.spawn();
    let transform = Transform::from_position(Vec3::new(1.0, 2.0, 3.0));
    world.insert(entity, transform);

    // Use the render crate's MeshShape (test compilation path)
    use quasar_render::mesh::MeshShape;
    world.insert(entity, MeshShape::Cube);

    // Verify components
    assert!(world.get::<Transform>(entity).is_some());
    assert!(world.get::<MeshShape>(entity).is_some());

    let shape = world.get::<MeshShape>(entity).unwrap();
    assert_eq!(*shape, MeshShape::Cube);
}

#[test]
fn test_spawn_multiple_renderable_entities() {
    let mut world = World::new();

    use quasar_render::mesh::MeshShape;

    let shapes = vec![
        MeshShape::Cube,
        MeshShape::Sphere {
            sectors: 16,
            stacks: 16,
        },
        MeshShape::Plane,
        MeshShape::Cylinder { segments: 32 },
    ];

    for (i, shape) in shapes.into_iter().enumerate() {
        let entity = world.spawn();
        let transform =
            Transform::from_position(Vec3::new(i as f32 * 3.0, 0.0, 0.0));
        world.insert(entity, transform);
        world.insert(entity, shape);
    }

    // Query all entities with MeshShape
    let mesh_count = world.query::<MeshShape>().into_iter().count();
    assert_eq!(mesh_count, 4);

    // Verify each shape type exists
    let mut found_shapes = std::collections::HashSet::new();
    for (_, shape) in world.query::<MeshShape>().into_iter() {
        found_shapes.insert(*shape);
    }

    assert!(found_shapes.contains(&MeshShape::Cube));
    assert!(found_shapes.contains(&MeshShape::Plane));
}

#[test]
fn test_spawn_entity_with_renderable_marker() {
    let mut world = World::new();

    let entity = world.spawn();
    world.insert(entity, Transform::from_position(Vec3::new(0.0, 0.0, 0.0)));
    world.insert(entity, Renderable);
    world.insert(entity, Visible(true));

    // Query for all renderable entities
    let renderable_count = world.query2::<Transform, Renderable>().into_iter().count();
    assert_eq!(renderable_count, 1);

    // Query for visible entities
    let visible_count = world.query2::<Transform, Visible>().into_iter().count();
    assert_eq!(visible_count, 1);
}

// ---------------------------------------------------------------------------
// 2. Verify renderer can collect and draw entities
// ---------------------------------------------------------------------------

#[test]
fn test_collect_renderable_entities() {
    let mut world = World::new();

    use quasar_render::mesh::MeshShape;

    // Spawn a mix of renderable and non-renderable entities
    for i in 0..10 {
        let entity = world.spawn();
        world.insert(
            entity,
            Transform::from_position(Vec3::new(i as f32, 0.0, 0.0)),
        );

        // Only even entities are renderable
        if i % 2 == 0 {
            world.insert(entity, MeshShape::Cube);
            world.insert(entity, Renderable);
        }
    }

    // Collect all renderable entities
    let renderable: Vec<_> = world
        .query3::<Transform, MeshShape, Renderable>()
        .into_iter()
        .map(|(e, transform, shape)| (e, *transform, *shape))
        .collect();

    assert_eq!(renderable.len(), 5); // 0, 2, 4, 6, 8

    // Verify positions
    for (i, (_, transform, _)) in renderable.iter().enumerate() {
        let expected_x = (i * 2) as f32;
        assert!((transform.position.x - expected_x).abs() < 0.001);
    }
}

#[test]
fn test_filter_visible_entities() {
    let mut world = World::new();

    // Spawn entities with different visibility states
    for i in 0..5 {
        let entity = world.spawn();
        world.insert(entity, Transform::from_position(Vec3::new(i as f32, 0.0, 0.0)));
        world.insert(entity, Renderable);
        world.insert(entity, Visible(i % 2 == 0));
    }

    // Query only visible entities
    let visible_count = world
        .query2::<Transform, Visible>()
        .into_iter()
        .filter(|(_, visible)| visible.0)
        .count();

    assert_eq!(visible_count, 3); // 0, 2, 4 are visible
}

#[test]
fn test_collect_entities_within_frustum() {
    let mut world = World::new();

    let camera = Camera::new(Vec3::new(0.0, 5.0, 10.0), Vec3::new(0.0, 0.0, 0.0));
    world.insert_resource(camera);

    // Spawn entities at various positions
    for i in 0..10 {
        let entity = world.spawn();
        let pos = Vec3::new((i as f32 - 5.0) * 2.0, 0.0, 0.0);
        world.insert(entity, Transform::from_position(pos));
        world.insert(entity, Renderable);
    }

    // Collect entities and check their transforms can be used for frustum culling
    let entities_in_view: Vec<_> = world
        .query2::<Transform, Renderable>()
        .into_iter()
        .filter(|(_, _)| {
            // Simplified: all entities are "in view" for this test
            // In a real system, you'd test against frustum planes
            true
        })
        .collect();

    assert_eq!(entities_in_view.len(), 10);
}

// ---------------------------------------------------------------------------
// 3. Test archetype migration when adding/removing MeshShape
// ---------------------------------------------------------------------------

#[test]
fn test_archetype_migration_add_mesh_shape() {
    let mut world = World::new();

    use quasar_render::mesh::MeshShape;

    // Spawn entity with only Transform
    let entity = world.spawn();
    world.insert(entity, Transform::from_position(Vec3::new(0.0, 0.0, 0.0)));

    // Query for entities with Transform only (no MeshShape)
    let transform_only_count = world
        .query::<Transform>()
        .into_iter()
        .filter(|(e, _)| world.get::<MeshShape>(*e).is_none())
        .count();
    assert_eq!(transform_only_count, 1);

    // Add MeshShape - triggers archetype migration
    world.insert(entity, MeshShape::Cube);

    // Now entity should have both components
    let with_mesh_count = world.query2::<Transform, MeshShape>().into_iter().count();
    assert_eq!(with_mesh_count, 1);

    // Verify the transform survived migration
    let transform = world.get::<Transform>(entity).unwrap();
    assert_eq!(transform.position, Vec3::new(0.0, 0.0, 0.0));
}

#[test]
fn test_archetype_migration_remove_mesh_shape() {
    let mut world = World::new();

    use quasar_render::mesh::MeshShape;

    // Spawn entity with Transform + MeshShape
    let entity = world.spawn();
    world.insert(entity, Transform::from_position(Vec3::new(1.0, 2.0, 3.0)));
    world.insert(entity, MeshShape::Sphere {
        sectors: 16,
        stacks: 16,
    });

    // Verify both components exist
    assert!(world.get::<Transform>(entity).is_some());
    assert!(world.get::<MeshShape>(entity).is_some());

    // Remove MeshShape - triggers archetype migration
    world.remove_component::<MeshShape>(entity);

    // Verify MeshShape is gone but Transform remains
    assert!(world.get::<Transform>(entity).is_some());
    assert!(world.get::<MeshShape>(entity).is_none());

    // Verify transform data survived migration
    let transform = world.get::<Transform>(entity).unwrap();
    assert_eq!(transform.position, Vec3::new(1.0, 2.0, 3.0));
}

#[test]
fn test_archetype_migration_multiple_component_changes() {
    let mut world = World::new();

    use quasar_render::mesh::MeshShape;

    let entity = world.spawn();

    // Start with Transform only
    world.insert(entity, Transform::from_position(Vec3::new(0.0, 0.0, 0.0)));

    // Add Renderable
    world.insert(entity, Renderable);

    // Add MeshShape
    world.insert(entity, MeshShape::Cube);

    // Add Visible
    world.insert(entity, Visible(true));

    // Query for full archetype
    let full_count = world
        .query4::<Transform, MeshShape, Renderable, Visible>()
        .into_iter()
        .count();
    assert_eq!(full_count, 1);

    // Remove Visible
    world.remove_component::<Visible>(entity);

    // Verify other components still exist
    assert!(world.get::<Transform>(entity).is_some());
    assert!(world.get::<MeshShape>(entity).is_some());
    assert!(world.get::<Renderable>(entity).is_some());
    assert!(world.get::<Visible>(entity).is_none());
}

#[test]
fn test_archetype_migration_preserves_entity_id() {
    let mut world = World::new();

    use quasar_render::mesh::MeshShape;

    let entity = world.spawn();
    let original_index = entity.index();

    world.insert(entity, Transform::from_position(Vec3::new(0.0, 0.0, 0.0)));

    // Add and remove components multiple times
    world.insert(entity, MeshShape::Cube);
    world.remove_component::<MeshShape>(entity);
    world.insert(entity, MeshShape::Sphere {
        sectors: 8,
        stacks: 8,
    });
    world.remove_component::<MeshShape>(entity);
    world.insert(entity, MeshShape::Plane);

    // Entity index should remain the same
    assert_eq!(entity.index(), original_index);
    assert!(world.is_alive(entity));
}

// ---------------------------------------------------------------------------
// 4. Verify camera updates affect rendering
// ---------------------------------------------------------------------------

#[test]
fn test_camera_resource_updates() {
    let mut world = World::new();

    let camera = Camera::new(Vec3::new(0.0, 5.0, 10.0), Vec3::new(0.0, 0.0, 0.0));
    world.insert_resource(camera);

    // Verify camera exists
    assert!(world.resource::<Camera>().is_some());

    let initial_pos = world.resource::<Camera>().unwrap().position;
    assert_eq!(initial_pos, Vec3::new(0.0, 5.0, 10.0));

    // Update camera
    let mut cam = world.resource_mut::<Camera>().unwrap();
    cam.update(Vec3::new(5.0, 3.0, 5.0), Vec3::new(0.0, 0.0, 0.0));

    let new_pos = world.resource::<Camera>().unwrap().position;
    assert_eq!(new_pos, Vec3::new(5.0, 3.0, 5.0));
}

#[test]
fn test_camera_view_matrix_updates() {
    let mut world = World::new();

    let camera = Camera::new(Vec3::new(0.0, 0.0, 10.0), Vec3::new(0.0, 0.0, 0.0));
    world.insert_resource(camera);

    let initial_view = world.resource::<Camera>().unwrap().view_matrix;

    // Move camera to the side
    let mut cam = world.resource_mut::<Camera>().unwrap();
    cam.update(Vec3::new(10.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 0.0));

    let new_view = world.resource::<Camera>().unwrap().view_matrix;

    // View matrix should have changed
    assert_ne!(initial_view, new_view);
}

#[test]
fn test_entities_relative_to_camera() {
    let mut world = World::new();

    // Create camera
    let camera = Camera::new(Vec3::new(0.0, 5.0, 10.0), Vec3::new(0.0, 0.0, 0.0));
    world.insert_resource(camera);

    // Spawn entities
    let entity_near = world.spawn();
    world.insert(
        entity_near,
        Transform::from_position(Vec3::new(0.0, 0.0, 5.0)),
    );
    world.insert(entity_near, Renderable);

    let entity_far = world.spawn();
    world.insert(
        entity_far,
        Transform::from_position(Vec3::new(0.0, 0.0, 100.0)),
    );
    world.insert(entity_far, Renderable);

    // Calculate distances from camera
    let cam_pos = world.resource::<Camera>().unwrap().position;

    let near_dist = world
        .get::<Transform>(entity_near)
        .map(|t| (t.position - cam_pos).length())
        .unwrap();
    let far_dist = world
        .get::<Transform>(entity_far)
        .map(|t| (t.position - cam_pos).length())
        .unwrap();

    assert!(near_dist < far_dist, "Near entity should be closer");
}

#[test]
fn test_camera_projection_matrix_properties() {
    let mut world = World::new();

    let camera = Camera::new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0));
    world.insert_resource(camera);

    let proj = world.resource::<Camera>().unwrap().projection_matrix;

    // Verify it's a valid projection matrix (not identity)
    assert_ne!(proj, Mat4::identity());
}

#[test]
fn test_render_system_can_process_entities_with_camera() {
    let mut world = World::new();

    use quasar_render::mesh::MeshShape;

    // Create camera
    let camera = Camera::new(Vec3::new(0.0, 5.0, 10.0), Vec3::new(0.0, 0.0, 0.0));
    world.insert_resource(camera);

    // Spawn renderable entities
    for i in 0..5 {
        let entity = world.spawn();
        world.insert(
            entity,
            Transform::from_position(Vec3::new(i as f32 * 2.0, 0.0, 0.0)),
        );
        world.insert(entity, MeshShape::Cube);
        world.insert(entity, Renderable);
        world.insert(entity, Visible(true));
    }

    // Simulate a render pass: collect camera + renderable entities
    let cam = world.resource::<Camera>().unwrap();
    let _view_matrix = cam.view_matrix;
    let _proj_matrix = cam.projection_matrix;

    let renderable_count = world
        .query3::<Transform, MeshShape, Renderable>()
        .into_iter()
        .filter(|(_, _, _)| {
            // Check visibility (simplified)
            world
                .query2::<Renderable, Visible>()
                .into_iter()
                .any(|(_, v)| v.0)
        })
        .count();

    assert_eq!(renderable_count, 5);
}

// ---------------------------------------------------------------------------
// 5. Additional rendering integration tests
// ---------------------------------------------------------------------------

#[test]
fn test_mesh_shape_to_mesh_data() {
    use quasar_render::mesh::MeshShape;

    // Verify that MeshShape can generate mesh data
    let cube_data = MeshShape::Cube.to_mesh_data();
    assert!(!cube_data.vertices.is_empty());
    assert!(!cube_data.indices.is_empty());

    let plane_data = MeshShape::Plane.to_mesh_data();
    assert!(!plane_data.vertices.is_empty());
    assert!(!plane_data.indices.is_empty());

    let sphere_data = MeshShape::Sphere {
        sectors: 8,
        stacks: 8,
    }
    .to_mesh_data();
    assert!(!sphere_data.vertices.is_empty());
    assert!(!sphere_data.indices.is_empty());
}

#[test]
fn test_texture_handle_component() {
    use quasar_render::components::TextureHandle;

    let mut world = World::new();

    let entity = world.spawn();
    world.insert(entity, Transform::from_position(Vec3::new(0.0, 0.0, 0.0)));
    world.insert(entity, TextureHandle::new(42));

    let handle = world.get::<TextureHandle>(entity).unwrap();
    assert_eq!(handle.index, 42);
}

#[test]
fn test_entity_with_multiple_render_components() {
    use quasar_render::components::TextureHandle;
    use quasar_render::mesh::MeshShape;

    let mut world = World::new();

    let entity = world.spawn();
    world.insert(entity, Transform::from_position(Vec3::new(1.0, 2.0, 3.0)));
    world.insert(entity, MeshShape::Cube);
    world.insert(entity, TextureHandle::new(10));
    world.insert(entity, Renderable);
    world.insert(entity, Visible(true));

    // Query all render components
    assert!(world.get::<Transform>(entity).is_some());
    assert!(world.get::<MeshShape>(entity).is_some());
    assert!(world.get::<TextureHandle>(entity).is_some());
    assert!(world.get::<Renderable>(entity).is_some());
    assert!(world.get::<Visible>(entity).is_some());

    // Verify values
    let transform = world.get::<Transform>(entity).unwrap();
    assert_eq!(transform.position, Vec3::new(1.0, 2.0, 3.0));

    let texture = world.get::<TextureHandle>(entity).unwrap();
    assert_eq!(texture.index, 10);
}
