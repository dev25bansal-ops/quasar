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
        if let Some(vec) = channel.downcast_mut::<Vec<T>>() {
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

/// Type-safe event channel with reader tracking.
///
/// Multiple systems can register as readers and each will receive
/// all events since their last read, independently.
pub struct EventsChannel<T> {
    events: Vec<T>,
    readers: Vec<usize>,
}

impl<T: Clone> Default for EventsChannel<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone> EventsChannel<T> {
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            readers: Vec::new(),
        }
    }

    /// Register a new reader and return its ID.
    pub fn register_reader(&mut self) -> usize {
        let id = self.readers.len();
        self.readers.push(0);
        id
    }

    /// Send an event to all readers.
    pub fn send(&mut self, event: T) {
        self.events.push(event);
    }

    /// Read all events since the last read for this reader.
    /// Returns an iterator over the new events.
    pub fn read(&mut self, reader_id: usize) -> impl Iterator<Item = &T> {
        let start = self.readers.get(reader_id).copied().unwrap_or(0);
        if reader_id < self.readers.len() {
            self.readers[reader_id] = self.events.len();
        }
        self.events[start..].iter()
    }

    /// Read all events since the last read, cloning them.
    pub fn read_cloned(&mut self, reader_id: usize) -> Vec<T> {
        self.read(reader_id).cloned().collect()
    }

    /// Get the number of events in the channel.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Check if the channel is empty.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Get the number of registered readers.
    pub fn reader_count(&self) -> usize {
        self.readers.len()
    }

    /// Clear all events and reset all reader positions.
    pub fn clear(&mut self) {
        self.events.clear();
        for reader in &mut self.readers {
            *reader = 0;
        }
    }

    /// Remove old events that have been read by all readers.
    /// Call periodically to prevent unbounded memory growth.
    pub fn drain_read(&mut self) {
        if self.readers.is_empty() {
            self.events.clear();
            return;
        }
        let min_read = *self.readers.iter().min().unwrap_or(&0);
        if min_read > 0 {
            self.events.drain(0..min_read);
            for reader in &mut self.readers {
                *reader = reader.saturating_sub(min_read);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_send_and_read() {
        let mut events = Events::new();
        events.send(42i32);
        events.send(100i32);

        let read = events.read::<i32>();
        assert_eq!(read.len(), 2);
        assert_eq!(read[0], 42);
        assert_eq!(read[1], 100);
    }

    #[test]
    fn events_clear() {
        let mut events = Events::new();
        events.send(1i32);
        events.clear::<i32>();
        assert!(events.read::<i32>().is_empty());
    }

    #[test]
    fn events_channel_reader_tracking() {
        let mut channel: EventsChannel<i32> = EventsChannel::new();
        let reader1 = channel.register_reader();
        let reader2 = channel.register_reader();

        channel.send(10);
        channel.send(20);

        let r1_events: Vec<_> = channel.read(reader1).copied().collect();
        assert_eq!(r1_events, vec![10, 20]);

        channel.send(30);

        let r1_events: Vec<_> = channel.read(reader1).copied().collect();
        assert_eq!(r1_events, vec![30]);

        let r2_events: Vec<_> = channel.read(reader2).copied().collect();
        assert_eq!(r2_events, vec![10, 20, 30]);
    }

    #[test]
    fn events_channel_drain_read() {
        let mut channel: EventsChannel<i32> = EventsChannel::new();
        let reader = channel.register_reader();

        channel.send(1);
        channel.send(2);
        channel.send(3);

        let _: Vec<_> = channel.read(reader).collect();
        assert_eq!(channel.len(), 3);

        channel.drain_read();
        assert_eq!(channel.len(), 0);
    }
}
