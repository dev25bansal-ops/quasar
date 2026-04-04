//! Console input abstraction for gamepad-first design.
//!
//! Provides unified input handling for:
//! - Gamepads (Xbox, PlayStation, Switch, etc.)
//! - Keyboard + Mouse fallback
//! - Touch controls
//! - Platform-specific mappings

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Gamepad button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GamepadButton {
    // Face buttons (Xbox layout)
    A, // South / Cross
    B, // East / Circle
    X, // West / Square
    Y, // North / Triangle
    // D-pad
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
    // Shoulders
    LeftBumper,   // L1 / LB
    RightBumper,  // R1 / RB
    LeftTrigger,  // L2 / LT
    RightTrigger, // R2 / RT
    // Sticks
    LeftStick,
    RightStick,
    // System
    Start,  // Menu / Options
    Select, // View / Share / Back
    Home,   // Guide / Home / PS
}

/// Gamepad axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GamepadAxis {
    LeftStickX,
    LeftStickY,
    RightStickX,
    RightStickY,
    LeftTrigger,
    RightTrigger,
}

/// Console input action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConsoleAction {
    // Navigation
    MenuUp,
    MenuDown,
    MenuLeft,
    MenuRight,
    MenuConfirm,
    MenuBack,
    MenuCancel,
    // Gameplay
    MoveX,
    MoveY,
    CameraX,
    CameraY,
    Jump,
    Attack,
    Attack2,
    Interact,
    Dodge,
    Block,
    Sprint,
    Crouch,
    // Quick actions
    QuickItem1,
    QuickItem2,
    QuickItem3,
    QuickItem4,
    // UI
    Pause,
    Inventory,
    Map,
    Journal,
    Skills,
    // Camera
    CameraZoomIn,
    CameraZoomOut,
    CameraReset,
    // Custom
    Custom(u32),
}

/// Platform type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Platform {
    PC,
    Xbox,
    PlayStation,
    Switch,
    Mobile,
}

impl Platform {
    pub fn button_name(&self, button: GamepadButton) -> &'static str {
        match self {
            Self::PC | Self::Xbox => match button {
                GamepadButton::A => "A",
                GamepadButton::B => "B",
                GamepadButton::X => "X",
                GamepadButton::Y => "Y",
                GamepadButton::LeftBumper => "LB",
                GamepadButton::RightBumper => "RB",
                GamepadButton::LeftTrigger => "LT",
                GamepadButton::RightTrigger => "RT",
                GamepadButton::Start => "Menu",
                GamepadButton::Select => "View",
                GamepadButton::Home => "Guide",
                _ => "",
            },
            Self::PlayStation => match button {
                GamepadButton::A => "Cross",
                GamepadButton::B => "Circle",
                GamepadButton::X => "Square",
                GamepadButton::Y => "Triangle",
                GamepadButton::LeftBumper => "L1",
                GamepadButton::RightBumper => "R1",
                GamepadButton::LeftTrigger => "L2",
                GamepadButton::RightTrigger => "R2",
                GamepadButton::Start => "Options",
                GamepadButton::Select => "Share",
                GamepadButton::Home => "PS",
                _ => "",
            },
            Self::Switch => match button {
                GamepadButton::A => "B",
                GamepadButton::B => "A",
                GamepadButton::X => "Y",
                GamepadButton::Y => "X",
                GamepadButton::LeftBumper => "L",
                GamepadButton::RightBumper => "R",
                GamepadButton::LeftTrigger => "ZL",
                GamepadButton::RightTrigger => "ZR",
                GamepadButton::Start => "+",
                GamepadButton::Select => "-",
                GamepadButton::Home => "Home",
                _ => "",
            },
            Self::Mobile => "",
        }
    }

    pub fn action_prompt(&self, action: ConsoleAction) -> String {
        match action {
            ConsoleAction::MenuConfirm => self.button_name(GamepadButton::A).to_string(),
            ConsoleAction::MenuBack => self.button_name(GamepadButton::B).to_string(),
            ConsoleAction::MenuCancel => self.button_name(GamepadButton::B).to_string(),
            ConsoleAction::Jump => self.button_name(GamepadButton::A).to_string(),
            ConsoleAction::Attack => self.button_name(GamepadButton::X).to_string(),
            ConsoleAction::Interact => self.button_name(GamepadButton::Y).to_string(),
            ConsoleAction::Dodge => self.button_name(GamepadButton::B).to_string(),
            ConsoleAction::Block => self.button_name(GamepadButton::RightBumper).to_string(),
            ConsoleAction::Sprint => self.button_name(GamepadButton::LeftStick).to_string(),
            ConsoleAction::Pause => self.button_name(GamepadButton::Start).to_string(),
            ConsoleAction::Inventory => self.button_name(GamepadButton::Select).to_string(),
            _ => String::new(),
        }
    }
}

/// Input binding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputBinding {
    /// Action this binding maps to.
    pub action: ConsoleAction,
    /// Primary gamepad binding.
    pub gamepad_primary: Option<GamepadBinding>,
    /// Secondary gamepad binding.
    pub gamepad_secondary: Option<GamepadBinding>,
    /// Primary keyboard binding.
    pub keyboard_primary: Option<KeyboardBinding>,
    /// Secondary keyboard binding.
    pub keyboard_secondary: Option<KeyboardBinding>,
}

impl InputBinding {
    pub fn new(action: ConsoleAction) -> Self {
        Self {
            action,
            gamepad_primary: None,
            gamepad_secondary: None,
            keyboard_primary: None,
            keyboard_secondary: None,
        }
    }

    pub fn gamepad(mut self, button: GamepadButton) -> Self {
        self.gamepad_primary = Some(GamepadBinding::Button(button));
        self
    }

    pub fn gamepad_axis(mut self, axis: GamepadAxis, threshold: f32) -> Self {
        self.gamepad_primary = Some(GamepadBinding::Axis { axis, threshold });
        self
    }

    pub fn keyboard(mut self, key: KeyCode) -> Self {
        self.keyboard_primary = Some(KeyboardBinding::Key(key));
        self
    }

    pub fn keyboard_mouse(mut self, mouse: MouseButton) -> Self {
        self.keyboard_primary = Some(KeyboardBinding::Mouse(mouse));
        self
    }
}

/// Gamepad binding type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GamepadBinding {
    Button(GamepadButton),
    Axis { axis: GamepadAxis, threshold: f32 },
}

/// Keyboard binding type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum KeyboardBinding {
    Key(KeyCode),
    Mouse(MouseButton),
    ModifierKey {
        key: KeyCode,
        modifiers: Vec<KeyModifier>,
    },
}

/// Key code (simplified).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyCode {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Num0,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    Space,
    Enter,
    Escape,
    Tab,
    Backspace,
    Delete,
    Insert,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    PageUp,
    PageDown,
    Shift,
    Control,
    Alt,
    Grave,
    Minus,
    Equal,
    LeftBracket,
    RightBracket,
    Semicolon,
    Quote,
    Comma,
    Period,
    Slash,
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    NumpadAdd,
    NumpadSubtract,
    NumpadMultiply,
    NumpadDivide,
    NumpadEnter,
    NumpadDecimal,
}

/// Key modifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyModifier {
    Shift,
    Control,
    Alt,
    Super,
}

/// Mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    X1,
    X2,
}

/// Input state for a single frame.
#[derive(Debug, Clone, Default)]
pub struct InputState {
    /// Buttons pressed this frame.
    pub buttons_pressed: HashMap<GamepadButton, bool>,
    /// Buttons held.
    pub buttons_held: HashMap<GamepadButton, bool>,
    /// Buttons released this frame.
    pub buttons_released: HashMap<GamepadButton, bool>,
    /// Axis values (-1 to 1).
    pub axes: HashMap<GamepadAxis, f32>,
    /// Actions triggered.
    pub actions: HashMap<ConsoleAction, ActionState>,
}

impl InputState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_action_pressed(&self, action: ConsoleAction) -> bool {
        self.actions
            .get(&action)
            .map(|s| s.pressed)
            .unwrap_or(false)
    }

    pub fn is_action_held(&self, action: ConsoleAction) -> bool {
        self.actions.get(&action).map(|s| s.held).unwrap_or(false)
    }

    pub fn is_action_released(&self, action: ConsoleAction) -> bool {
        self.actions
            .get(&action)
            .map(|s| s.released)
            .unwrap_or(false)
    }

    pub fn get_action_value(&self, action: ConsoleAction) -> f32 {
        self.actions.get(&action).map(|s| s.value).unwrap_or(0.0)
    }

    pub fn get_axis(&self, axis: GamepadAxis) -> f32 {
        self.axes.get(&axis).copied().unwrap_or(0.0)
    }
}

/// Action state.
#[derive(Debug, Clone, Copy, Default)]
pub struct ActionState {
    pub pressed: bool,
    pub held: bool,
    pub released: bool,
    pub value: f32,
}

/// Console input system.
pub struct ConsoleInput {
    /// Platform type.
    pub platform: Platform,
    /// Action bindings.
    pub bindings: HashMap<ConsoleAction, InputBinding>,
    /// Current input state.
    pub state: InputState,
    /// Previous input state.
    pub prev_state: InputState,
    /// Vibration intensity.
    pub vibration: [f32; 2],
    /// Is gamepad connected.
    pub gamepad_connected: bool,
    /// Last used input type.
    pub last_input_type: InputType,
    /// Deadzone for sticks.
    pub stick_deadzone: f32,
    /// Trigger threshold.
    pub trigger_threshold: f32,
}

impl ConsoleInput {
    pub fn new(platform: Platform) -> Self {
        let mut bindings = HashMap::new();

        // Default bindings (Xbox-style)
        bindings.insert(
            ConsoleAction::MenuConfirm,
            InputBinding::new(ConsoleAction::MenuConfirm)
                .gamepad(GamepadButton::A)
                .keyboard(KeyCode::Enter),
        );
        bindings.insert(
            ConsoleAction::MenuBack,
            InputBinding::new(ConsoleAction::MenuBack)
                .gamepad(GamepadButton::B)
                .keyboard(KeyCode::Escape),
        );
        bindings.insert(
            ConsoleAction::Jump,
            InputBinding::new(ConsoleAction::Jump)
                .gamepad(GamepadButton::A)
                .keyboard(KeyCode::Space),
        );
        bindings.insert(
            ConsoleAction::Attack,
            InputBinding::new(ConsoleAction::Attack)
                .gamepad(GamepadButton::X)
                .keyboard_mouse(MouseButton::Left),
        );
        bindings.insert(
            ConsoleAction::Attack2,
            InputBinding::new(ConsoleAction::Attack2)
                .gamepad(GamepadButton::Y)
                .keyboard_mouse(MouseButton::Right),
        );
        bindings.insert(
            ConsoleAction::Interact,
            InputBinding::new(ConsoleAction::Interact)
                .gamepad(GamepadButton::Y)
                .keyboard(KeyCode::E),
        );
        bindings.insert(
            ConsoleAction::Dodge,
            InputBinding::new(ConsoleAction::Dodge)
                .gamepad(GamepadButton::B)
                .keyboard(KeyCode::Shift),
        );
        bindings.insert(
            ConsoleAction::Block,
            InputBinding::new(ConsoleAction::Block)
                .gamepad(GamepadButton::RightBumper)
                .keyboard_mouse(MouseButton::Right),
        );
        bindings.insert(
            ConsoleAction::Sprint,
            InputBinding::new(ConsoleAction::Sprint)
                .gamepad(GamepadButton::LeftStick)
                .keyboard(KeyCode::Shift),
        );
        bindings.insert(
            ConsoleAction::Pause,
            InputBinding::new(ConsoleAction::Pause)
                .gamepad(GamepadButton::Start)
                .keyboard(KeyCode::Escape),
        );
        bindings.insert(
            ConsoleAction::Inventory,
            InputBinding::new(ConsoleAction::Inventory)
                .gamepad(GamepadButton::Select)
                .keyboard(KeyCode::I),
        );
        bindings.insert(
            ConsoleAction::Map,
            InputBinding::new(ConsoleAction::Map)
                .gamepad(GamepadButton::Y)
                .keyboard(KeyCode::M),
        );

        Self {
            platform,
            bindings,
            state: InputState::new(),
            prev_state: InputState::new(),
            vibration: [0.0, 0.0],
            gamepad_connected: false,
            last_input_type: InputType::Keyboard,
            stick_deadzone: 0.15,
            trigger_threshold: 0.5,
        }
    }

    pub fn set_binding(&mut self, binding: InputBinding) {
        self.bindings.insert(binding.action, binding);
    }

    pub fn update(&mut self) {
        // Store previous state
        self.prev_state = self.state.clone();

        // Process bindings
        for (action, binding) in &self.bindings {
            let mut action_state = ActionState::default();

            // Check gamepad binding
            if self.gamepad_connected {
                if let Some(ref gamepad) = binding.gamepad_primary {
                    self.process_gamepad_binding(gamepad, &mut action_state);
                }
                if let Some(ref gamepad) = binding.gamepad_secondary {
                    self.process_gamepad_binding(gamepad, &mut action_state);
                }
            }

            // Check keyboard binding (always)
            if let Some(ref keyboard) = binding.keyboard_primary {
                self.process_keyboard_binding(keyboard, &mut action_state);
            }
            if let Some(ref keyboard) = binding.keyboard_secondary {
                self.process_keyboard_binding(keyboard, &mut action_state);
            }

            self.state.actions.insert(*action, action_state);
        }

        // Apply deadzones
        self.apply_deadzones();
    }

    fn process_gamepad_binding(&self, binding: &GamepadBinding, action_state: &mut ActionState) {
        match binding {
            GamepadBinding::Button(button) => {
                if self
                    .state
                    .buttons_held
                    .get(button)
                    .copied()
                    .unwrap_or(false)
                {
                    action_state.held = true;
                    action_state.value = 1.0;
                }
                if self
                    .state
                    .buttons_pressed
                    .get(button)
                    .copied()
                    .unwrap_or(false)
                {
                    action_state.pressed = true;
                }
                if self
                    .state
                    .buttons_released
                    .get(button)
                    .copied()
                    .unwrap_or(false)
                {
                    action_state.released = true;
                }
            }
            GamepadBinding::Axis { axis, threshold } => {
                let value = self.state.axes.get(axis).copied().unwrap_or(0.0);
                if value.abs() > threshold.abs() {
                    action_state.held = true;
                    action_state.value = value;
                }
            }
        }
    }

    fn process_keyboard_binding(&self, binding: &KeyboardBinding, action_state: &mut ActionState) {
        // Placeholder - would integrate with actual keyboard state
        let _ = (binding, action_state);
    }

    fn apply_deadzones(&mut self) {
        for axis in [
            GamepadAxis::LeftStickX,
            GamepadAxis::LeftStickY,
            GamepadAxis::RightStickX,
            GamepadAxis::RightStickY,
        ] {
            if let Some(value) = self.state.axes.get_mut(&axis) {
                if value.abs() < self.stick_deadzone {
                    *value = 0.0;
                } else {
                    *value = value.signum()
                        * ((value.abs() - self.stick_deadzone) / (1.0 - self.stick_deadzone));
                }
            }
        }
    }

    pub fn set_vibration(&mut self, strong: f32, weak: f32) {
        self.vibration = [strong.clamp(0.0, 1.0), weak.clamp(0.0, 1.0)];
    }

    pub fn stop_vibration(&mut self) {
        self.vibration = [0.0, 0.0];
    }

    pub fn is_action_pressed(&self, action: ConsoleAction) -> bool {
        self.state.is_action_pressed(action)
    }

    pub fn is_action_held(&self, action: ConsoleAction) -> bool {
        self.state.is_action_held(action)
    }

    pub fn is_action_released(&self, action: ConsoleAction) -> bool {
        self.state.is_action_released(action)
    }

    pub fn get_action_value(&self, action: ConsoleAction) -> f32 {
        self.state.get_action_value(action)
    }

    pub fn get_move_vector(&self) -> [f32; 2] {
        let x = self.state.get_axis(GamepadAxis::LeftStickX);
        let y = self.state.get_axis(GamepadAxis::LeftStickY);
        [x, y]
    }

    pub fn get_camera_vector(&self) -> [f32; 2] {
        let x = self.state.get_axis(GamepadAxis::RightStickX);
        let y = self.state.get_axis(GamepadAxis::RightStickY);
        [x, y]
    }

    pub fn get_action_prompt(&self, action: ConsoleAction) -> String {
        self.platform.action_prompt(action)
    }
}

/// Input type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputType {
    Keyboard,
    Gamepad,
    Touch,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_names() {
        let xbox = Platform::Xbox;
        assert_eq!(xbox.button_name(GamepadButton::A), "A");

        let ps = Platform::PlayStation;
        assert_eq!(ps.button_name(GamepadButton::A), "Cross");
        assert_eq!(ps.button_name(GamepadButton::Y), "Triangle");
    }

    #[test]
    fn input_binding() {
        let binding = InputBinding::new(ConsoleAction::Jump)
            .gamepad(GamepadButton::A)
            .keyboard(KeyCode::Space);

        assert_eq!(binding.action, ConsoleAction::Jump);
        assert!(binding.gamepad_primary.is_some());
        assert!(binding.keyboard_primary.is_some());
    }

    #[test]
    fn input_state() {
        let mut state = InputState::new();
        state.actions.insert(
            ConsoleAction::Jump,
            ActionState {
                pressed: true,
                held: false,
                released: false,
                value: 1.0,
            },
        );

        assert!(state.is_action_pressed(ConsoleAction::Jump));
        assert!(!state.is_action_held(ConsoleAction::Jump));
    }

    #[test]
    fn deadzone() {
        let mut input = ConsoleInput::new(Platform::Xbox);
        input.state.axes.insert(GamepadAxis::LeftStickX, 0.1);
        input.apply_deadzones();

        assert_eq!(input.state.get_axis(GamepadAxis::LeftStickX), 0.0);

        input.state.axes.insert(GamepadAxis::LeftStickX, 0.5);
        input.apply_deadzones();
        assert!(input.state.get_axis(GamepadAxis::LeftStickX).abs() > 0.0);
    }
}
