//! # Quasar Window
//!
//! Window creation and input handling via [`winit`].

pub mod window;
pub mod input;

pub use window::QuasarWindow;
pub use input::{Input, KeyState};
