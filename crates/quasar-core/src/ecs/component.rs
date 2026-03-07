//! Component storage — type-erased containers that hold per-entity data.

use std::any::Any;
use std::collections::HashMap;

use super::Entity;

/// Marker trait for data that can be attached to entities.
///
/// Any `'static + Send + Sync` type automatically implements `Component`.
pub trait Component: 'static + Send + Sync {}

/// Blanket implementation: every `'static + Send + Sync` type is a component.
impl<T: 'static + Send + Sync> Component for T {}

// ---------------------------------------------------------------------------
// Internal: type-erased storage
// ---------------------------------------------------------------------------

/// Trait object interface for component storage, allowing the [`World`] to
/// store heterogeneous component types in a single collection.
#[allow(dead_code)]
pub(crate) trait ComponentStorage: Any + Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn remove(&mut self, entity: Entity) -> bool;
    fn has(&self, entity: Entity) -> bool;
    fn len(&self) -> usize;
    fn insert_raw(&mut self, entity: Entity, component: Box<dyn Any + Send + Sync>);
    /// Advance the global tick used for change detection.
    fn tick(&mut self);
    /// Return the current global tick.
    fn current_tick(&self) -> u64;
}

/// Concrete storage for a specific component type `T`.
pub(crate) struct TypedStorage<T: Component> {
    pub(crate) data: HashMap<u32, T>,
    /// Per-entity change tick — set to `current_tick` on insert or mutable access.
    pub(crate) change_ticks: HashMap<u32, u64>,
    /// Monotonically increasing tick, advanced once per frame.
    pub(crate) current_tick: u64,
}

impl<T: Component> TypedStorage<T> {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
            change_ticks: HashMap::new(),
            current_tick: 0,
        }
    }

    /// Insert a component for the given entity, returning the old value if any.
    pub fn insert(&mut self, entity: Entity, value: T) -> Option<T> {
        self.change_ticks.insert(entity.index, self.current_tick);
        self.data.insert(entity.index, value)
    }

    /// Get a shared reference to the component for an entity.
    pub fn get(&self, entity: Entity) -> Option<&T> {
        self.data.get(&entity.index)
    }

    /// Get a mutable reference to the component for an entity.
    /// Marks the component as changed for change-detection queries.
    pub fn get_mut(&mut self, entity: Entity) -> Option<&mut T> {
        if self.data.contains_key(&entity.index) {
            self.change_ticks.insert(entity.index, self.current_tick);
        }
        self.data.get_mut(&entity.index)
    }

    /// Check whether a component was changed since the given tick.
    pub fn changed_since(&self, entity_index: u32, since_tick: u64) -> bool {
        self.change_ticks
            .get(&entity_index)
            .map_or(false, |&tick| tick > since_tick)
    }
}

impl<T: Component> ComponentStorage for TypedStorage<T> {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn remove(&mut self, entity: Entity) -> bool {
        self.change_ticks.remove(&entity.index);
        self.data.remove(&entity.index).is_some()
    }

    fn has(&self, entity: Entity) -> bool {
        self.data.contains_key(&entity.index)
    }

    fn len(&self) -> usize {
        self.data.len()
    }

    fn insert_raw(&mut self, entity: Entity, component: Box<dyn Any + Send + Sync>) {
        if let Ok(typed) = component.downcast::<T>() {
            self.change_ticks.insert(entity.index, self.current_tick);
            self.data.insert(entity.index, *typed);
        }
    }

    fn tick(&mut self) {
        self.current_tick += 1;
    }

    fn current_tick(&self) -> u64 {
        self.current_tick
    }
}
