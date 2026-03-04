//! Instanced mesh rendering — batch identical meshes into single draw calls.
//!
//! Dramatically improves performance when rendering many copies of the
//! same mesh (e.g., grass, particles, crowds).

use crate::mesh::MeshShape;
use quasar_math::Transform;

/// Maximum instances per batch.
pub const MAX_INSTANCES: usize = 1024;

/// Component marking an entity for instanced rendering.
///
/// Entities with the same mesh and material are batched together.
#[derive(Debug, Clone)]
pub struct InstancedMesh {
    /// The mesh shape to render.
    pub mesh: MeshShape,
    /// Optional material override.
    pub material_index: Option<u32>,
}

impl InstancedMesh {
    pub fn new(mesh: MeshShape) -> Self {
        Self {
            mesh,
            material_index: None,
        }
    }

    pub fn with_material(mut self, index: u32) -> Self {
        self.material_index = Some(index);
        self
    }
}

/// GPU buffer holding instance data (model matrices).
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct InstanceData {
    pub model: [[f32; 4]; 4],
}

impl InstanceData {
    pub fn from_transform(transform: &Transform) -> Self {
        Self {
            model: transform.matrix().to_cols_array_2d(),
        }
    }
}

/// A batch of instances sharing the same mesh and material.
pub struct InstanceBatch {
    /// Mesh shape for this batch.
    pub mesh: MeshShape,
    /// Material index (None = default).
    pub material_index: Option<u32>,
    /// Instance data buffer.
    pub instances: Vec<InstanceData>,
}

impl InstanceBatch {
    pub fn new(mesh: MeshShape, material_index: Option<u32>) -> Self {
        Self {
            mesh,
            material_index,
            instances: Vec::new(),
        }
    }

    pub fn add(&mut self, transform: &Transform) {
        self.instances.push(InstanceData::from_transform(transform));
    }

    pub fn clear(&mut self) {
        self.instances.clear();
    }

    pub fn len(&self) -> usize {
        self.instances.len()
    }

    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }
}

/// System that collects instanced meshes and batches them.
pub struct InstanceCollector;

impl InstanceCollector {
    /// Collect all instanced meshes into batches by (mesh, material).
    pub fn collect(world: &quasar_core::ecs::World) -> Vec<InstanceBatch> {
        use std::collections::HashMap;

        let mut batches: HashMap<(MeshShape, Option<u32>), InstanceBatch> = HashMap::new();

        // Query all instanced meshes with transforms
        for (entity, instanced) in world.query::<InstancedMesh>() {
            if let Some(transform) = world.get::<Transform>(entity) {
                let key = (instanced.mesh, instanced.material_index);
                let batch = batches.entry(key).or_insert_with(|| {
                    InstanceBatch::new(instanced.mesh, instanced.material_index)
                });
                batch.add(transform);
            }
        }

        batches.into_values().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quasar_math::Vec3;

    #[test]
    fn instance_data_from_transform() {
        let transform = Transform::from_position(Vec3::new(1.0, 2.0, 3.0));
        let data = InstanceData::from_transform(&transform);
        assert_eq!(data.model[3][0], 1.0);
        assert_eq!(data.model[3][1], 2.0);
        assert_eq!(data.model[3][2], 3.0);
    }

    #[test]
    fn batch_add_instances() {
        let mut batch = InstanceBatch::new(MeshShape::Cube, None);
        assert!(batch.is_empty());

        batch.add(&Transform::IDENTITY);
        assert_eq!(batch.len(), 1);

        batch.clear();
        assert!(batch.is_empty());
    }
}
