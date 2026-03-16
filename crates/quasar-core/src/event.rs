//! Typed event bus for decoupled communication between systems.

use std::any::{Any, TypeId};
use std::collections::HashMap;

/// A collection of event channels, keyed by type.
///
/// Systems can send events that other systems read, enabling loose coupling.
///
/// # Examples
/// ```
/// use quasar_core::Events;
///
/// struct DamageEvent { amount: u32 }
///
/// let mut events = Events::new();
/// events.send(DamageEvent { amount: 50 });
///
/// for ev in events.read::<DamageEvent>() {
///     assert_eq!(ev.amount, 50);
/// }
/// events.clear::<DamageEvent>();
/// ```
pub struct Events {
    channels: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl Events {
    pub fn new() -> Self {
        Self {
            channels: HashMap::new(),
        }
    }

    /// Send an event, appending it to the channel for type `T`.
    pub fn send<T: 'static + Send + Sync>(&mut self, event: T) {
        let type_id = TypeId::of::<T>();
        let channel = self
            .channels
            .entry(type_id)
            .or_insert_with(|| Box::new(Vec::<T>::new()));
        if let Some(vec) = channel
            .downcast_mut::<Vec<T>>()
        {
            vec.push(event);
        }
    }

    /// Read all events of type `T` sent since last clear.
    pub fn read<T: 'static + Send + Sync>(&self) -> &[T] {
        let type_id = TypeId::of::<T>();
        self.channels
            .get(&type_id)
            .and_then(|c| c.downcast_ref::<Vec<T>>())
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Clear all events of type `T`. Call at the end of each frame.
    pub fn clear<T: 'static + Send + Sync>(&mut self) {
        let type_id = TypeId::of::<T>();
        if let Some(channel) = self.channels.get_mut(&type_id) {
            if let Some(vec) = channel.downcast_mut::<Vec<T>>() {
                vec.clear();
            }
        }
    }

    /// Clear all event channels.
    pub fn clear_all(&mut self) {
        self.channels.clear();
    }
}

impl Default for Events {
    fn default() -> Self {
        Self::new()
    }
}
