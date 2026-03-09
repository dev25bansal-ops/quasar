//! # Quasar UI
//!
//! Retained-mode in-game UI system for the Quasar Engine.
//!
//! Provides a widget tree, flexbox-inspired layout engine, text rendering
//! via fontdue, and an ECS plugin that updates/renders the UI each frame.

#![deny(clippy::unwrap_used, clippy::expect_used)]

pub mod layout;
pub mod plugin;
pub mod renderer;
pub mod style;
pub mod widget;
pub mod widgets;

pub use layout::{LayoutRect, LayoutSolver};
pub use plugin::UiPlugin;
pub use renderer::{UiRenderPass, UiVertex};
pub use style::{Anchor, Color, FlexDirection, UiStyle};
pub use widget::{UiNode, UiTree, WidgetId};
pub use widgets::{Button, Checkbox, Panel, ProgressBar, Slider, TextInput};
