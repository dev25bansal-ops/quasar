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
}

/// Concrete storage for a specific component type `T`.
pub(crate) struct TypedStorage<T: Component> {
    pub(crate) data: HashMap<u32, T>,
}

impl<T: Component> TypedStorage<T> {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }

    /// Insert a component for the given entity, returning the old value if any.
    pub fn insert(&mut self, entity: Entity, value: T) -> Option<T> {
        self.data.insert(entity.index, value)
    }

    /// Get a shared reference to the component for an entity.
    pub fn get(&self, entity: Entity) -> Option<&T> {
        self.data.get(&entity.index)
    }

    /// Get a mutable reference to the component for an entity.
    pub fn get_mut(&mut self, entity: Entity) -> Option<&mut T> {
        self.data.get_mut(&entity.index)
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
            self.data.insert(entity.index, *typed);
        }
    }
}
