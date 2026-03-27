//! Input rebinding system for runtime key remapping.
//!
//! Provides:
//! - Save/load action bindings to configuration files
//! - Runtime rebinding with conflict detection
//! - Gamepad/controller support
//! - UI-friendly rebind state machine
//!
//! # Example
//!
//! ```ignore
//! use quasar_window::rebinding::*;
//!
//! // Load bindings from file
//! let mut config = InputRebindConfig::load("input_config.json").unwrap_or_default();
//!
//! // Start rebinding "jump" action
//! config.start_rebind("jump", None);
//!
//! // In game loop, poll for input
//! if let Some(binding) = config.poll_rebind(&input) {
//!     println!("Bound 'jump' to {:?}", binding);
//!     config.save("input_config.json").unwrap();
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use winit::keyboard::KeyCode;

use crate::action_map::InputBinding;
use crate::input::{Input, MouseButton};

/// Gamepad button for input bindings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GamepadButton {
    A,
    B,
    X,
    Y,
    LeftBumper,
    RightBumper,
    LeftTrigger,
    RightTrigger,
    LeftStick,
    RightStick,
    Start,
    Select,
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
}

/// Gamepad axis for analog input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GamepadAxis {
    LeftStickX,
    LeftStickY,
    RightStickX,
    RightStickY,
    LeftTrigger,
    RightTrigger,
}

/// Serializable key code (string representation).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SerializableKeyCode(pub String);

impl From<KeyCode> for SerializableKeyCode {
    fn from(code: KeyCode) -> Self {
        Self(format!("{:?}", code))
    }
}

impl TryFrom<SerializableKeyCode> for KeyCode {
    type Error = ();

    fn try_from(value: SerializableKeyCode) -> Result<Self, Self::Error> {
        keycode_from_str(&value.0).ok_or(())
    }
}

/// Serializable mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SerializableMouseButton {
    Left,
    Right,
    Middle,
}

impl From<MouseButton> for SerializableMouseButton {
    fn from(btn: MouseButton) -> Self {
        match btn {
            MouseButton::Left => Self::Left,
            MouseButton::Right => Self::Right,
            MouseButton::Middle => Self::Middle,
        }
    }
}

impl From<SerializableMouseButton> for MouseButton {
    fn from(btn: SerializableMouseButton) -> Self {
        match btn {
            SerializableMouseButton::Left => MouseButton::Left,
            SerializableMouseButton::Right => MouseButton::Right,
            SerializableMouseButton::Middle => MouseButton::Middle,
        }
    }
}

/// Extended input binding including gamepad support.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RebindBinding {
    Key(SerializableKeyCode),
    Mouse(SerializableMouseButton),
    GamepadButton(GamepadButton),
}

impl RebindBinding {
    pub fn from_key(key: KeyCode) -> Self {
        Self::Key(SerializableKeyCode::from(key))
    }

    pub fn from_mouse(button: MouseButton) -> Self {
        Self::Mouse(SerializableMouseButton::from(button))
    }

    pub fn to_input_binding(&self) -> Option<InputBinding> {
        match self {
            Self::Key(k) => KeyCode::try_from(k.clone()).ok().map(InputBinding::Key),
            Self::Mouse(m) => Some(InputBinding::Mouse(MouseButton::from(*m))),
            Self::GamepadButton(_) => None,
        }
    }
}

impl TryFrom<RebindBinding> for InputBinding {
    type Error = ();

    fn try_from(binding: RebindBinding) -> Result<Self, Self::Error> {
        binding.to_input_binding().ok_or(())
    }
}

/// State of the rebinding process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RebindState {
    #[default]
    Idle,
    WaitingForInput,
    Cancelled,
    Completed,
}

/// Configuration for input rebinding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputRebindConfig {
    pub bindings: HashMap<String, Vec<RebindBinding>>,
    #[serde(skip)]
    pub rebind_action: Option<String>,
    #[serde(skip)]
    pub rebind_state: RebindState,
    #[serde(skip)]
    pub rebind_index: Option<usize>,
    #[serde(skip)]
    pub conflict_actions: HashSet<String>,
    pub allow_duplicates: bool,
    pub rebind_timeout: f32,
    #[serde(skip)]
    pub rebind_elapsed: f32,
}

impl Default for InputRebindConfig {
    fn default() -> Self {
        let mut bindings = HashMap::new();

        bindings.insert(
            "move_forward".to_string(),
            vec![
                RebindBinding::from_key(KeyCode::KeyW),
                RebindBinding::GamepadButton(GamepadButton::DPadUp),
            ],
        );
        bindings.insert(
            "move_backward".to_string(),
            vec![
                RebindBinding::from_key(KeyCode::KeyS),
                RebindBinding::GamepadButton(GamepadButton::DPadDown),
            ],
        );
        bindings.insert(
            "move_left".to_string(),
            vec![
                RebindBinding::from_key(KeyCode::KeyA),
                RebindBinding::GamepadButton(GamepadButton::DPadLeft),
            ],
        );
        bindings.insert(
            "move_right".to_string(),
            vec![
                RebindBinding::from_key(KeyCode::KeyD),
                RebindBinding::GamepadButton(GamepadButton::DPadRight),
            ],
        );
        bindings.insert(
            "jump".to_string(),
            vec![
                RebindBinding::from_key(KeyCode::Space),
                RebindBinding::GamepadButton(GamepadButton::A),
            ],
        );
        bindings.insert(
            "interact".to_string(),
            vec![
                RebindBinding::from_key(KeyCode::KeyE),
                RebindBinding::GamepadButton(GamepadButton::X),
            ],
        );
        bindings.insert(
            "pause".to_string(),
            vec![
                RebindBinding::from_key(KeyCode::Escape),
                RebindBinding::GamepadButton(GamepadButton::Start),
            ],
        );
        bindings.insert(
            "attack".to_string(),
            vec![
                RebindBinding::from_mouse(MouseButton::Left),
                RebindBinding::GamepadButton(GamepadButton::RightTrigger),
            ],
        );
        bindings.insert(
            "aim".to_string(),
            vec![
                RebindBinding::from_mouse(MouseButton::Right),
                RebindBinding::GamepadButton(GamepadButton::LeftTrigger),
            ],
        );

        Self {
            bindings,
            rebind_action: None,
            rebind_state: RebindState::Idle,
            rebind_index: None,
            conflict_actions: HashSet::new(),
            allow_duplicates: false,
            rebind_timeout: 5.0,
            rebind_elapsed: 0.0,
        }
    }
}

impl InputRebindConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, std::io::Error> {
        let json = std::fs::read_to_string(path)?;
        let config: Self = serde_json::from_str(&json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(config)
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), std::io::Error> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, json)
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn get_bindings(&self, action: &str) -> Option<&[RebindBinding]> {
        self.bindings.get(action).map(|v| v.as_slice())
    }

    pub fn add_binding(&mut self, action: &str, binding: RebindBinding) {
        self.bindings
            .entry(action.to_string())
            .or_default()
            .push(binding);
    }

    pub fn remove_binding(&mut self, action: &str, index: usize) -> Option<RebindBinding> {
        if let Some(bindings) = self.bindings.get_mut(action) {
            if index < bindings.len() {
                let removed = bindings.remove(index);
                if bindings.is_empty() {
                    self.bindings.remove(action);
                }
                Some(removed)
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn clear_action(&mut self, action: &str) {
        self.bindings.remove(action);
    }

    pub fn clear_all(&mut self) {
        self.bindings.clear();
    }

    pub fn reset_to_defaults(&mut self) {
        *self = Self::default();
    }

    pub fn start_rebind(&mut self, action: &str, index: Option<usize>) {
        self.rebind_action = Some(action.to_string());
        self.rebind_index = index;
        self.rebind_state = RebindState::WaitingForInput;
        self.rebind_elapsed = 0.0;
    }

    pub fn cancel_rebind(&mut self) {
        self.rebind_action = None;
        self.rebind_index = None;
        self.rebind_state = RebindState::Cancelled;
        self.rebind_elapsed = 0.0;
    }

    pub fn poll_rebind(&mut self, input: &Input) -> Option<RebindBinding> {
        if self.rebind_state != RebindState::WaitingForInput {
            return None;
        }

        let binding = self.detect_input(input)?;

        if self.rebind_timeout > 0.0 {
            self.rebind_elapsed += 1.0 / 60.0;
            if self.rebind_elapsed >= self.rebind_timeout {
                self.cancel_rebind();
                return None;
            }
        }

        if !self.allow_duplicates {
            if let Some(conflict) = self.find_conflict(&binding) {
                self.conflict_actions.insert(conflict);
                return None;
            }
        }

        let action = self.rebind_action.clone()?;
        let index = self.rebind_index;

        if let Some(idx) = index {
            if let Some(bindings) = self.bindings.get_mut(&action) {
                if idx < bindings.len() {
                    bindings[idx] = binding.clone();
                }
            }
        } else {
            self.add_binding(&action, binding.clone());
        }

        self.rebind_action = None;
        self.rebind_index = None;
        self.rebind_state = RebindState::Completed;
        self.rebind_elapsed = 0.0;

        Some(binding)
    }

    pub fn update(&mut self, dt: f32) {
        if self.rebind_state == RebindState::WaitingForInput && self.rebind_timeout > 0.0 {
            self.rebind_elapsed += dt;
            if self.rebind_elapsed >= self.rebind_timeout {
                self.cancel_rebind();
            }
        }
    }

    fn detect_input(&self, input: &Input) -> Option<RebindBinding> {
        for key in KEY_LIST {
            if input.just_pressed(*key) {
                return Some(RebindBinding::from_key(*key));
            }
        }

        for button in [MouseButton::Left, MouseButton::Right, MouseButton::Middle] {
            if input.mouse_just_pressed(button) {
                return Some(RebindBinding::from_mouse(button));
            }
        }

        None
    }

    pub fn find_conflict(&self, binding: &RebindBinding) -> Option<String> {
        for (action, bindings) in &self.bindings {
            if bindings.contains(binding) {
                return Some(action.clone());
            }
        }
        None
    }

    pub fn find_all_conflicts(&self, binding: &RebindBinding) -> Vec<String> {
        self.bindings
            .iter()
            .filter(|(_, bindings)| bindings.contains(binding))
            .map(|(action, _)| action.clone())
            .collect()
    }

    pub fn is_rebinding(&self) -> bool {
        self.rebind_state == RebindState::WaitingForInput
    }

    pub fn rebind_action(&self) -> Option<&str> {
        self.rebind_action.as_deref()
    }

    pub fn rebind_state(&self) -> RebindState {
        self.rebind_state
    }

    pub fn rebind_remaining(&self) -> Option<f32> {
        if self.rebind_timeout > 0.0 && self.rebind_state == RebindState::WaitingForInput {
            Some((self.rebind_timeout - self.rebind_elapsed).max(0.0))
        } else {
            None
        }
    }

    pub fn is_pressed(&self, action: &str, input: &Input) -> bool {
        self.bindings
            .get(action)
            .is_some_and(|bs| bs.iter().any(|b| self.binding_pressed(b, input)))
    }

    pub fn just_pressed(&self, action: &str, input: &Input) -> bool {
        self.bindings
            .get(action)
            .is_some_and(|bs| bs.iter().any(|b| self.binding_just_pressed(b, input)))
    }

    pub fn just_released(&self, action: &str, input: &Input) -> bool {
        self.bindings
            .get(action)
            .is_some_and(|bs| bs.iter().any(|b| self.binding_just_released(b, input)))
    }

    fn binding_pressed(&self, binding: &RebindBinding, input: &Input) -> bool {
        match binding {
            RebindBinding::Key(k) => {
                if let Ok(key) = KeyCode::try_from(k.clone()) {
                    input.is_pressed(key)
                } else {
                    false
                }
            }
            RebindBinding::Mouse(m) => input.is_mouse_pressed(MouseButton::from(*m)),
            RebindBinding::GamepadButton(_) => false,
        }
    }

    fn binding_just_pressed(&self, binding: &RebindBinding, input: &Input) -> bool {
        match binding {
            RebindBinding::Key(k) => {
                if let Ok(key) = KeyCode::try_from(k.clone()) {
                    input.just_pressed(key)
                } else {
                    false
                }
            }
            RebindBinding::Mouse(m) => input.mouse_just_pressed(MouseButton::from(*m)),
            RebindBinding::GamepadButton(_) => false,
        }
    }

    fn binding_just_released(&self, binding: &RebindBinding, input: &Input) -> bool {
        match binding {
            RebindBinding::Key(k) => {
                if let Ok(key) = KeyCode::try_from(k.clone()) {
                    input.just_released(key)
                } else {
                    false
                }
            }
            RebindBinding::Mouse(m) => input.mouse_just_released(MouseButton::from(*m)),
            RebindBinding::GamepadButton(_) => false,
        }
    }

    pub fn to_action_map(&self) -> crate::action_map::ActionMap {
        let mut map = crate::action_map::ActionMap::new();
        for (action, bindings) in &self.bindings {
            for binding in bindings {
                if let Some(input_binding) = binding.to_input_binding() {
                    map.bind(action, input_binding);
                }
            }
        }
        map
    }

    pub fn binding_name(binding: &RebindBinding) -> String {
        match binding {
            RebindBinding::Key(k) => {
                let name = &k.0;
                name.strip_prefix("Key").unwrap_or(name).to_string()
            }
            RebindBinding::Mouse(m) => format!("{:?}", m),
            RebindBinding::GamepadButton(b) => format!("{:?}", b),
        }
    }

    pub fn action_binding_names(&self, action: &str) -> Vec<String> {
        self.bindings
            .get(action)
            .map(|bs| bs.iter().map(Self::binding_name).collect())
            .unwrap_or_default()
    }

    pub fn action_names(&self) -> Vec<&String> {
        self.bindings.keys().collect()
    }
}

const KEY_LIST: &[KeyCode] = &[
    KeyCode::Space,
    KeyCode::KeyA,
    KeyCode::KeyB,
    KeyCode::KeyC,
    KeyCode::KeyD,
    KeyCode::KeyE,
    KeyCode::KeyF,
    KeyCode::KeyG,
    KeyCode::KeyH,
    KeyCode::KeyI,
    KeyCode::KeyJ,
    KeyCode::KeyK,
    KeyCode::KeyL,
    KeyCode::KeyM,
    KeyCode::KeyN,
    KeyCode::KeyO,
    KeyCode::KeyP,
    KeyCode::KeyQ,
    KeyCode::KeyR,
    KeyCode::KeyS,
    KeyCode::KeyT,
    KeyCode::KeyU,
    KeyCode::KeyV,
    KeyCode::KeyW,
    KeyCode::KeyX,
    KeyCode::KeyY,
    KeyCode::KeyZ,
    KeyCode::Digit0,
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
    KeyCode::Digit7,
    KeyCode::Digit8,
    KeyCode::Digit9,
    KeyCode::F1,
    KeyCode::F2,
    KeyCode::F3,
    KeyCode::F4,
    KeyCode::F5,
    KeyCode::F6,
    KeyCode::F7,
    KeyCode::F8,
    KeyCode::F9,
    KeyCode::F10,
    KeyCode::F11,
    KeyCode::F12,
    KeyCode::Escape,
    KeyCode::Enter,
    KeyCode::Tab,
    KeyCode::Backspace,
    KeyCode::Insert,
    KeyCode::Delete,
    KeyCode::Home,
    KeyCode::End,
    KeyCode::PageUp,
    KeyCode::PageDown,
    KeyCode::ArrowUp,
    KeyCode::ArrowDown,
    KeyCode::ArrowLeft,
    KeyCode::ArrowRight,
    KeyCode::ShiftLeft,
    KeyCode::ShiftRight,
    KeyCode::ControlLeft,
    KeyCode::ControlRight,
    KeyCode::AltLeft,
    KeyCode::AltRight,
];

fn keycode_from_str(s: &str) -> Option<KeyCode> {
    for key in KEY_LIST {
        if format!("{:?}", key) == s {
            return Some(*key);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_bindings() {
        let config = InputRebindConfig::default();
        assert!(config.bindings.contains_key("jump"));
        assert!(config.bindings.contains_key("move_forward"));
    }

    #[test]
    fn add_and_remove_binding() {
        let mut config = InputRebindConfig::new();
        config.clear_all();

        config.add_binding("test", RebindBinding::from_key(KeyCode::KeyT));
        assert_eq!(config.get_bindings("test").unwrap().len(), 1);

        config.remove_binding("test", 0);
        assert!(config.get_bindings("test").is_none());
    }

    #[test]
    fn rebind_cycle() {
        let mut config = InputRebindConfig::new();
        config.clear_all();

        config.start_rebind("jump", None);
        assert!(config.is_rebinding());
        assert_eq!(config.rebind_action(), Some("jump"));

        config.cancel_rebind();
        assert!(!config.is_rebinding());
        assert_eq!(config.rebind_state, RebindState::Cancelled);
    }

    #[test]
    fn conflict_detection() {
        let config = InputRebindConfig::default();

        let conflict = config.find_conflict(&RebindBinding::from_key(KeyCode::Space));
        assert_eq!(conflict, Some("jump".to_string()));

        let no_conflict = config.find_conflict(&RebindBinding::from_key(KeyCode::KeyZ));
        assert!(no_conflict.is_none());
    }

    #[test]
    fn serialization_roundtrip() {
        let config = InputRebindConfig::default();
        let json = config.to_json().unwrap();
        let loaded = InputRebindConfig::from_json(&json).unwrap();

        assert_eq!(loaded.bindings.len(), config.bindings.len());
        assert!(loaded.bindings.contains_key("jump"));
    }

    #[test]
    fn binding_name_formatting() {
        assert_eq!(
            InputRebindConfig::binding_name(&RebindBinding::from_key(KeyCode::KeyA)),
            "A"
        );
        assert_eq!(
            InputRebindConfig::binding_name(&RebindBinding::from_mouse(MouseButton::Left)),
            "Left"
        );
    }

    #[test]
    fn timeout_expiry() {
        let mut config = InputRebindConfig::new();
        config.clear_all();
        config.rebind_timeout = 0.1;

        config.start_rebind("test", None);
        assert!(config.is_rebinding());

        config.update(0.2);
        assert!(!config.is_rebinding());
        assert_eq!(config.rebind_state, RebindState::Cancelled);
    }

    #[test]
    fn to_action_map() {
        let config = InputRebindConfig::default();
        let action_map = config.to_action_map();

        let mut input = Input::new();
        input.key_pressed(KeyCode::Space);

        assert!(action_map.is_pressed("jump", &input));
    }
}
