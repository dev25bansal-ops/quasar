//! Window and input handling tests

#[test]
fn action_map_creation() {
    use quasar_window::ActionMap;

    let map = ActionMap::new();
    assert!(map.is_empty());
}

#[test]
fn action_map_bind_key() {
    use quasar_window::{ActionMap, KeyCode};

    let map = ActionMap::new().bind("jump", KeyCode::Space);

    assert!(!map.is_empty());
}

#[test]
fn action_map_bind_mouse() {
    use quasar_window::{ActionMap, MouseButton};

    let map = ActionMap::new().bind("shoot", MouseButton::Left);

    assert!(!map.is_empty());
}

#[test]
fn action_map_multiple_bindings() {
    use quasar_window::{ActionMap, KeyCode};

    let map = ActionMap::new()
        .bind("jump", KeyCode::Space)
        .bind("jump", KeyCode::KeyW);

    assert!(!map.is_empty());
}

#[test]
fn input_state_default() {
    use quasar_window::InputState;

    let state = InputState::default();
    assert!(!state.key_pressed(KeyCode::Space));
}

#[test]
fn key_code_variants() {
    use quasar_window::KeyCode;

    assert!(matches!(KeyCode::Space, KeyCode::Space));
    assert!(matches!(KeyCode::KeyA, KeyCode::KeyA));
    assert!(matches!(KeyCode::Escape, KeyCode::Escape));
}

#[test]
fn mouse_button_variants() {
    use quasar_window::MouseButton;

    assert!(matches!(MouseButton::Left, MouseButton::Left));
    assert!(matches!(MouseButton::Right, MouseButton::Right));
    assert!(matches!(MouseButton::Middle, MouseButton::Middle));
}
