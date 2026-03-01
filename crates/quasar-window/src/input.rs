//! Input state tracking — keyboard and mouse.

use std::collections::HashSet;
use winit::keyboard::KeyCode;

/// The state of a key: pressed this frame, held, or released.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyState {
    /// Key went down this frame.
    JustPressed,
    /// Key is being held.
    Held,
    /// Key went up this frame.
    JustReleased,
}

/// Tracks keyboard and mouse input state.
///
/// Call [`Input::begin_frame`] at the start of each frame to clear per-frame
/// state, then feed events from winit.
pub struct Input {
    /// Keys currently held down.
    pressed: HashSet<KeyCode>,
    /// Keys pressed this frame (for "just pressed" detection).
    just_pressed: HashSet<KeyCode>,
    /// Keys released this frame.
    just_released: HashSet<KeyCode>,
    /// Current cursor position in physical pixels.
    pub cursor_position: Option<(f64, f64)>,
    /// Mouse delta since last frame (for FPS-style look).
    pub mouse_delta: (f64, f64),
}

impl Input {
    pub fn new() -> Self {
        Self {
            pressed: HashSet::new(),
            just_pressed: HashSet::new(),
            just_released: HashSet::new(),
            cursor_position: None,
            mouse_delta: (0.0, 0.0),
        }
    }

    /// Clear per-frame state. Call once at the start of each frame.
    pub fn begin_frame(&mut self) {
        self.just_pressed.clear();
        self.just_released.clear();
        self.mouse_delta = (0.0, 0.0);
    }

    /// Record a key press event from winit.
    pub fn key_pressed(&mut self, key: KeyCode) {
        if self.pressed.insert(key) {
            self.just_pressed.insert(key);
        }
    }

    /// Record a key release event from winit.
    pub fn key_released(&mut self, key: KeyCode) {
        self.pressed.remove(&key);
        self.just_released.insert(key);
    }

    /// Record a mouse movement delta.
    pub fn mouse_moved(&mut self, dx: f64, dy: f64) {
        self.mouse_delta.0 += dx;
        self.mouse_delta.1 += dy;
    }

    /// Is this key currently held down?
    pub fn is_pressed(&self, key: KeyCode) -> bool {
        self.pressed.contains(&key)
    }

    /// Was this key pressed this frame?
    pub fn just_pressed(&self, key: KeyCode) -> bool {
        self.just_pressed.contains(&key)
    }

    /// Was this key released this frame?
    pub fn just_released(&self, key: KeyCode) -> bool {
        self.just_released.contains(&key)
    }

    /// Get the input state for a key.
    pub fn key_state(&self, key: KeyCode) -> Option<KeyState> {
        if self.just_pressed.contains(&key) {
            Some(KeyState::JustPressed)
        } else if self.pressed.contains(&key) {
            Some(KeyState::Held)
        } else if self.just_released.contains(&key) {
            Some(KeyState::JustReleased)
        } else {
            None
        }
    }
}

impl Default for Input {
    fn default() -> Self {
        Self::new()
    }
}
