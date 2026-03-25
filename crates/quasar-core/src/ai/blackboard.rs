//! Blackboard - Shared memory for AI decision making.
//!
//! The blackboard provides a key-value store that AI systems can read from
//! and write to, enabling communication between different parts of the
//! behavior tree and other game systems.

use std::collections::HashMap;

/// A value stored in the blackboard.
#[derive(Debug, Clone)]
pub enum BlackboardValue {
    Bool(bool),
    Int(i64),
    Float(f32),
    String(String),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Entity(u64),
    Timestamp(u64),
    Duration(f32),
}

impl BlackboardValue {
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            BlackboardValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            BlackboardValue::Int(i) => Some(*i),
            BlackboardValue::Float(f) => Some(*f as i64),
            _ => None,
        }
    }

    pub fn as_float(&self) -> Option<f32> {
        match self {
            BlackboardValue::Float(f) => Some(*f),
            BlackboardValue::Int(i) => Some(*i as f32),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&str> {
        match self {
            BlackboardValue::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_vec2(&self) -> Option<[f32; 2]> {
        match self {
            BlackboardValue::Vec2(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_vec3(&self) -> Option<[f32; 3]> {
        match self {
            BlackboardValue::Vec3(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_entity(&self) -> Option<u64> {
        match self {
            BlackboardValue::Entity(e) => Some(*e),
            _ => None,
        }
    }
}

/// The blackboard - shared memory for AI.
#[derive(Debug, Clone, Default)]
pub struct Blackboard {
    values: HashMap<String, BlackboardValue>,
    dirty_keys: Vec<String>,
}

impl Blackboard {
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
            dirty_keys: Vec::new(),
        }
    }

    pub fn set<K: Into<String>>(&mut self, key: K, value: BlackboardValue) {
        let key = key.into();
        self.values.insert(key.clone(), value);
        if !self.dirty_keys.contains(&key) {
            self.dirty_keys.push(key);
        }
    }

    pub fn get(&self, key: &str) -> Option<&BlackboardValue> {
        self.values.get(key)
    }

    pub fn get_bool(&self, key: &str) -> bool {
        self.get(key).and_then(|v| v.as_bool()).unwrap_or(false)
    }

    pub fn get_float(&self, key: &str) -> f32 {
        self.get(key).and_then(|v| v.as_float()).unwrap_or(0.0)
    }

    pub fn get_int(&self, key: &str) -> i64 {
        self.get(key).and_then(|v| v.as_int()).unwrap_or(0)
    }

    pub fn get_string(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(|v| v.as_string())
    }

    pub fn get_vec3(&self, key: &str) -> [f32; 3] {
        self.get(key).and_then(|v| v.as_vec3()).unwrap_or([0.0; 3])
    }

    pub fn get_entity(&self, key: &str) -> Option<u64> {
        self.get(key).and_then(|v| v.as_entity())
    }

    pub fn contains(&self, key: &str) -> bool {
        self.values.contains_key(key)
    }

    pub fn remove(&mut self, key: &str) -> Option<BlackboardValue> {
        self.values.remove(key)
    }

    pub fn clear(&mut self) {
        self.values.clear();
        self.dirty_keys.clear();
    }

    pub fn clear_dirty(&mut self) {
        self.dirty_keys.clear();
    }

    pub fn dirty_keys(&self) -> &[String] {
        &self.dirty_keys
    }

    pub fn is_dirty(&self, key: &str) -> bool {
        self.dirty_keys.contains(&key.to_string())
    }

    pub fn increment(&mut self, key: &str, amount: i64) {
        let current = self.get_int(key);
        self.set(key, BlackboardValue::Int(current + amount));
    }

    pub fn increment_float(&mut self, key: &str, amount: f32) {
        let current = self.get_float(key);
        self.set(key, BlackboardValue::Float(current + amount));
    }

    pub fn toggle(&mut self, key: &str) {
        let current = self.get_bool(key);
        self.set(key, BlackboardValue::Bool(!current));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blackboard_set_get() {
        let mut bb = Blackboard::new();
        bb.set("health", BlackboardValue::Int(100));
        assert_eq!(bb.get_int("health"), 100);
    }

    #[test]
    fn blackboard_missing_key() {
        let bb = Blackboard::new();
        assert_eq!(bb.get_int("missing"), 0);
        assert_eq!(bb.get_float("missing"), 0.0);
        assert_eq!(bb.get_bool("missing"), false);
    }

    #[test]
    fn blackboard_dirty_tracking() {
        let mut bb = Blackboard::new();
        bb.set("test", BlackboardValue::Bool(true));
        assert!(bb.is_dirty("test"));
        bb.clear_dirty();
        assert!(!bb.is_dirty("test"));
    }

    #[test]
    fn blackboard_increment() {
        let mut bb = Blackboard::new();
        bb.set("counter", BlackboardValue::Int(5));
        bb.increment("counter", 3);
        assert_eq!(bb.get_int("counter"), 8);
    }

    #[test]
    fn blackboard_toggle() {
        let mut bb = Blackboard::new();
        bb.set("flag", BlackboardValue::Bool(false));
        bb.toggle("flag");
        assert!(bb.get_bool("flag"));
        bb.toggle("flag");
        assert!(!bb.get_bool("flag"));
    }

    #[test]
    fn blackboard_value_conversions() {
        let v = BlackboardValue::Int(42);
        assert_eq!(v.as_int(), Some(42));
        assert_eq!(v.as_float(), Some(42.0));

        let v = BlackboardValue::Float(3.14);
        assert_eq!(v.as_int(), Some(3));
        assert_eq!(v.as_float(), Some(3.14));

        let v = BlackboardValue::Vec3([1.0, 2.0, 3.0]);
        assert_eq!(v.as_vec3(), Some([1.0, 2.0, 3.0]));
    }
}
