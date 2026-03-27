//! UI system unit tests

#[test]
fn ui_tree_creation() {
    use quasar_ui::UiTree;

    let tree = UiTree::new();
    assert!(tree.is_empty());
}

#[test]
fn ui_node_creation() {
    use quasar_ui::UiNode;

    let node = UiNode::new();
    assert_eq!(node.x, 0.0);
    assert_eq!(node.y, 0.0);
}

#[test]
fn ui_node_position() {
    use quasar_ui::UiNode;

    let node = UiNode::new().with_position(100.0, 200.0);

    assert_eq!(node.x, 100.0);
    assert_eq!(node.y, 200.0);
}

#[test]
fn ui_node_size() {
    use quasar_ui::UiNode;

    let node = UiNode::new().with_size(50.0, 30.0);

    assert_eq!(node.width, 50.0);
    assert_eq!(node.height, 30.0);
}

#[test]
fn button_creation() {
    use quasar_ui::Button;

    let button = Button::new("Click Me");
    assert_eq!(button.text, "Click Me");
}

#[test]
fn button_default() {
    use quasar_ui::Button;

    let button = Button::default();
    assert!(button.text.is_empty());
}

#[test]
fn checkbox_creation() {
    use quasar_ui::Checkbox;

    let checkbox = Checkbox::new("Option");
    assert_eq!(checkbox.label, "Option");
    assert!(!checkbox.checked);
}

#[test]
fn checkbox_checked() {
    use quasar_ui::Checkbox;

    let checkbox = Checkbox::new("Option").checked(true);
    assert!(checkbox.checked);
}

#[test]
fn slider_creation() {
    use quasar_ui::Slider;

    let slider = Slider::new(0.0, 100.0);
    assert_eq!(slider.min, 0.0);
    assert_eq!(slider.max, 100.0);
    assert_eq!(slider.value, 0.0);
}

#[test]
fn slider_value() {
    use quasar_ui::Slider;

    let slider = Slider::new(0.0, 100.0).with_value(50.0);
    assert_eq!(slider.value, 50.0);
}

#[test]
fn progress_bar_creation() {
    use quasar_ui::ProgressBar;

    let progress = ProgressBar::new(0.5);
    assert!((progress.value - 0.5).abs() < 0.001);
}

#[test]
fn text_input_creation() {
    use quasar_ui::TextInput;

    let input = TextInput::new("placeholder");
    assert_eq!(input.placeholder, "placeholder");
}
