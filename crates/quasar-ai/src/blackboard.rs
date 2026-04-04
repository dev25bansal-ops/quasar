//! Blackboard - Shared Knowledge System for AI Agents
//!
//! A blackboard is a shared data structure that allows different AI systems
//! to communicate and share knowledge about the game world.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::Hash;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlackboardKey(pub u64);

impl BlackboardKey {
    pub fn new(name: &str) -> Self {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        name.hash(&mut hasher);
        Self(hasher.finish())
    }

    pub fn from_u64(id: u64) -> Self {
        Self(id)
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[derive(Default)]
pub enum BlackboardValue {
    Bool(bool),
    Int(i64),
    Float(f32),
    String(String),
    Vec3([f32; 3]),
    Entity(u64),
    List(Vec<BlackboardValue>),
    Map(HashMap<String, BlackboardValue>),
    #[default]
    Null,
}


impl BlackboardValue {
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(i) => Some(*i),
            Self::Float(f) => Some(*f as i64),
            _ => None,
        }
    }

    pub fn as_float(&self) -> Option<f32> {
        match self {
            Self::Float(f) => Some(*f),
            Self::Int(i) => Some(*i as f32),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_vec3(&self) -> Option<[f32; 3]> {
        match self {
            Self::Vec3(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_entity(&self) -> Option<u64> {
        match self {
            Self::Entity(e) => Some(*e),
            _ => None,
        }
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            Self::Bool(b) => *b,
            Self::Int(i) => *i != 0,
            Self::Float(f) => *f != 0.0,
            Self::String(s) => !s.is_empty(),
            Self::Null => false,
            _ => true,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Bool(_) => "bool",
            Self::Int(_) => "int",
            Self::Float(_) => "float",
            Self::String(_) => "string",
            Self::Vec3(_) => "vec3",
            Self::Entity(_) => "entity",
            Self::List(_) => "list",
            Self::Map(_) => "map",
            Self::Null => "null",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlackboardEntry {
    pub value: BlackboardValue,
    pub timestamp: f64,
    pub changed_count: u32,
}

#[derive(Debug, Clone)]
pub struct Blackboard {
    entries: HashMap<BlackboardKey, BlackboardEntry>,
    name_to_key: HashMap<String, BlackboardKey>,
    current_time: f64,
}

impl Default for Blackboard {
    fn default() -> Self {
        Self::new()
    }
}

impl Blackboard {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            name_to_key: HashMap::new(),
            current_time: 0.0,
        }
    }

    pub fn set_time(&mut self, time: f64) {
        self.current_time = time;
    }

    pub fn insert(&mut self, key: &str, value: BlackboardValue) -> BlackboardKey {
        let key_id = BlackboardKey::new(key);
        self.name_to_key.insert(key.to_string(), key_id);

        let entry = self.entries.entry(key_id).or_insert(BlackboardEntry {
            value: BlackboardValue::Null,
            timestamp: self.current_time,
            changed_count: 0,
        });

        if entry.value != value {
            entry.value = value;
            entry.timestamp = self.current_time;
            entry.changed_count += 1;
        }

        key_id
    }

    pub fn get(&self, key: &str) -> Option<&BlackboardValue> {
        self.name_to_key
            .get(key)
            .and_then(|k| self.entries.get(k))
            .map(|e| &e.value)
    }

    pub fn get_key(&self, key: BlackboardKey) -> Option<&BlackboardValue> {
        self.entries.get(&key).map(|e| &e.value)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut BlackboardValue> {
        self.name_to_key
            .get(key)
            .and_then(|k| self.entries.get_mut(k))
            .map(|e| &mut e.value)
    }

    pub fn remove(&mut self, key: &str) -> Option<BlackboardValue> {
        self.name_to_key
            .remove(key)
            .and_then(|k| self.entries.remove(&k))
            .map(|e| e.value)
    }

    pub fn contains(&self, key: &str) -> bool {
        self.name_to_key
            .get(key)
            .map(|k| self.entries.contains_key(k))
            .unwrap_or(false)
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.name_to_key.keys().map(|s| s.as_str())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.name_to_key.clear();
    }

    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.get(key)?.as_bool()
    }

    pub fn get_int(&self, key: &str) -> Option<i64> {
        self.get(key)?.as_int()
    }

    pub fn get_float(&self, key: &str) -> Option<f32> {
        self.get(key)?.as_float()
    }

    pub fn get_string(&self, key: &str) -> Option<&str> {
        self.get(key)?.as_string()
    }

    pub fn get_vec3(&self, key: &str) -> Option<[f32; 3]> {
        self.get(key)?.as_vec3()
    }

    pub fn get_entity(&self, key: &str) -> Option<u64> {
        self.get(key)?.as_entity()
    }

    pub fn set_bool(&mut self, key: &str, value: bool) -> BlackboardKey {
        self.insert(key, BlackboardValue::Bool(value))
    }

    pub fn set_int(&mut self, key: &str, value: i64) -> BlackboardKey {
        self.insert(key, BlackboardValue::Int(value))
    }

    pub fn set_float(&mut self, key: &str, value: f32) -> BlackboardKey {
        self.insert(key, BlackboardValue::Float(value))
    }

    pub fn set_string(&mut self, key: &str, value: String) -> BlackboardKey {
        self.insert(key, BlackboardValue::String(value))
    }

    pub fn set_vec3(&mut self, key: &str, value: [f32; 3]) -> BlackboardKey {
        self.insert(key, BlackboardValue::Vec3(value))
    }

    pub fn set_entity(&mut self, key: &str, value: u64) -> BlackboardKey {
        self.insert(key, BlackboardValue::Entity(value))
    }

    pub fn changed_since(&self, key: &str, time: f64) -> bool {
        self.name_to_key
            .get(key)
            .and_then(|k| self.entries.get(k))
            .map(|e| e.timestamp > time)
            .unwrap_or(false)
    }

    pub fn get_timestamp(&self, key: &str) -> Option<f64> {
        self.name_to_key
            .get(key)
            .and_then(|k| self.entries.get(k))
            .map(|e| e.timestamp)
    }

    pub fn snapshot(&self) -> HashMap<String, BlackboardValue> {
        self.name_to_key
            .iter()
            .filter_map(|(name, key)| {
                self.entries
                    .get(key)
                    .map(|e| (name.clone(), e.value.clone()))
            })
            .collect()
    }

    pub fn restore(&mut self, snapshot: HashMap<String, BlackboardValue>) {
        self.clear();
        for (name, value) in snapshot {
            self.insert(&name, value);
        }
    }
}

impl Serialize for Blackboard {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.entries.len()))?;
        for (name, key) in &self.name_to_key {
            if let Some(entry) = self.entries.get(key) {
                map.serialize_entry(name, &entry.value)?;
            }
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for Blackboard {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let map: HashMap<String, BlackboardValue> = HashMap::deserialize(deserializer)?;
        let mut blackboard = Blackboard::new();
        for (name, value) in map {
            blackboard.insert(&name, value);
        }
        Ok(blackboard)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blackboard_basic_operations() {
        let mut bb = Blackboard::new();
        bb.set_bool("test_bool", true);
        bb.set_int("test_int", 42);
        bb.set_float("test_float", 3.14);
        bb.set_string("test_string", "hello".to_string());
        bb.set_vec3("test_vec3", [1.0, 2.0, 3.0]);

        assert_eq!(bb.get_bool("test_bool"), Some(true));
        assert_eq!(bb.get_int("test_int"), Some(42));
        assert!((bb.get_float("test_float").unwrap() - 3.14).abs() < 0.001);
        assert_eq!(bb.get_string("test_string"), Some("hello"));
        assert_eq!(bb.get_vec3("test_vec3"), Some([1.0, 2.0, 3.0]));
    }

    #[test]
    fn blackboard_type_conversions() {
        let mut bb = Blackboard::new();
        bb.set_int("value", 10);
        assert_eq!(bb.get_float("value"), Some(10.0));
        assert_eq!(bb.get_int("value"), Some(10));
    }

    #[test]
    fn blackboard_truthy() {
        assert!(BlackboardValue::Bool(true).is_truthy());
        assert!(!BlackboardValue::Bool(false).is_truthy());
        assert!(BlackboardValue::Int(1).is_truthy());
        assert!(!BlackboardValue::Int(0).is_truthy());
        assert!(BlackboardValue::String("hello".into()).is_truthy());
        assert!(!BlackboardValue::String("".into()).is_truthy());
    }

    #[test]
    fn blackboard_missing_key() {
        let bb = Blackboard::new();
        assert_eq!(bb.get_bool("missing"), None);
        assert!(!bb.contains("missing"));
    }

    #[test]
    fn blackboard_change_tracking() {
        let mut bb = Blackboard::new();
        bb.set_time(1.0);
        bb.set_bool("flag", false);

        bb.set_time(2.0);
        bb.set_bool("flag", true);

        assert!(bb.changed_since("flag", 1.0));
        assert!(!bb.changed_since("flag", 2.0));
    }

    #[test]
    fn blackboard_snapshot_restore() {
        let mut bb1 = Blackboard::new();
        bb1.set_bool("a", true);
        bb1.set_int("b", 42);

        let snapshot = bb1.snapshot();

        let mut bb2 = Blackboard::new();
        bb2.restore(snapshot);

        assert_eq!(bb2.get_bool("a"), Some(true));
        assert_eq!(bb2.get_int("b"), Some(42));
    }
}
