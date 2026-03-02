//! Input action mapping — bind named actions to physical keys / mouse buttons.
//!
//! ```rust,ignore
//! use quasar_window::{ActionMap, InputBinding};
//! use winit::keyboard::KeyCode;
//!
//! let mut actions = ActionMap::new();
//! actions.bind("jump", InputBinding::Key(KeyCode::Space));
//! actions.bind("fire", InputBinding::Mouse(MouseButton::Left));
//!
//! // In the game loop:
//! if actions.just_pressed("jump", &input) { /* ... */ }
//! ```

use std::collections::HashMap;

use winit::keyboard::KeyCode;

use crate::input::{Input, MouseButton};

/// A physical input that can be bound to an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InputBinding {
    /// A keyboard key.
    Key(KeyCode),
    /// A mouse button.
    Mouse(MouseButton),
}

/// Maps human-readable action names to one or more [`InputBinding`]s.
///
/// Each action can have several bindings — any one of them activating counts
/// as the action being active.
#[derive(Debug, Clone)]
pub struct ActionMap {
    bindings: HashMap<String, Vec<InputBinding>>,
}

impl ActionMap {
    /// Create an empty action map.
    pub fn new() -> Self {
        Self {
            bindings: HashMap::new(),
        }
    }

    /// Bind an action name to a physical input.
    ///
    /// Multiple bindings per action are allowed (e.g. WASD *and* arrow keys
    /// for the same "move_forward" action).
    pub fn bind(&mut self, action: impl Into<String>, binding: InputBinding) -> &mut Self {
        self.bindings
            .entry(action.into())
            .or_default()
            .push(binding);
        self
    }

    /// Convenience — bind a key to an action.
    pub fn bind_key(&mut self, action: impl Into<String>, key: KeyCode) -> &mut Self {
        self.bind(action, InputBinding::Key(key))
    }

    /// Convenience — bind a mouse button to an action.
    pub fn bind_mouse(
        &mut self,
        action: impl Into<String>,
        button: MouseButton,
    ) -> &mut Self {
        self.bind(action, InputBinding::Mouse(button))
    }

    /// Remove all bindings for an action.
    pub fn unbind(&mut self, action: &str) {
        self.bindings.remove(action);
    }

    /// Remove all bindings.
    pub fn clear(&mut self) {
        self.bindings.clear();
    }

    // ------------------------------------------------------------------
    // Queries
    // ------------------------------------------------------------------

    /// Is the action currently held down?
    pub fn is_pressed(&self, action: &str, input: &Input) -> bool {
        self.bindings
            .get(action)
            .is_some_and(|bs| bs.iter().any(|b| binding_pressed(b, input)))
    }

    /// Was the action activated this frame?
    pub fn just_pressed(&self, action: &str, input: &Input) -> bool {
        self.bindings
            .get(action)
            .is_some_and(|bs| bs.iter().any(|b| binding_just_pressed(b, input)))
    }

    /// Was the action released this frame?
    pub fn just_released(&self, action: &str, input: &Input) -> bool {
        self.bindings.get(action).is_some_and(|bs| {
            bs.iter().any(|b| binding_just_released(b, input))
        })
    }

    /// Get the raw axis value for a 1-D action defined by two bindings
    /// (negative and positive). Returns a value in \[-1.0, 1.0\].
    ///
    /// Example: `axis("strafe", "move_left", "move_right", &input)`
    pub fn axis(
        &self,
        negative_action: &str,
        positive_action: &str,
        input: &Input,
    ) -> f32 {
        let neg = if self.is_pressed(negative_action, input) {
            -1.0
        } else {
            0.0
        };
        let pos = if self.is_pressed(positive_action, input) {
            1.0
        } else {
            0.0
        };
        neg + pos
    }
}

impl Default for ActionMap {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn binding_pressed(b: &InputBinding, input: &Input) -> bool {
    match b {
        InputBinding::Key(k) => input.is_pressed(*k),
        InputBinding::Mouse(m) => input.is_mouse_pressed(*m),
    }
}

fn binding_just_pressed(b: &InputBinding, input: &Input) -> bool {
    match b {
        InputBinding::Key(k) => input.just_pressed(*k),
        InputBinding::Mouse(m) => input.mouse_just_pressed(*m),
    }
}

fn binding_just_released(b: &InputBinding, input: &Input) -> bool {
    match b {
        InputBinding::Key(k) => input.just_released(*k),
        InputBinding::Mouse(m) => input.mouse_just_released(*m),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bind_and_query() {
        let mut map = ActionMap::new();
        map.bind_key("jump", KeyCode::Space);

        let mut input = Input::new();
        assert!(!map.is_pressed("jump", &input));

        input.key_pressed(KeyCode::Space);
        assert!(map.is_pressed("jump", &input));
        assert!(map.just_pressed("jump", &input));
    }

    #[test]
    fn multiple_bindings() {
        let mut map = ActionMap::new();
        map.bind_key("forward", KeyCode::KeyW);
        map.bind_key("forward", KeyCode::ArrowUp);

        let mut input = Input::new();
        input.key_pressed(KeyCode::ArrowUp);
        assert!(map.is_pressed("forward", &input));
    }

    #[test]
    fn axis_value() {
        let mut map = ActionMap::new();
        map.bind_key("left", KeyCode::KeyA);
        map.bind_key("right", KeyCode::KeyD);

        let mut input = Input::new();
        input.key_pressed(KeyCode::KeyA);
        assert_eq!(map.axis("left", "right", &input), -1.0);

        let mut input2 = Input::new();
        input2.key_pressed(KeyCode::KeyD);
        assert_eq!(map.axis("left", "right", &input2), 1.0);

        // Both pressed → cancels out.
        let mut input3 = Input::new();
        input3.key_pressed(KeyCode::KeyA);
        input3.key_pressed(KeyCode::KeyD);
        assert_eq!(map.axis("left", "right", &input3), 0.0);
    }

    #[test]
    fn unbind_and_clear() {
        let mut map = ActionMap::new();
        map.bind_key("jump", KeyCode::Space);
        map.unbind("jump");

        let mut input = Input::new();
        input.key_pressed(KeyCode::Space);
        assert!(!map.is_pressed("jump", &input));

        map.bind_key("jump", KeyCode::Space);
        map.clear();
        assert!(!map.is_pressed("jump", &input));
    }
}
