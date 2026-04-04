//! Performance monitoring and profiling tools for Quasar Engine.
//!
//! # Features
//!
//! - `cpu` - CPU profiling with frame timing and scoped timing (enabled by default)
//! - `gpu` - GPU profiling with wgpu timestamp queries
//! - `ui` - Real-time profiler UI with egui

mod stats;

#[cfg(feature = "cpu")]
pub mod cpu;

#[cfg(feature = "cpu")]
pub mod memory;

#[cfg(feature = "gpu")]
pub mod gpu;

pub mod export;

pub use stats::*;

#[cfg(feature = "cpu")]
pub use cpu::*;

#[cfg(feature = "cpu")]
pub use memory::*;

#[cfg(feature = "gpu")]
pub use gpu::*;
