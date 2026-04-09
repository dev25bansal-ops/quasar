//! Network replication system - auto-replicate components across clients.

use std::any::TypeId;
use std::collections::HashMap;

use quasar_math::{Quat, Vec3};

/// Replication mode for a component field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplicationMode {
    /// Field is replicated to all peers.
    Replicated,
    /// Owner predicts locally, others receive from server.
    OwnerPredicted,
    /// Server-only, not replicated.
    ServerOnly,
}

/// Metadata about a replicated field.
#[derive(Debug, Clone)]
pub struct ReplicatedField {
    pub name: &'static str,
    pub type_name: &'static str,
    pub mode: ReplicationMode,
}

/// Trait implemented by components that can be replicated.
pub trait Replicate: 'static + Send + Sync {
    /// Component type name for network identification.
    const TYPE_NAME: &'static str;

    /// List of replicated fields.
    const FIELDS: &'static [ReplicatedField];

    /// Serialize component state to bytes.
    fn serialize(&self, buf: &mut Vec<u8>);

    /// Deserialize component state from bytes.
    fn deserialize(data: &[u8]) -> Self;

    /// Compute delta from previous state.
    fn compute_delta(&self, previous: &Self, buf: &mut Vec<u8>) -> bool;

    /// Apply delta to current state.
    fn apply_delta(&mut self, delta: &[u8]);
}

/// Serializer function type
type SerializerFn = fn(&dyn std::any::Any, &mut Vec<u8>);
/// Deserializer function type
type DeserializerFn = fn(&[u8]) -> Box<dyn std::any::Any>;

/// Global registry of replicatable component types.
pub struct ReplicationRegistry {
    type_ids: HashMap<TypeId, u16>,
    serializers: HashMap<u16, SerializerFn>,
    deserializers: HashMap<u16, DeserializerFn>,
    type_names: HashMap<u16, &'static str>,
    next_id: u16,
}

impl ReplicationRegistry {
    pub fn new() -> Self {
        Self {
            type_ids: HashMap::new(),
            serializers: HashMap::new(),
            deserializers: HashMap::new(),
            type_names: HashMap::new(),
            next_id: 0,
        }
    }

    /// Register a component type for replication.
    pub fn register<T: Replicate>(&mut self) {
        let id = self.next_id;
        self.next_id += 1;

        self.type_ids.insert(TypeId::of::<T>(), id);
        self.type_names.insert(id, T::TYPE_NAME);

        self.serializers.insert(id, |any, buf| {
            if let Some(component) = any.downcast_ref::<T>() {
                component.serialize(buf);
            }
        });

        self.deserializers
            .insert(id, |data| Box::new(T::deserialize(data)));
    }

    /// Get network ID for a component type.
    pub fn type_id<T: 'static>(&self) -> Option<u16> {
        self.type_ids.get(&TypeId::of::<T>()).copied()
    }

    /// Serialize a component by type ID.
    pub fn serialize(&self, type_id: u16, component: &dyn std::any::Any, buf: &mut Vec<u8>) {
        if let Some(serializer) = self.serializers.get(&type_id) {
            serializer(component, buf);
        }
    }

    /// Deserialize a component by type ID.
    pub fn deserialize(&self, type_id: u16, data: &[u8]) -> Option<Box<dyn std::any::Any>> {
        self.deserializers
            .get(&type_id)
            .map(|deserializer| deserializer(data))
    }
}

impl Default for ReplicationRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Delta Compression with Float Quantization
// ---------------------------------------------------------------------------

/// Quantize a position to 1cm precision.
pub fn quantize_position(pos: Vec3) -> [i32; 3] {
    const SCALE: f32 = 100.0; // 1cm precision
    [
        (pos.x * SCALE).round() as i32,
        (pos.y * SCALE).round() as i32,
        (pos.z * SCALE).round() as i32,
    ]
}

/// Dequantize a position from 1cm precision.
pub fn dequantize_position(quant: [i32; 3]) -> Vec3 {
    const SCALE: f32 = 100.0;
    Vec3::new(
        quant[0] as f32 / SCALE,
        quant[1] as f32 / SCALE,
        quant[2] as f32 / SCALE,
    )
}

/// Quantize a rotation quaternion using smallest-3 encoding (48 bits).
/// Saves 33% bandwidth vs full quaternion (128 bits).
pub fn quantize_rotation(rot: Quat) -> [i16; 3] {
    // Find the largest component
    let (largest_idx, sign) = {
        let abs_vals = [rot.x.abs(), rot.y.abs(), rot.z.abs(), rot.w.abs()];
        let mut max_idx = 0;
        let mut max_val = abs_vals[0];
        for (i, &val) in abs_vals.iter().enumerate().skip(1) {
            if val > max_val {
                max_idx = i;
                max_val = val;
            }
        }
        // Determine sign to ensure largest component is positive
        let sign = if [rot.x, rot.y, rot.z, rot.w][max_idx] >= 0.0 {
            1.0
        } else {
            -1.0
        };
        (max_idx, sign)
    };

    // Compute the three smallest components
    let components = [rot.x * sign, rot.y * sign, rot.z * sign, rot.w * sign];
    let mut result = [0i16; 3];
    let mut j = 0;
    for (i, &comp) in components.iter().enumerate() {
        if i != largest_idx {
            // Scale to 16-bit range
            result[j] = (comp * 32767.0).round() as i16;
            j += 1;
        }
    }

    result
}

/// Dequantize a rotation from smallest-3 encoding.
pub fn dequantize_rotation(quant: [i16; 3], largest_idx: u8) -> Quat {
    let mut components = [0.0f32; 4];
    let mut j = 0;
    for (i, comp) in components.iter_mut().enumerate() {
        if i as u8 != largest_idx {
            *comp = quant[j] as f32 / 32767.0;
            j += 1;
        }
    }

    // Reconstruct the largest component
    let sum_sq = components.iter().map(|x| x * x).sum::<f32>();
    components[largest_idx as usize] = (1.0 - sum_sq).max(0.0).sqrt();

    Quat::from_xyzw(components[0], components[1], components[2], components[3])
}

/// Quantize an angle to 0.1 degree precision.
pub fn quantize_angle_degrees(angle: f32) -> u16 {
    const SCALE: f32 = 10.0; // 0.1 degree precision
    ((angle * SCALE).round() as u16).wrapping_add(32768) // offset to handle negative
}

/// Dequantize an angle from 0.1 degree precision.
pub fn dequantize_angle_degrees(quant: u16) -> f32 {
    const SCALE: f32 = 10.0;
    (quant.wrapping_sub(32768) as f32) / SCALE
}

// ---------------------------------------------------------------------------
// Interest Management (Spatial Filtering)
// ---------------------------------------------------------------------------

/// Spatial filter for interest management.
pub struct SpatialFilter {
    pub center: Vec3,
    pub radius: f32,
}

impl SpatialFilter {
    pub fn new(center: Vec3, radius: f32) -> Self {
        Self { center, radius }
    }

    /// Check if a position is within the filter.
    pub fn contains(&self, pos: Vec3) -> bool {
        (pos - self.center).length_squared() <= self.radius * self.radius
    }

    /// Check if an AABB overlaps the filter.
    pub fn overlaps_aabb(&self, min: Vec3, max: Vec3) -> bool {
        let closest = self.center.clamp(min, max);
        (closest - self.center).length_squared() <= self.radius * self.radius
    }
}

/// Grid-based spatial acceleration for O(1) entity lookup.
pub struct SpatialGrid {
    cell_size: f32,
    cells: HashMap<(i32, i32, i32), Vec<crate::ecs::Entity>>,
}

impl SpatialGrid {
    pub fn new(cell_size: f32) -> Self {
        Self {
            cell_size,
            cells: HashMap::new(),
        }
    }

    pub fn cell_coord(&self, pos: Vec3) -> (i32, i32, i32) {
        (
            (pos.x / self.cell_size).floor() as i32,
            (pos.y / self.cell_size).floor() as i32,
            (pos.z / self.cell_size).floor() as i32,
        )
    }

    pub fn insert(&mut self, entity: crate::ecs::Entity, pos: Vec3) {
        let cell = self.cell_coord(pos);
        self.cells.entry(cell).or_default().push(entity);
    }

    pub fn remove(&mut self, entity: crate::ecs::Entity, pos: Vec3) {
        let cell = self.cell_coord(pos);
        if let Some(entities) = self.cells.get_mut(&cell) {
            entities.retain(|&e| e != entity);
        }
    }

    pub fn query(&self, filter: &SpatialFilter) -> Vec<crate::ecs::Entity> {
        let min_cell = self.cell_coord(filter.center - Vec3::splat(filter.radius));
        let max_cell = self.cell_coord(filter.center + Vec3::splat(filter.radius));

        let mut result = Vec::new();
        for x in min_cell.0..=max_cell.0 {
            for y in min_cell.1..=max_cell.1 {
                for z in min_cell.2..=max_cell.2 {
                    if let Some(entities) = self.cells.get(&(x, y, z)) {
                        result.extend(entities.iter().copied());
                    }
                }
            }
        }
        result
    }
}

// ---------------------------------------------------------------------------
// Property-based tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn prop_position_quantization_error_bounded(
            x in -10000f32..10000f32,
            y in -10000f32..10000f32,
            z in -10000f32..10000f32
        ) {
            let pos = Vec3::new(x, y, z);
            let quant = quantize_position(pos);
            let restored = dequantize_position(quant);

            let max_error = 0.011; // Slightly above 1cm for float rounding
            prop_assert!((restored.x - pos.x).abs() < max_error);
            prop_assert!((restored.y - pos.y).abs() < max_error);
            prop_assert!((restored.z - pos.z).abs() < max_error);
        }

        #[test]
        fn prop_rotation_quantization_angle_error_bounded(
            axis_x in -1.0f32..=1.0,
            axis_y in -1.0f32..=1.0,
            axis_z in -1.0f32..=1.0,
            angle in 0.5f32..=std::f32::consts::TAU
        ) {
            let axis = Vec3::new(axis_x, axis_y, axis_z);
            // Skip near-zero axes that would produce NaN on normalize
            prop_assume!(axis.length_squared() > 0.1);
            let axis = axis.normalize();
            let rot = Quat::from_axis_angle(axis, angle);

            let quant = quantize_rotation(rot);
            // Use the largest component index from the original quaternion
            let abs_vals = [rot.x.abs(), rot.y.abs(), rot.z.abs(), rot.w.abs()];
            let mut largest_idx = 0u8;
            let mut largest_val = abs_vals[0];
            for (i, &v) in abs_vals.iter().enumerate().skip(1) {
                if v > largest_val {
                    largest_val = v;
                    largest_idx = i as u8;
                }
            }

            let restored = dequantize_rotation(quant, largest_idx);
            let dot = rot.x * restored.x + rot.y * restored.y +
                      rot.z * restored.z + rot.w * restored.w;
            // Angular distance between quaternions: 2 * acos(|dot|)
            // q and -q represent the same rotation, so we use abs(dot).
            let dot_clamped = dot.abs().clamp(0.0, 1.0);
            let angle_error = 2.0 * dot_clamped.acos();

            prop_assert!(angle_error < 0.02,
                "Rotation error too large: {} rad (axis={:?}, angle={}, largest_idx={})",
                angle_error, axis, angle, largest_idx);
        }
    }
}
