//! Type-safe event bus for decoupled system communication.
//!
//! Provides a priority-aware event system where systems can send and receive
//! typed events without direct coupling. Events are cleared each frame and
//! support multiple independent readers.
//!
//! # Architecture
//!
//! - `EventBus`: Central resource storing all event channels by TypeId
//! - `EventReader<T>`: System parameter for reading events of type T
//! - `EventWriter<T>`: System parameter for sending events of type T
//! - `Event`: Marker trait for event types (requires `Send + Sync + Clone`)
//!
//! # Example
//!
//! ```rust,ignore
//! use quasar_core::event_bus::{EventBus, EventReader, EventWriter, Event};
//!
//! #[derive(Clone, Event)]
//! struct DamageEvent { entity: Entity, amount: u32 }
//!
//! // In a sending system
//! fn damage_system(mut writer: EventWriter<DamageEvent>) {
//!     writer.send(DamageEvent { entity, amount: 10 });
//! }
//!
//! // In a receiving system
//! fn health_system(mut reader: EventReader<DamageEvent>) {
//!     for event in reader.read() {
//!         apply_damage(event.entity, event.amount);
//!     }
//! }
//! ```

use std::any::{Any, TypeId};
use std::collections::VecDeque;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::RwLock;
use rustc_hash::FxHashMap;

/// Priority level for events. Higher priority events are processed first.
pub type EventPriority = u32;

/// Default priority for events.
pub const DEFAULT_PRIORITY: EventPriority = 0;

/// Marker trait for event types.
///
/// All event types must implement this trait to be used with the event bus.
/// The trait requires `Send + Sync + 'static + Clone` for thread safety.
pub trait Event: Send + Sync + Clone + 'static {}

impl<T: Send + Sync + Clone + 'static> Event for T {}

/// Internal storage for events of a specific type with priority ordering.
struct EventQueue<T> {
    events: VecDeque<(EventPriority, T)>,
}

#[allow(dead_code)]
impl<T> EventQueue<T> {
    fn new() -> Self {
        Self {
            events: VecDeque::new(),
        }
    }

    fn push(&mut self, priority: EventPriority, event: T) {
        let insert_pos = self
            .events
            .iter()
            .position(|(p, _)| priority > *p)
            .unwrap_or(self.events.len());
        self.events.insert(insert_pos, (priority, event));
    }

    fn extend_ordered(&mut self, events: Vec<(EventPriority, T)>) {
        for (priority, event) in events {
            self.push(priority, event);
        }
    }

    fn len(&self) -> usize {
        self.events.len()
    }

    fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    fn clear(&mut self) {
        self.events.clear();
    }
}

/// Type-erased event channel storage.
#[allow(dead_code)]
trait EventChannel: Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn clear(&mut self);
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
}

impl<T: Event> EventChannel for RwLock<EventQueue<T>> {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn clear(&mut self) {
        self.write().clear();
    }

    fn len(&self) -> usize {
        self.read().len()
    }

    fn is_empty(&self) -> bool {
        self.read().is_empty()
    }
}

/// Global reader ID counter.
static READER_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generates a unique reader ID.
fn next_reader_id() -> u64 {
    READER_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Reader state for tracking which events have been consumed.
#[derive(Debug, Clone)]
struct ReaderState {
    reader_id: u64,
    last_seen_index: usize,
}

/// Central event bus resource for storing and distributing events.
///
/// The `EventBus` is inserted as a resource into the `World` and provides
/// type-erased storage for events of any type implementing `Event`.
/// Events are automatically cleared at the end of each frame.
///
/// # Thread Safety
///
/// The event bus uses `RwLock` internally to allow concurrent reads from
/// multiple `EventReader`s while ensuring safe writes from `EventWriter`s.
pub struct EventBus {
    channels: FxHashMap<TypeId, Box<dyn EventChannel>>,
    reader_states: RwLock<FxHashMap<TypeId, Vec<ReaderState>>>,
}

impl EventBus {
    /// Creates a new empty event bus.
    pub fn new() -> Self {
        Self {
            channels: FxHashMap::default(),
            reader_states: RwLock::new(FxHashMap::default()),
        }
    }

    /// Sends an event with default priority.
    pub fn send<T: Event>(&mut self, event: T) {
        self.send_with_priority(event, DEFAULT_PRIORITY);
    }

    /// Sends an event with a specific priority.
    ///
    /// Higher priority events are processed first by readers.
    pub fn send_with_priority<T: Event>(&mut self, event: T, priority: EventPriority) {
        let type_id = TypeId::of::<T>();
        self.ensure_channel::<T>();
        if let Some(channel) = self.channels.get(&type_id) {
            if let Some(queue) = channel.as_any().downcast_ref::<RwLock<EventQueue<T>>>() {
                queue.write().push(priority, event);
            }
        }
    }

    /// Sends multiple events with default priority.
    pub fn send_batch<T: Event>(&mut self, events: Vec<T>) {
        self.send_batch_with_priority(events, DEFAULT_PRIORITY);
    }

    /// Sends multiple events with a specific priority.
    pub fn send_batch_with_priority<T: Event>(&mut self, events: Vec<T>, priority: EventPriority) {
        let type_id = TypeId::of::<T>();
        self.ensure_channel::<T>();
        if let Some(channel) = self.channels.get(&type_id) {
            if let Some(queue) = channel.as_any().downcast_ref::<RwLock<EventQueue<T>>>() {
                let prioritized: Vec<_> = events.into_iter().map(|e| (priority, e)).collect();
                queue.write().extend_ordered(prioritized);
            }
        }
    }

    /// Registers a new reader for event type T and returns a reader ID.
    pub fn register_reader<T: Event>(&self) -> u64 {
        let type_id = TypeId::of::<T>();
        let reader_id = next_reader_id();
        let mut states = self.reader_states.write();
        let readers = states.entry(type_id).or_default();
        readers.push(ReaderState {
            reader_id,
            last_seen_index: 0,
        });
        reader_id
    }

    /// Reads events for a specific reader, returning cloned events.
    pub fn read<T: Event>(&self, reader_id: u64) -> Vec<T> {
        let type_id = TypeId::of::<T>();
        let start_index = {
            let states = self.reader_states.read();
            states
                .get(&type_id)
                .and_then(|readers| readers.iter().find(|r| r.reader_id == reader_id))
                .map(|r| r.last_seen_index)
                .unwrap_or(0)
        };

        let events: Vec<T> = if let Some(channel) = self.channels.get(&type_id) {
            if let Some(queue) = channel.as_any().downcast_ref::<RwLock<EventQueue<T>>>() {
                let queue = queue.read();
                queue
                    .events
                    .iter()
                    .skip(start_index)
                    .map(|(_, e)| e.clone())
                    .collect()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        let new_index = start_index + events.len();
        {
            let mut states = self.reader_states.write();
            if let Some(readers) = states.get_mut(&type_id) {
                if let Some(reader) = readers.iter_mut().find(|r| r.reader_id == reader_id) {
                    reader.last_seen_index = new_index;
                }
            }
        }

        events
    }

    /// Clears all events of type T.
    pub fn clear<T: Event>(&mut self) {
        let type_id = TypeId::of::<T>();
        if let Some(channel) = self.channels.get_mut(&type_id) {
            channel.clear();
        }
        let mut states = self.reader_states.write();
        if let Some(readers) = states.get_mut(&type_id) {
            for reader in readers {
                reader.last_seen_index = 0;
            }
        }
    }

    /// Clears all events and resets all reader positions.
    pub fn clear_all(&mut self) {
        for channel in self.channels.values_mut() {
            channel.clear();
        }
        let mut states = self.reader_states.write();
        for readers in states.values_mut() {
            for reader in readers {
                reader.last_seen_index = 0;
            }
        }
    }

    /// Returns the number of pending events of type T.
    pub fn len<T: Event>(&self) -> usize {
        let type_id = TypeId::of::<T>();
        self.channels.get(&type_id).map(|c| c.len()).unwrap_or(0)
    }

    /// Returns true if there are no pending events of type T.
    pub fn is_empty<T: Event>(&self) -> bool {
        self.len::<T>() == 0
    }

    /// Returns the total number of events across all types.
    pub fn total_events(&self) -> usize {
        self.channels.values().map(|c| c.len()).sum()
    }

    fn ensure_channel<T: Event>(&mut self) {
        let type_id = TypeId::of::<T>();
        self.channels
            .entry(type_id)
            .or_insert_with(|| Box::new(RwLock::new(EventQueue::<T>::new())));
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

/// System parameter for reading events of type T.
///
/// Each `EventReader` maintains its own read position, allowing multiple
/// systems to independently read the same events without interference.
///
/// # Example
///
/// ```rust,ignore
/// fn system(mut reader: EventReader<DamageEvent>) {
///     for event in reader.read() {
///         println!("Damage: {}", event.amount);
///     }
/// }
/// ```
pub struct EventReader<'a, T: Event> {
    bus: &'a EventBus,
    reader_id: u64,
    _marker: PhantomData<T>,
}

impl<'a, T: Event> EventReader<'a, T> {
    /// Creates a new event reader for the given event bus.
    pub fn new(bus: &'a EventBus) -> Self {
        let reader_id = bus.register_reader::<T>();
        Self {
            bus,
            reader_id,
            _marker: PhantomData,
        }
    }

    /// Creates an event reader with an existing reader ID.
    pub fn with_reader_id(bus: &'a EventBus, reader_id: u64) -> Self {
        Self {
            bus,
            reader_id,
            _marker: PhantomData,
        }
    }

    /// Iterates over all unread events of type T.
    pub fn read(&self) -> EventIter<T> {
        let events = self.bus.read::<T>(self.reader_id);
        EventIter {
            events: events.into_iter(),
            _marker: PhantomData,
        }
    }

    /// Returns the reader ID for this reader.
    pub fn reader_id(&self) -> u64 {
        self.reader_id
    }
}

/// Iterator over events from an `EventReader`.
pub struct EventIter<T: Event> {
    events: std::vec::IntoIter<T>,
    _marker: PhantomData<T>,
}

impl<T: Event> Iterator for EventIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.events.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.events.size_hint()
    }
}

impl<T: Event> ExactSizeIterator for EventIter<T> {}

/// System parameter for writing events of type T.
///
/// `EventWriter` provides methods to send single events or batches,
/// with optional priority ordering.
///
/// # Example
///
/// ```rust,ignore
/// fn system(mut writer: EventWriter<DamageEvent>) {
///     writer.send(DamageEvent { amount: 10 });
///     writer.send_batch(vec![
///         DamageEvent { amount: 5 },
///         DamageEvent { amount: 15 },
///     ]);
/// }
/// ```
pub struct EventWriter<'a, T: Event> {
    bus: &'a mut EventBus,
    _marker: PhantomData<T>,
}

impl<'a, T: Event> EventWriter<'a, T> {
    /// Creates a new event writer for the given event bus.
    pub fn new(bus: &'a mut EventBus) -> Self {
        Self {
            bus,
            _marker: PhantomData,
        }
    }

    /// Sends a single event with default priority.
    pub fn send(&mut self, event: T) {
        self.bus.send(event);
    }

    /// Sends a single event with a specific priority.
    pub fn send_with_priority(&mut self, event: T, priority: EventPriority) {
        self.bus.send_with_priority(event, priority);
    }

    /// Sends multiple events with default priority.
    pub fn send_batch(&mut self, events: Vec<T>) {
        self.bus.send_batch(events);
    }

    /// Sends multiple events with a specific priority.
    pub fn send_batch_with_priority(&mut self, events: Vec<T>, priority: EventPriority) {
        self.bus.send_batch_with_priority(events, priority);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    struct TestEvent {
        value: i32,
    }

    #[derive(Debug, Clone, PartialEq)]
    struct AnotherEvent {
        message: String,
    }

    #[test]
    fn event_bus_send_and_read() {
        let mut bus = EventBus::new();
        bus.send(TestEvent { value: 42 });
        bus.send(TestEvent { value: 100 });

        let reader_id = bus.register_reader::<TestEvent>();
        let events: Vec<_> = bus.read::<TestEvent>(reader_id);

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].value, 42);
        assert_eq!(events[1].value, 100);
    }

    #[test]
    fn event_bus_clear() {
        let mut bus = EventBus::new();
        bus.send(TestEvent { value: 1 });
        bus.send(TestEvent { value: 2 });

        bus.clear::<TestEvent>();

        assert!(bus.is_empty::<TestEvent>());
    }

    #[test]
    fn event_bus_clear_all() {
        let mut bus = EventBus::new();
        bus.send(TestEvent { value: 1 });
        bus.send(AnotherEvent {
            message: "hello".to_string(),
        });

        bus.clear_all();

        assert!(bus.is_empty::<TestEvent>());
        assert!(bus.is_empty::<AnotherEvent>());
    }

    #[test]
    fn multiple_event_types() {
        let mut bus = EventBus::new();
        bus.send(TestEvent { value: 10 });
        bus.send(AnotherEvent {
            message: "test".to_string(),
        });
        bus.send(TestEvent { value: 20 });

        let reader1 = bus.register_reader::<TestEvent>();
        let reader2 = bus.register_reader::<AnotherEvent>();

        let test_events: Vec<_> = bus
            .read::<TestEvent>(reader1)
            .iter()
            .map(|e| e.value)
            .collect();
        let other_events: Vec<_> = bus
            .read::<AnotherEvent>(reader2)
            .iter()
            .map(|e| e.message.clone())
            .collect();

        assert_eq!(test_events, vec![10, 20]);
        assert_eq!(other_events, vec!["test"]);
    }

    #[test]
    fn priority_ordering() {
        let mut bus = EventBus::new();
        bus.send_with_priority(TestEvent { value: 1 }, 0);
        bus.send_with_priority(TestEvent { value: 2 }, 10);
        bus.send_with_priority(TestEvent { value: 3 }, 5);
        bus.send_with_priority(TestEvent { value: 4 }, 10);

        let reader_id = bus.register_reader::<TestEvent>();
        let events: Vec<_> = bus
            .read::<TestEvent>(reader_id)
            .iter()
            .map(|e| e.value)
            .collect();

        assert_eq!(events, vec![2, 4, 3, 1]);
    }

    #[test]
    fn batch_sending() {
        let mut bus = EventBus::new();
        bus.send_batch(vec![
            TestEvent { value: 1 },
            TestEvent { value: 2 },
            TestEvent { value: 3 },
        ]);

        let reader_id = bus.register_reader::<TestEvent>();
        let events: Vec<_> = bus
            .read::<TestEvent>(reader_id)
            .iter()
            .map(|e| e.value)
            .collect();

        assert_eq!(events, vec![1, 2, 3]);
    }

    #[test]
    fn batch_with_priority() {
        let mut bus = EventBus::new();
        bus.send(TestEvent { value: 0 });
        bus.send_batch_with_priority(vec![TestEvent { value: 10 }, TestEvent { value: 20 }], 100);

        let reader_id = bus.register_reader::<TestEvent>();
        let events: Vec<_> = bus
            .read::<TestEvent>(reader_id)
            .iter()
            .map(|e| e.value)
            .collect();

        assert_eq!(events, vec![10, 20, 0]);
    }

    #[test]
    fn multiple_readers_independent() {
        let mut bus = EventBus::new();

        bus.send(TestEvent { value: 1 });
        bus.send(TestEvent { value: 2 });

        let reader1 = bus.register_reader::<TestEvent>();
        let reader2 = bus.register_reader::<TestEvent>();

        let events1: Vec<_> = bus
            .read::<TestEvent>(reader1)
            .iter()
            .map(|e| e.value)
            .collect();
        assert_eq!(events1, vec![1, 2]);

        bus.send(TestEvent { value: 3 });

        let events1_again: Vec<_> = bus
            .read::<TestEvent>(reader1)
            .iter()
            .map(|e| e.value)
            .collect();
        let events2: Vec<_> = bus
            .read::<TestEvent>(reader2)
            .iter()
            .map(|e| e.value)
            .collect();

        assert_eq!(events1_again, vec![3]);
        assert_eq!(events2, vec![1, 2, 3]);
    }

    #[test]
    fn event_reader_wrapper() {
        let mut bus = EventBus::new();
        bus.send(TestEvent { value: 42 });

        let reader = EventReader::<TestEvent>::new(&bus);
        let events: Vec<_> = reader.read().map(|e| e.value).collect();

        assert_eq!(events, vec![42]);
    }

    #[test]
    fn event_writer_wrapper() {
        let mut bus = EventBus::new();
        {
            let mut writer = EventWriter::new(&mut bus);
            writer.send(TestEvent { value: 10 });
            writer.send_batch(vec![TestEvent { value: 20 }, TestEvent { value: 30 }]);
        }

        let reader = EventReader::<TestEvent>::new(&bus);
        let events: Vec<_> = reader.read().map(|e| e.value).collect();

        assert_eq!(events, vec![10, 20, 30]);
    }

    #[test]
    fn event_writer_priority() {
        let mut bus = EventBus::new();
        {
            let mut writer = EventWriter::new(&mut bus);
            writer.send(TestEvent { value: 1 });
            writer.send_with_priority(TestEvent { value: 99 }, 100);
            writer.send(TestEvent { value: 2 });
        }

        let reader = EventReader::<TestEvent>::new(&bus);
        let events: Vec<_> = reader.read().map(|e| e.value).collect();

        assert_eq!(events, vec![99, 1, 2]);
    }

    #[test]
    fn len_and_is_empty() {
        let mut bus = EventBus::new();

        assert!(bus.is_empty::<TestEvent>());
        assert_eq!(bus.len::<TestEvent>(), 0);

        bus.send(TestEvent { value: 1 });
        bus.send(TestEvent { value: 2 });

        assert!(!bus.is_empty::<TestEvent>());
        assert_eq!(bus.len::<TestEvent>(), 2);
    }

    #[test]
    fn total_events() {
        let mut bus = EventBus::new();
        assert_eq!(bus.total_events(), 0);

        bus.send(TestEvent { value: 1 });
        bus.send(TestEvent { value: 2 });
        bus.send(AnotherEvent {
            message: "hello".to_string(),
        });

        assert_eq!(bus.total_events(), 3);
    }

    #[test]
    fn reader_position_reset_on_clear() {
        let mut bus = EventBus::new();
        bus.send(TestEvent { value: 1 });

        let reader_id = bus.register_reader::<TestEvent>();
        let _ = bus.read::<TestEvent>(reader_id);

        bus.send(TestEvent { value: 2 });
        bus.clear::<TestEvent>();
        bus.send(TestEvent { value: 3 });

        let events: Vec<_> = bus
            .read::<TestEvent>(reader_id)
            .iter()
            .map(|e| e.value)
            .collect();
        assert_eq!(events, vec![3]);
    }

    #[test]
    fn default_event_bus() {
        let bus = EventBus::default();
        assert!(bus.is_empty::<TestEvent>());
    }

    #[test]
    fn event_iter_size_hint() {
        let mut bus = EventBus::new();
        bus.send(TestEvent { value: 1 });
        bus.send(TestEvent { value: 2 });
        bus.send(TestEvent { value: 3 });

        let reader = EventReader::<TestEvent>::new(&bus);
        let iter = reader.read();

        assert_eq!(iter.len(), 3);
    }

    #[test]
    fn zero_priority_events() {
        let mut bus = EventBus::new();
        bus.send_with_priority(TestEvent { value: 1 }, 0);
        bus.send_with_priority(TestEvent { value: 2 }, 0);
        bus.send_with_priority(TestEvent { value: 3 }, 0);

        let reader_id = bus.register_reader::<TestEvent>();
        let events: Vec<_> = bus
            .read::<TestEvent>(reader_id)
            .iter()
            .map(|e| e.value)
            .collect();

        assert_eq!(events, vec![1, 2, 3]);
    }

    #[test]
    fn event_reader_id_unique() {
        let bus = EventBus::new();

        let reader1 = EventReader::<TestEvent>::new(&bus);
        let reader2 = EventReader::<TestEvent>::new(&bus);

        assert_ne!(reader1.reader_id(), reader2.reader_id());
    }

    #[test]
    fn event_reader_with_existing_id() {
        let bus = EventBus::new();
        let reader_id = bus.register_reader::<TestEvent>();

        let reader = EventReader::<TestEvent>::with_reader_id(&bus, reader_id);
        assert_eq!(reader.reader_id(), reader_id);
    }
}
