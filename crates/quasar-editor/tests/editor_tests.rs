//! Editor system unit tests

#[test]
fn editor_state_default() {
    use quasar_editor::EditorState;

    let state = EditorState::default();
    assert!(!state.visible);
}

#[test]
fn editor_state_toggle() {
    use quasar_editor::EditorState;

    let mut state = EditorState::default();
    state.toggle();
    assert!(state.visible);
}

#[test]
fn selection_default() {
    use quasar_editor::Selection;

    let selection = Selection::default();
    assert!(selection.is_empty());
}

#[test]
fn selection_add() {
    use quasar_core::Entity;
    use quasar_editor::Selection;

    let mut selection = Selection::default();
    let entity = Entity::new(0, 0);
    selection.add(entity);

    assert!(!selection.is_empty());
    assert!(selection.contains(entity));
}

#[test]
fn selection_clear() {
    use quasar_core::Entity;
    use quasar_editor::Selection;

    let mut selection = Selection::default();
    selection.add(Entity::new(0, 0));
    selection.clear();

    assert!(selection.is_empty());
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
    use quasar_editor::Console;

    let console = Console::new();
    assert!(console.entries().is_empty());
}

#[test]
fn console_log() {
    use quasar_editor::Console;

    let mut console = Console::new();
    console.log("Test message");

    assert!(!console.entries().is_empty());
}

#[test]
fn undo_stack_creation() {
    use quasar_editor::UndoStack;

    let stack = UndoStack::new();
    assert!(stack.can_undo() == false);
    assert!(stack.can_redo() == false);
}
