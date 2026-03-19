//! Sparse-set component storage.
//!
//! An alternative to archetype SoA columns for components that are:
//! - Frequently added/removed (avoids archetype migration cost)
//! - Rarely iterated in bulk (cache locality less important)
//! - Used as tags, markers, or flags
//!
//! Provides O(1) insert, remove, and lookup by entity index.
//! Components stored in sparse sets do NOT affect archetype signatures.

use std::any::{Any, TypeId};
use std::collections::HashMap;

use super::Entity;

/// A generic sparse-set storing components of type `T` keyed by entity index.
pub struct SparseSet<T: 'static + Send + Sync> {
    /// Sparse array: entity index → dense index.
    sparse: HashMap<u32, usize>,
    /// Dense data array.
    dense: Vec<T>,
    /// Parallel entity array — same indexing as `dense`.
    entities: Vec<u32>,
}

impl<T: 'static + Send + Sync> SparseSet<T> {
    pub fn new() -> Self {
        Self {
            sparse: HashMap::new(),
            dense: Vec::new(),
            entities: Vec::new(),
        }
    }

    pub fn insert(&mut self, entity: Entity, value: T) {
        let idx = entity.index();
        if let Some(&dense_idx) = self.sparse.get(&idx) {
            // Overwrite existing value.
            self.dense[dense_idx] = value;
        } else {
            let dense_idx = self.dense.len();
            self.dense.push(value);
            self.entities.push(idx);
            self.sparse.insert(idx, dense_idx);
        }
    }

    pub fn remove(&mut self, entity: Entity) -> Option<T> {
        let idx = entity.index();
        let dense_idx = self.sparse.remove(&idx)?;
        let last = self.dense.len() - 1;

        if dense_idx != last {
            let swapped_entity = self.entities[last];
            self.sparse.insert(swapped_entity, dense_idx);
            self.entities[dense_idx] = swapped_entity;
        }

        self.entities.pop();
        Some(self.dense.swap_remove(dense_idx))
    }

    pub fn get(&self, entity: Entity) -> Option<&T> {
        let &dense_idx = self.sparse.get(&entity.index())?;
        self.dense.get(dense_idx)
    }

    pub fn get_mut(&mut self, entity: Entity) -> Option<&mut T> {
        let &dense_idx = self.sparse.get(&entity.index())?;
        self.dense.get_mut(dense_idx)
    }

    pub fn contains(&self, entity: Entity) -> bool {
        self.sparse.contains_key(&entity.index())
    }

    pub fn len(&self) -> usize {
        self.dense.len()
    }

    pub fn is_empty(&self) -> bool {
        self.dense.is_empty()
    }

    /// Iterate over all (entity_index, &T) pairs.
    pub fn iter(&self) -> impl Iterator<Item = (u32, &T)> {
        self.entities.iter().copied().zip(self.dense.iter())
    }

    /// Iterate over all (entity_index, &mut T) pairs.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (u32, &mut T)> {
        self.entities.iter().copied().zip(self.dense.iter_mut())
    }
}

impl<T: 'static + Send + Sync> Default for SparseSet<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Type-erased sparse-set storage for runtime component access.
pub trait ErasedSparseSet: Any + Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn remove_entity(&mut self, entity: Entity);
    fn contains_entity(&self, entity: Entity) -> bool;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<T: 'static + Send + Sync> ErasedSparseSet for SparseSet<T> {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn remove_entity(&mut self, entity: Entity) {
        self.remove(entity);
    }
    fn contains_entity(&self, entity: Entity) -> bool {
        self.contains(entity)
    }
    fn len(&self) -> usize {
        self.len()
    }
}

/// Registry of type-erased sparse sets, keyed by TypeId.
pub struct SparseSetStorage {
    sets: HashMap<TypeId, Box<dyn ErasedSparseSet>>,
}

impl SparseSetStorage {
    pub fn new() -> Self {
        Self {
            sets: HashMap::new(),
        }
    }

    /// Get or create a typed sparse set for component T.
    pub fn get_or_create<T: 'static + Send + Sync>(&mut self) -> &mut SparseSet<T> {
        let type_id = TypeId::of::<T>();
        match self
            .sets
            .entry(type_id)
            .or_insert_with(|| Box::new(SparseSet::<T>::new()))
            .as_any_mut()
            .downcast_mut::<SparseSet<T>>()
        {
            Some(set) => set,
            None => unreachable!("type mismatch in sparse set storage"),
        }
    }

    /// Get a typed sparse set for component T (read-only).
    pub fn get<T: 'static + Send + Sync>(&self) -> Option<&SparseSet<T>> {
        let type_id = TypeId::of::<T>();
        self.sets
            .get(&type_id)?
            .as_any()
            .downcast_ref::<SparseSet<T>>()
    }

    /// Get a typed sparse set for component T (mutable).
    pub fn get_mut<T: 'static + Send + Sync>(&mut self) -> Option<&mut SparseSet<T>> {
        let type_id = TypeId::of::<T>();
        self.sets
            .get_mut(&type_id)?
            .as_any_mut()
            .downcast_mut::<SparseSet<T>>()
    }

    /// Remove an entity from all sparse sets.
    pub fn remove_entity(&mut self, entity: Entity) {
        for set in self.sets.values_mut() {
            set.remove_entity(entity);
        }
    }

    /// Check if a specific component type is stored as a sparse set for this entity.
    pub fn contains<T: 'static + Send + Sync>(&self, entity: Entity) -> bool {
        self.get::<T>().is_some_and(|s| s.contains(entity))
    }

    /// Check if a type is registered as a sparse set.
    pub fn has_type(&self, type_id: &TypeId) -> bool {
        self.sets.contains_key(type_id)
    }
}

impl Default for SparseSetStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_get() {
        let mut set = SparseSet::<u32>::new();
        let e = Entity::new(5, 0);
        set.insert(e, 42);
        assert_eq!(set.get(e), Some(&42));
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn overwrite() {
        let mut set = SparseSet::<u32>::new();
        let e = Entity::new(3, 0);
        set.insert(e, 10);
        set.insert(e, 20);
        assert_eq!(set.get(e), Some(&20));
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn remove_and_swap() {
        let mut set = SparseSet::<&str>::new();
        let e0 = Entity::new(0, 0);
        let e1 = Entity::new(1, 0);
        let e2 = Entity::new(2, 0);

        set.insert(e0, "a");
        set.insert(e1, "b");
        set.insert(e2, "c");

        assert_eq!(set.remove(e0), Some("a"));
        assert_eq!(set.len(), 2);
        // e2 was swapped into the freed slot
        assert_eq!(set.get(e1), Some(&"b"));
        assert_eq!(set.get(e2), Some(&"c"));
        assert!(!set.contains(e0));
    }

    #[test]
    fn iter() {
        let mut set = SparseSet::<i32>::new();
        set.insert(Entity::new(10, 0), 100);
        set.insert(Entity::new(20, 0), 200);

        let mut items: Vec<_> = set.iter().collect();
        items.sort_by_key(|(idx, _)| *idx);
        assert_eq!(items, vec![(10, &100), (20, &200)]);
    }

    #[test]
    fn storage_registry() {
        let mut storage = SparseSetStorage::new();
        let e = Entity::new(1, 0);

        storage.get_or_create::<f32>().insert(e, 3.14);
        storage
            .get_or_create::<String>()
            .insert(e, "hello".to_string());

        assert_eq!(storage.get::<f32>().unwrap().get(e), Some(&3.14));
        assert!(storage.contains::<String>(e));

        storage.remove_entity(e);
        assert!(!storage.contains::<f32>(e));
        assert!(!storage.contains::<String>(e));
    }
}
