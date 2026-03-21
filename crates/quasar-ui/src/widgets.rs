//! High-level convenience widgets built on top of [`UiTree`].
//!
//! Each widget is a helper that creates one or more [`UiNode`]s with
//! appropriate styles and interaction handling.

use crate::style::{Color, FlexDirection, SizeDimension, UiStyle};
use crate::widget::{UiContent, UiInteraction, UiTree, WidgetId};

// ─── Color palette ──────────────────────────────────────────────────────────

const BUTTON_BG: Color = Color {
    r: 0.25,
    g: 0.25,
    b: 0.30,
    a: 1.0,
};
const BUTTON_HOVER: Color = Color {
    r: 0.35,
    g: 0.35,
    b: 0.42,
    a: 1.0,
};
const BUTTON_PRESSED: Color = Color {
    r: 0.18,
    g: 0.18,
    b: 0.22,
    a: 1.0,
};
const SLIDER_TRACK: Color = Color {
    r: 0.2,
    g: 0.2,
    b: 0.2,
    a: 1.0,
};
const SLIDER_FILL: Color = Color {
    r: 0.3,
    g: 0.55,
    b: 0.95,
    a: 1.0,
};
const CHECKBOX_BG: Color = Color {
    r: 0.2,
    g: 0.2,
    b: 0.2,
    a: 1.0,
};
const CHECKBOX_CHECK: Color = Color {
    r: 0.3,
    g: 0.7,
    b: 0.3,
    a: 1.0,
};
const INPUT_BG: Color = Color {
    r: 0.12,
    g: 0.12,
    b: 0.14,
    a: 1.0,
};
const PROGRESS_BG: Color = Color {
    r: 0.15,
    g: 0.15,
    b: 0.15,
    a: 1.0,
};
const PROGRESS_FILL: Color = Color {
    r: 0.2,
    g: 0.6,
    b: 0.9,
    a: 1.0,
};

// ─── Button ─────────────────────────────────────────────────────────────────

/// Descriptor for creating a button widget.
pub struct ButtonDesc {
    pub label: String,
    pub width: SizeDimension,
    pub height: SizeDimension,
    pub font_size: f32,
}

impl Default for ButtonDesc {
    fn default() -> Self {
        Self {
            label: String::from("Button"),
            width: SizeDimension::Px(120.0),
            height: SizeDimension::Px(36.0),
            font_size: 16.0,
        }
    }
}

/// A clickable button with text label, hover and pressed states.
pub struct Button {
    /// Root node of the button.
    pub root: WidgetId,
    /// Text label node.
    pub label: WidgetId,
}

impl Button {
    /// Create a button under `parent` inside the given tree.
    pub fn spawn(tree: &mut UiTree, parent: WidgetId, desc: ButtonDesc) -> Self {
        let root = tree.add_child(
            parent,
            UiStyle {
                width: desc.width,
                height: desc.height,
                padding: [6.0, 16.0, 6.0, 16.0],
                background_color: BUTTON_BG,
                border_radius: 4.0,
                flex_direction: FlexDirection::Row,
                ..Default::default()
            },
        );
        let label = tree.add_text(
            root,
            &desc.label,
            UiStyle {
                text_color: Color::WHITE,
                font_size: desc.font_size,
                ..Default::default()
            },
        );
        Self { root, label }
    }

    /// Returns `true` when the button was clicked this frame.
    pub fn clicked(&self, tree: &UiTree) -> bool {
        tree.get(self.root)
            .map(|n| n.interaction.clicked)
            .unwrap_or(false)
    }

    /// Update visual feedback based on interaction state.
    pub fn update_visuals(&self, tree: &mut UiTree) {
        if let Some(node) = tree.get_mut(self.root) {
            node.style.background_color = match node.interaction {
                UiInteraction { pressed: true, .. } => BUTTON_PRESSED,
                UiInteraction { hovered: true, .. } => BUTTON_HOVER,
                _ => BUTTON_BG,
            };
        }
    }
}

// ─── Slider ─────────────────────────────────────────────────────────────────

/// Descriptor for creating a slider widget.
pub struct SliderDesc {
    pub min: f32,
    pub max: f32,
    pub value: f32,
    pub width: f32,
    pub height: f32,
}

impl Default for SliderDesc {
    fn default() -> Self {
        Self {
            min: 0.0,
            max: 1.0,
            value: 0.5,
            width: 200.0,
            height: 20.0,
        }
    }
}

/// A horizontal slider (track + fill bar).
pub struct Slider {
    /// Background track node.
    pub track: WidgetId,
    /// Filled portion of the track.
    pub fill: WidgetId,
    /// Current normalized value (0..1).
    pub value: f32,
    pub min: f32,
    pub max: f32,
    total_width: f32,
}

impl Slider {
    pub fn spawn(tree: &mut UiTree, parent: WidgetId, desc: SliderDesc) -> Self {
        let track = tree.add_child(
            parent,
            UiStyle {
                width: SizeDimension::Px(desc.width),
                height: SizeDimension::Px(desc.height),
                background_color: SLIDER_TRACK,
                border_radius: desc.height / 2.0,
                ..Default::default()
            },
        );

        let norm = ((desc.value - desc.min) / (desc.max - desc.min)).clamp(0.0, 1.0);
        let fill = tree.add_child(
            track,
            UiStyle {
                width: SizeDimension::Px(desc.width * norm),
                height: SizeDimension::Px(desc.height),
                background_color: SLIDER_FILL,
                border_radius: desc.height / 2.0,
                ..Default::default()
            },
        );

        Self {
            track,
            fill,
            value: norm,
            min: desc.min,
            max: desc.max,
            total_width: desc.width,
        }
    }

    /// Set the slider value (clamped to min..max) and update fill width.
    pub fn set_value(&mut self, tree: &mut UiTree, v: f32) {
        self.value = ((v - self.min) / (self.max - self.min)).clamp(0.0, 1.0);
        if let Some(fill_node) = tree.get_mut(self.fill) {
            fill_node.style.width = SizeDimension::Px(self.total_width * self.value);
        }
    }

    /// Get the current value in the original range.
    pub fn get_value(&self) -> f32 {
        self.min + self.value * (self.max - self.min)
    }
}

// ─── Checkbox ───────────────────────────────────────────────────────────────

/// Descriptor for creating a checkbox widget.
pub struct CheckboxDesc {
    pub label: String,
    pub checked: bool,
    pub size: f32,
}

impl Default for CheckboxDesc {
    fn default() -> Self {
        Self {
            label: String::from("Checkbox"),
            checked: false,
            size: 20.0,
        }
    }
}

/// A toggle checkbox (box + optional label).
pub struct Checkbox {
    pub root: WidgetId,
    pub box_node: WidgetId,
    pub label: WidgetId,
    pub checked: bool,
}

impl Checkbox {
    pub fn spawn(tree: &mut UiTree, parent: WidgetId, desc: CheckboxDesc) -> Self {
        let root = tree.add_child(
            parent,
            UiStyle {
                flex_direction: FlexDirection::Row,
                gap: 8.0,
                ..Default::default()
            },
        );
        let box_node = tree.add_child(
            root,
            UiStyle {
                width: SizeDimension::Px(desc.size),
                height: SizeDimension::Px(desc.size),
                background_color: if desc.checked {
                    CHECKBOX_CHECK
                } else {
                    CHECKBOX_BG
                },
                border_radius: 3.0,
                border_width: 1.0,
                border_color: Color::rgba(0.5, 0.5, 0.5, 1.0),
                ..Default::default()
            },
        );
        let label = tree.add_text(
            root,
            &desc.label,
            UiStyle {
                text_color: Color::WHITE,
                font_size: 14.0,
                ..Default::default()
            },
        );
        Self {
            root,
            box_node,
            label,
            checked: desc.checked,
        }
    }

    /// Toggle the checkbox if clicked.  Returns `true` when the state changed.
    pub fn update(&mut self, tree: &mut UiTree) -> bool {
        let clicked = tree
            .get(self.root)
            .map(|n| n.interaction.clicked)
            .unwrap_or(false);

        if clicked {
            self.checked = !self.checked;
            if let Some(node) = tree.get_mut(self.box_node) {
                node.style.background_color = if self.checked {
                    CHECKBOX_CHECK
                } else {
                    CHECKBOX_BG
                };
            }
        }
        clicked
    }
}

// ─── Text Input ─────────────────────────────────────────────────────────────

/// Descriptor for creating a text input widget.
pub struct TextInputDesc {
    pub placeholder: String,
    pub width: f32,
    pub height: f32,
}

impl Default for TextInputDesc {
    fn default() -> Self {
        Self {
            placeholder: String::from("Type here…"),
            width: 200.0,
            height: 30.0,
        }
    }
}

/// A single-line text input field.
pub struct TextInput {
    pub root: WidgetId,
    pub text_node: WidgetId,
    pub text: String,
    pub placeholder: String,
    pub focused: bool,
}

impl TextInput {
    pub fn spawn(tree: &mut UiTree, parent: WidgetId, desc: TextInputDesc) -> Self {
        let root = tree.add_child(
            parent,
            UiStyle {
                width: SizeDimension::Px(desc.width),
                height: SizeDimension::Px(desc.height),
                background_color: INPUT_BG,
                border_radius: 3.0,
                border_width: 1.0,
                border_color: Color::rgba(0.4, 0.4, 0.4, 1.0),
                padding: [4.0, 8.0, 4.0, 8.0],
                ..Default::default()
            },
        );
        let text_node = tree.add_text(
            root,
            &desc.placeholder,
            UiStyle {
                text_color: Color::rgba(0.5, 0.5, 0.5, 1.0),
                font_size: 14.0,
                ..Default::default()
            },
        );
        Self {
            root,
            text_node,
            text: String::new(),
            placeholder: desc.placeholder,
            focused: false,
        }
    }

    /// Call each frame to handle focus toggling on click.
    pub fn update_focus(&mut self, tree: &mut UiTree) {
        let clicked = tree
            .get(self.root)
            .map(|n| n.interaction.clicked)
            .unwrap_or(false);
        if clicked {
            self.focused = !self.focused;
        }
        if let Some(node) = tree.get_mut(self.root) {
            node.style.border_color = if self.focused {
                Color::rgba(0.4, 0.6, 1.0, 1.0)
            } else {
                Color::rgba(0.4, 0.4, 0.4, 1.0)
            };
        }
    }

    /// Append a character (call from input handling).
    pub fn push_char(&mut self, tree: &mut UiTree, ch: char) {
        if !self.focused {
            return;
        }
        self.text.push(ch);
        self.refresh_display(tree);
    }

    /// Remove the last character.
    pub fn pop_char(&mut self, tree: &mut UiTree) {
        if !self.focused {
            return;
        }
        self.text.pop();
        self.refresh_display(tree);
    }

    fn refresh_display(&self, tree: &mut UiTree) {
        if let Some(node) = tree.get_mut(self.text_node) {
            if self.text.is_empty() {
                node.content = UiContent::Text(self.placeholder.clone());
                node.style.text_color = Color::rgba(0.5, 0.5, 0.5, 1.0);
            } else {
                node.content = UiContent::Text(self.text.clone());
                node.style.text_color = Color::WHITE;
            }
        }
    }
}

// ─── Progress Bar ───────────────────────────────────────────────────────────

/// Descriptor for creating a progress bar widget.
pub struct ProgressBarDesc {
    pub progress: f32,
    pub width: f32,
    pub height: f32,
}

impl Default for ProgressBarDesc {
    fn default() -> Self {
        Self {
            progress: 0.0,
            width: 200.0,
            height: 16.0,
        }
    }
}

/// A visual progress indicator (0.0–1.0).
pub struct ProgressBar {
    pub track: WidgetId,
    pub fill: WidgetId,
    pub progress: f32,
    total_width: f32,
}

impl ProgressBar {
    pub fn spawn(tree: &mut UiTree, parent: WidgetId, desc: ProgressBarDesc) -> Self {
        let progress = desc.progress.clamp(0.0, 1.0);
        let track = tree.add_child(
            parent,
            UiStyle {
                width: SizeDimension::Px(desc.width),
                height: SizeDimension::Px(desc.height),
                background_color: PROGRESS_BG,
                border_radius: desc.height / 2.0,
                ..Default::default()
            },
        );
        let fill = tree.add_child(
            track,
            UiStyle {
                width: SizeDimension::Px(desc.width * progress),
                height: SizeDimension::Px(desc.height),
                background_color: PROGRESS_FILL,
                border_radius: desc.height / 2.0,
                ..Default::default()
            },
        );
        Self {
            track,
            fill,
            progress,
            total_width: desc.width,
        }
    }

    /// Update the progress value (clamped 0..1).
    pub fn set_progress(&mut self, tree: &mut UiTree, v: f32) {
        self.progress = v.clamp(0.0, 1.0);
        if let Some(fill_node) = tree.get_mut(self.fill) {
            fill_node.style.width = SizeDimension::Px(self.total_width * self.progress);
        }
    }
}

// ─── Panel ──────────────────────────────────────────────────────────────────

/// A styled container panel (background + padding + optional title).
pub struct Panel {
    pub root: WidgetId,
    pub title: Option<WidgetId>,
}

impl Panel {
    /// Create a panel under `parent`.  If `title` is `Some`, a text header is added.
    pub fn spawn(tree: &mut UiTree, parent: WidgetId, title: Option<&str>, style: UiStyle) -> Self {
        let root = tree.add_child(parent, style);
        let title_id = title.map(|t| {
            tree.add_text(
                root,
                t,
                UiStyle {
                    text_color: Color::WHITE,
                    font_size: 18.0,
                    padding: [0.0, 0.0, 8.0, 0.0],
                    ..Default::default()
                },
            )
        });
        Self {
            root,
            title: title_id,
        }
    }
}
