//! Network replication system - auto-replicate components across clients.

use std::any::TypeId;
use std::collections::HashMap;

use quasar_math::{Quat, Vec3};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplicationMode {
    Replicated,
    OwnerPredicted,
    ServerOnly,
}

#[derive(Debug, Clone)]
pub struct ReplicatedField {
    pub name: &'static str,
    pub type_name: &'static str,
    pub mode: ReplicationMode,
}

pub trait Replicate: 'static + Send + Sync {
    const TYPE_NAME: &'static str;
    const FIELDS: &'static [ReplicatedField];
    fn serialize(&self, buf: &mut Vec<u8>);
    fn deserialize(data: &[u8]) -> Self;
    fn compute_delta(&self, previous: &Self, buf: &mut Vec<u8>) -> bool;
    fn apply_delta(&mut self, delta: &[u8]);
}

type SerializerFn = fn(&dyn std::any::Any, &mut Vec<u8>);
type DeserializerFn = fn(&[u8]) -> Box<dyn std::any::Any>;

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

    pub fn type_id<T: 'static>(&self) -> Option<u16> {
        self.type_ids.get(&TypeId::of::<T>()).copied()
    }

    pub fn serialize(&self, type_id: u16, component: &dyn std::any::Any, buf: &mut Vec<u8>) {
        if let Some(serializer) = self.serializers.get(&type_id) {
            serializer(component, buf);
        }
    }

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

pub fn quantize_position(pos: Vec3) -> [i32; 3] {
    const SCALE: f32 = 100.0;
    [
        (pos.x * SCALE).round() as i32,
        (pos.y * SCALE).round() as i32,
        (pos.z * SCALE).round() as i32,
    ]
}

pub fn dequantize_position(quant: [i32; 3]) -> Vec3 {
    const SCALE: f32 = 100.0;
    Vec3::new(
        quant[0] as f32 / SCALE,
        quant[1] as f32 / SCALE,
        quant[2] as f32 / SCALE,
    )
}

pub fn quantize_rotation(rot: Quat) -> [i16; 3] {
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
        let sign = if [rot.x, rot.y, rot.z, rot.w][max_idx] >= 0.0 {
            1.0
        } else {
            -1.0
        };
        (max_idx, sign)
    };

    let components = [rot.x * sign, rot.y * sign, rot.z * sign, rot.w * sign];
    let mut result = [0i16; 3];
    let mut j = 0;
    for (i, &comp) in components.iter().enumerate() {
        if i != largest_idx {
            result[j] = (comp * 32767.0).round() as i16;
            j += 1;
        }
    }

    result
}

pub fn dequantize_rotation(quant: [i16; 3], largest_idx: u8) -> Quat {
    let mut components = [0.0f32; 4];
    let mut j = 0;
    for (i, comp) in components.iter_mut().enumerate() {
        if i as u8 != largest_idx {
            *comp = quant[j] as f32 / 32767.0;
            j += 1;
        }
    }

    let sum_sq = components.iter().map(|x| x * x).sum::<f32>();
    components[largest_idx as usize] = (1.0 - sum_sq).max(0.0).sqrt();

    Quat::from_xyzw(components[0], components[1], components[2], components[3])
}

pub fn quantize_angle_degrees(angle: f32) -> u16 {
    const SCALE: f32 = 10.0;
    ((angle * SCALE).round() as u16).wrapping_add(32768)
}

pub fn dequantize_angle_degrees(quant: u16) -> f32 {
    const SCALE: f32 = 10.0;
    (quant.wrapping_sub(32768) as f32) / SCALE
}

pub struct SpatialFilter {
    pub center: Vec3,
    pub radius: f32,
}

impl SpatialFilter {
    pub fn new(center: Vec3, radius: f32) -> Self {
        Self { center, radius }
    }

    pub fn contains(&self, pos: Vec3) -> bool {
        (pos - self.center).length_squared() <= self.radius * self.radius
    }

    pub fn overlaps_aabb(&self, min: Vec3, max: Vec3) -> bool {
        let closest = self.center.clamp(min, max);
        (closest - self.center).length_squared() <= self.radius * self.radius
    }
}

pub struct SpatialGrid<E = u64> {
    cell_size: f32,
    cells: HashMap<(i32, i32, i32), Vec<E>>,
}

impl<E: Copy + PartialEq> SpatialGrid<E> {
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

    pub fn insert(&mut self, entity: E, pos: Vec3) {
        let cell = self.cell_coord(pos);
        self.cells.entry(cell).or_default().push(entity);
    }

    pub fn remove(&mut self, entity: E, pos: Vec3) {
        let cell = self.cell_coord(pos);
        if let Some(entities) = self.cells.get_mut(&cell) {
            entities.retain(|&e| e != entity);
        }
    }

    pub fn query(&self, filter: &SpatialFilter) -> Vec<E> {
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
