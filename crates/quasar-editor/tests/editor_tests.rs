//! Editor system unit tests

#[test]
fn editor_state_default() {
    use quasar_editor::EditorState;
    use quasar_editor::EditorMode;

    let state = EditorState::default();
    assert_eq!(state.mode, EditorMode::Stopped);
}

#[test]
fn editor_state_mode_transitions() {
    use quasar_editor::EditorState;
    use quasar_editor::EditorMode;

    let mut state = EditorState::default();
    assert_eq!(state.mode, EditorMode::Stopped);

    state.mode = EditorMode::Playing;
    assert_eq!(state.mode, EditorMode::Playing);

    state.pause();
    assert_eq!(state.mode, EditorMode::Paused);
}

#[test]
fn gizmo_mode_variants() {
    use quasar_editor::GizmoMode;

    assert!(matches!(GizmoMode::Translate, GizmoMode::Translate));
    assert!(matches!(GizmoMode::Rotate, GizmoMode::Rotate));
    assert!(matches!(GizmoMode::Scale, GizmoMode::Scale));
}

#[test]
fn console_creation() {
    use quasar_editor::ConsoleLog;

    let console = ConsoleLog::new();
    assert!(console.is_empty());
}

#[test]
fn console_log() {
    use quasar_editor::ConsoleLog;

    let mut console = ConsoleLog::new();
    console.log("Test message");

    assert!(!console.is_empty());
    assert_eq!(console.len(), 1);
}

#[test]
fn undo_stack_creation() {
    use quasar_editor::UndoStack;

    let stack = UndoStack::new();
    assert!(stack.can_undo() == false);
    assert!(stack.can_redo() == false);
}
