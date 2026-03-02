//! Centralized error types for the Quasar Engine.

use std::fmt;

/// Top-level engine error.
#[derive(Debug)]
pub enum QuasarError {
    /// GPU / rendering subsystem failure.
    Render(String),
    /// Window creation or event-loop failure.
    Window(String),
    /// Scripting engine failure.
    Script(String),
    /// Asset loading failure.
    Asset(String),
    /// Generic / catch-all.
    Other(String),
}

impl fmt::Display for QuasarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Render(msg) => write!(f, "Render error: {msg}"),
            Self::Window(msg) => write!(f, "Window error: {msg}"),
            Self::Script(msg) => write!(f, "Script error: {msg}"),
            Self::Asset(msg) => write!(f, "Asset error: {msg}"),
            Self::Other(msg) => write!(f, "Error: {msg}"),
        }
    }
}

impl std::error::Error for QuasarError {}

/// Convenience alias used throughout the engine.
pub type QuasarResult<T> = Result<T, QuasarError>;
