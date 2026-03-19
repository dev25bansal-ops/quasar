//! Component storage — marker trait for ECS components.

use std::ops::{Deref, DerefMut};

/// Marker trait for data that can be attached to entities.
///
/// Any `'static + Send + Sync` type automatically implements `Component`.
pub trait Component: 'static + Send + Sync {}

/// Blanket implementation: every `'static + Send + Sync` type is a component.
impl<T: 'static + Send + Sync> Component for T {}

/// Mutable borrow guard that automatically marks component as changed on drop.
///
/// This enables `FilterChanged<T>` to work correctly without manual marking.
/// When the guard is dropped, the component's change tick is updated.
pub struct Mut<'a, T: Component> {
    data: &'a mut T,
    changed_tick: &'a mut u64,
    tick: u64,
}

impl<'a, T: Component> Mut<'a, T> {
    /// Create a new mutable guard.
    ///
    /// # Safety
    /// The `changed_tick` reference must be valid for the lifetime of the guard
    /// and must correspond to the change tick storage for this component instance.
    pub unsafe fn new(data: &'a mut T, changed_tick: &'a mut u64, tick: u64) -> Self {
        Self {
            data,
            changed_tick,
            tick,
        }
    }

    /// Manually mark the component as changed (e.g. before long operations).
    pub fn mark_changed(&mut self) {
        *self.changed_tick = self.tick;
    }

    /// Get a mutable reference without triggering change detection.
    ///
    /// # Safety
    /// The caller is responsible for calling `mark_changed()` if the data is modified.
    pub unsafe fn bypass_change_detection(&mut self) -> &mut T {
        self.data
    }
}

impl<'a, T: Component> Deref for Mut<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.data
    }
}

impl<'a, T: Component> DerefMut for Mut<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // Mark as changed on first mutable access
        *self.changed_tick = self.tick;
        self.data
    }
}

impl<'a, T: Component> Drop for Mut<'a, T> {
    fn drop(&mut self) {
        // Ensure change tick is set on drop
        *self.changed_tick = self.tick;
    }
}
