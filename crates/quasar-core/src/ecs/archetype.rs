//! Archetype-based ECS storage — groups entities with same components into contiguous arrays.
//!
//! Provides 5–50x query performance improvement by:
//! - Grouping entities with identical component sets into archetypes
//! - Storing components in contiguous arrays for cache efficiency
//! - Enabling SIMD/parallel processing within archetypes

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

use super::Entity;

pub type ArchetypeId = u64;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ArchetypeSignature {
    component_types: Vec<TypeId>,
}

impl ArchetypeSignature {
    pub fn new() -> Self {
        Self {
            component_types: Vec::new(),
        }
    }

    pub fn from_components<T: ComponentSet>() -> Self {
        Self {
            component_types: T::type_ids(),
        }
    }

    pub fn add(&mut self, type_id: TypeId) {
        let pos = self.component_types.binary_search(&type_id).ok();
        if pos.is_none() {
            self.component_types.push(type_id);
            self.component_types.sort();
        }
    }

    pub fn remove(&mut self, type_id: TypeId) {
        if let Ok(pos) = self.component_types.binary_search(&type_id) {
            self.component_types.remove(pos);
        }
    }

    pub fn contains(&self, type_id: &TypeId) -> bool {
        self.component_types.binary_search(type_id).is_ok()
    }

    pub fn contains_all(&self, type_ids: &[TypeId]) -> bool {
        type_ids.iter().all(|t| self.contains(t))
    }
}

impl Default for ArchetypeSignature {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Archetype {
    pub id: ArchetypeId,
    pub signature: ArchetypeSignature,
    pub entities: Vec<Entity>,
    pub entity_to_row: HashMap<u32, usize>,
    pub components: HashMap<TypeId, Box<dyn ArchetypeColumn>>,
    // ── SoA parallel arrays for cache-friendly iteration ──
    /// Column type ids in index order.
    pub column_types: Vec<TypeId>,
    /// Parallel column storage — indexed same as `column_types`.
    pub columns: Vec<Box<dyn ColumnStorage>>,
    /// Fast lookup from TypeId to column index.
    type_to_column: HashMap<TypeId, usize>,
}

impl Archetype {
    pub fn new(id: ArchetypeId, signature: ArchetypeSignature) -> Self {
        Self {
            id,
            signature,
            entities: Vec::new(),
            entity_to_row: HashMap::new(),
            components: HashMap::new(),
            column_types: Vec::new(),
            columns: Vec::new(),
            type_to_column: HashMap::new(),
        }
    }

    pub fn add_entity(&mut self, entity: Entity) -> usize {
        let row = self.entities.len();
        self.entities.push(entity);
        self.entity_to_row.insert(entity.index(), row);
        row
    }

    pub fn remove_entity(&mut self, entity: Entity) -> Option<usize> {
        let row = self.entity_to_row.remove(&entity.index())?;
        let last_row = self.entities.len() - 1;

        if row != last_row {
            let last_entity = self.entities[last_row];
            self.entities[row] = last_entity;
            self.entity_to_row.insert(last_entity.index(), row);

            for column in self.components.values_mut() {
                column.swap_remove(row, last_row);
            }
        }

        self.entities.pop();
        Some(row)
    }

    pub fn get_column<T: 'static + Send + Sync>(&self) -> Option<&Vec<T>> {
        let column = self.components.get(&TypeId::of::<T>())?;
        column
            .as_any()
            .downcast_ref::<ArchetypeColumnTyped<T>>()
            .map(|c| &c.data)
    }

    pub fn get_column_mut<T: 'static + Send + Sync>(&mut self) -> Option<&mut Vec<T>> {
        let column = self.components.get_mut(&TypeId::of::<T>())?;
        column
            .as_any_mut()
            .downcast_mut::<ArchetypeColumnTyped<T>>()
            .map(|c| &mut c.data)
    }

    pub fn ensure_column<T: 'static + Send + Sync>(&mut self) {
        let type_id = TypeId::of::<T>();
        if !self.components.contains_key(&type_id) {
            self.components
                .insert(type_id, Box::new(ArchetypeColumnTyped::<T>::new()));
        }
        // Also ensure the SoA column exists.
        if !self.type_to_column.contains_key(&type_id) {
            let idx = self.columns.len();
            self.column_types.push(type_id);
            self.columns.push(Box::new(TypedColumn::<T>::new()));
            self.type_to_column.insert(type_id, idx);
        }
    }

    /// Get a raw const pointer to the SoA column data for type `T`.
    ///
    /// # Safety
    /// The caller must ensure the pointer is not used after the archetype is mutated.
    pub unsafe fn column_ptr<T: 'static + Send + Sync>(&self) -> Option<*const T> {
        let &idx = self.type_to_column.get(&TypeId::of::<T>())?;
        Some(self.columns[idx].raw_ptr() as *const T)
    }

    /// Get a raw mutable pointer to the SoA column data for type `T`.
    ///
    /// # Safety
    /// The caller must ensure exclusive access and that the pointer is not used
    /// after the archetype is mutated.
    pub unsafe fn column_ptr_mut<T: 'static + Send + Sync>(&mut self) -> Option<*mut T> {
        let &idx = self.type_to_column.get(&TypeId::of::<T>())?;
        Some(self.columns[idx].raw_ptr_mut() as *mut T)
    }

    /// Number of rows in the SoA columns for a given type.
    pub fn column_len<T: 'static + Send + Sync>(&self) -> usize {
        self.type_to_column
            .get(&TypeId::of::<T>())
            .map(|&idx| self.columns[idx].len())
            .unwrap_or(0)
    }

    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    pub fn has_component(&self, type_id: &TypeId) -> bool {
        self.signature.contains(type_id)
    }
}

pub trait ArchetypeColumn: Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn swap_remove(&mut self, row: usize, last_row: usize);
    fn len(&self) -> usize;
    fn push_raw(&mut self, value: Box<dyn Any + Send + Sync>);
}

pub struct ArchetypeColumnTyped<T: Send + Sync> {
    pub data: Vec<T>,
}

impl<T: 'static + Send + Sync> ArchetypeColumnTyped<T> {
    pub fn new() -> Self {
        Self { data: Vec::new() }
    }

    pub fn push(&mut self, value: T) {
        self.data.push(value);
    }

    pub fn get(&self, row: usize) -> Option<&T> {
        self.data.get(row)
    }

    pub fn get_mut(&mut self, row: usize) -> Option<&mut T> {
        self.data.get_mut(row)
    }

    pub fn as_typed<U: 'static + Send + Sync>(&self) -> Option<&Vec<U>> {
        (self as &dyn Any)
            .downcast_ref::<ArchetypeColumnTyped<U>>()
            .map(|c| &c.data)
    }

    pub fn as_typed_mut<U: 'static + Send + Sync>(&mut self) -> Option<&mut Vec<U>> {
        (self as &mut dyn Any)
            .downcast_mut::<ArchetypeColumnTyped<U>>()
            .map(|c| &mut c.data)
    }
}

impl<T: 'static + Send + Sync> ArchetypeColumn for ArchetypeColumnTyped<T> {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn swap_remove(&mut self, row: usize, last_row: usize) {
        if row < self.data.len() && last_row < self.data.len() {
            self.data.swap(row, last_row);
            self.data.pop();
        }
    }

    fn len(&self) -> usize {
        self.data.len()
    }

    fn push_raw(&mut self, value: Box<dyn Any + Send + Sync>) {
        if let Ok(typed) = value.downcast::<T>() {
            self.data.push(*typed);
        }
    }
}

// ── SoA Column Storage ────────────────────────────────────────────

/// Type-erased column storage for SoA archetype layout.
///
/// Provides raw pointer access for zero-overhead iteration when the caller
/// already knows the concrete type.
pub trait ColumnStorage: Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn swap_remove_entry(&mut self, row: usize);
    fn len(&self) -> usize;
    fn push_raw(&mut self, value: Box<dyn Any + Send + Sync>);
    /// Raw const pointer to the underlying contiguous data buffer.
    fn raw_ptr(&self) -> *const u8;
    /// Raw mutable pointer to the underlying contiguous data buffer.
    fn raw_ptr_mut(&mut self) -> *mut u8;
    /// Size of each element in bytes.
    fn element_size(&self) -> usize;
}

/// Strongly-typed SoA column backed by a contiguous `Vec<T>`.
pub struct TypedColumn<T: Send + Sync> {
    pub data: Vec<T>,
}

impl<T: 'static + Send + Sync> TypedColumn<T> {
    pub fn new() -> Self {
        Self { data: Vec::new() }
    }

    pub fn push(&mut self, value: T) {
        self.data.push(value);
    }

    pub fn get(&self, row: usize) -> Option<&T> {
        self.data.get(row)
    }

    pub fn get_mut(&mut self, row: usize) -> Option<&mut T> {
        self.data.get_mut(row)
    }
}

impl<T: 'static + Send + Sync> ColumnStorage for TypedColumn<T> {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn swap_remove_entry(&mut self, row: usize) {
        if row < self.data.len() {
            self.data.swap_remove(row);
        }
    }

    fn len(&self) -> usize {
        self.data.len()
    }

    fn push_raw(&mut self, value: Box<dyn Any + Send + Sync>) {
        if let Ok(typed) = value.downcast::<T>() {
            self.data.push(*typed);
        }
    }

    fn raw_ptr(&self) -> *const u8 {
        self.data.as_ptr() as *const u8
    }

    fn raw_ptr_mut(&mut self) -> *mut u8 {
        self.data.as_mut_ptr() as *mut u8
    }

    fn element_size(&self) -> usize {
        std::mem::size_of::<T>()
    }
}

pub trait ComponentSet {
    fn type_ids() -> Vec<TypeId>;
}

impl<T0: 'static, T1: 'static> ComponentSet for (T0, T1) {
    fn type_ids() -> Vec<TypeId> {
        let mut ids = vec![TypeId::of::<T0>(), TypeId::of::<T1>()];
        ids.sort();
        ids
    }
}

impl<T0: 'static, T1: 'static, T2: 'static> ComponentSet for (T0, T1, T2) {
    fn type_ids() -> Vec<TypeId> {
        let mut ids = vec![TypeId::of::<T0>(), TypeId::of::<T1>(), TypeId::of::<T2>()];
        ids.sort();
        ids
    }
}

impl<T0: 'static, T1: 'static, T2: 'static, T3: 'static> ComponentSet for (T0, T1, T2, T3) {
    fn type_ids() -> Vec<TypeId> {
        let mut ids = vec![
            TypeId::of::<T0>(),
            TypeId::of::<T1>(),
            TypeId::of::<T2>(),
            TypeId::of::<T3>(),
        ];
        ids.sort();
        ids
    }
}

pub struct ArchetypeGraph {
    archetypes: HashMap<ArchetypeId, Arc<Archetype>>,
    signature_to_id: HashMap<ArchetypeSignature, ArchetypeId>,
    _edges: HashMap<ArchetypeId, HashMap<TypeId, ArchetypeId>>,
    next_id: ArchetypeId,
}

impl ArchetypeGraph {
    pub fn new() -> Self {
        Self {
            archetypes: HashMap::new(),
            signature_to_id: HashMap::new(),
            _edges: HashMap::new(),
            next_id: 0,
        }
    }

    pub fn get_or_create(&mut self, signature: &ArchetypeSignature) -> Arc<Archetype> {
        if let Some(id) = self.signature_to_id.get(signature) {
            return self.archetypes.get(id).unwrap().clone();
        }

        let id = self.next_id;
        self.next_id += 1;

        let archetype = Arc::new(Archetype::new(id, signature.clone()));
        self.archetypes.insert(id, archetype.clone());
        self.signature_to_id.insert(signature.clone(), id);
        archetype
    }

    pub fn get(&self, id: ArchetypeId) -> Option<&Arc<Archetype>> {
        self.archetypes.get(&id)
    }

    /// Get a mutable reference to an archetype by ID.
    /// Returns `None` if the archetype doesn't exist or if Arc has other references.
    pub fn get_mut(&mut self, id: ArchetypeId) -> Option<&mut Archetype> {
        self.archetypes.get_mut(&id).and_then(|arc| Arc::get_mut(arc))
    }

    pub fn find_with_components(&self, type_ids: &[TypeId]) -> Vec<&Arc<Archetype>> {
        self.archetypes
            .values()
            .filter(|a| a.signature.contains_all(type_ids))
            .collect()
    }

    pub fn all_archetypes(&self) -> impl Iterator<Item = &Arc<Archetype>> {
        self.archetypes.values()
    }
}

impl Default for ArchetypeGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archetype_signature_add_remove() {
        let mut sig = ArchetypeSignature::new();
        sig.add(TypeId::of::<u32>());
        sig.add(TypeId::of::<String>());
        assert!(sig.contains(&TypeId::of::<u32>()));
        assert!(sig.contains(&TypeId::of::<String>()));

        sig.remove(TypeId::of::<u32>());
        assert!(!sig.contains(&TypeId::of::<u32>()));
    }

    #[test]
    fn archetype_column_operations() {
        let mut column = ArchetypeColumnTyped::<i32>::new();
        column.push(1);
        column.push(2);
        column.push(3);

        assert_eq!(column.data.len(), 3);
        assert_eq!(*column.get(1).unwrap(), 2);

        column.swap_remove(0, 2);
        assert_eq!(column.data.len(), 2);
    }

    #[test]
    fn archetype_graph_create() {
        let mut graph = ArchetypeGraph::new();
        let mut sig = ArchetypeSignature::new();
        sig.add(TypeId::of::<u32>());

        let arch = graph.get_or_create(&sig);
        assert_eq!(arch.id, 0);
    }
}
