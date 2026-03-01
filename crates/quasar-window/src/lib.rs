//! # Quasar Window
//!
//! Window creation and input handling via [`winit`].

pub mod input;
pub mod window;

pub use input::{Input, KeyState, MouseButton};
pub use window::{QuasarWindow, WindowConfig};
