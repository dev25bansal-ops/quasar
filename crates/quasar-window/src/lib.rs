//! # Quasar Window
//!
//! Window creation and input handling via [`winit`].

#![deny(clippy::unwrap_used, clippy::expect_used)]

pub mod action_map;
pub mod action_plugin;
pub mod input;
pub mod window;

pub use action_map::{ActionMap, InputBinding};
pub use action_plugin::{ActionEvent, ActionMapPlugin, ActionMapSystem, ActionState};
pub use input::{Input, KeyState, MouseButton};
pub use window::{QuasarWindow, WindowConfig};
