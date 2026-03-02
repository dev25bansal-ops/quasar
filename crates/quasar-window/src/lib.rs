//! # Quasar Window
//!
//! Window creation and input handling via [`winit`].

pub mod action_map;
pub mod input;
pub mod window;

pub use action_map::{ActionMap, InputBinding};
pub use input::{Input, KeyState, MouseButton};
pub use window::{QuasarWindow, WindowConfig};
