//! Centralized error types for the Quasar Engine.
//!
//! Provides rich error context including:
//! - Error codes for easy identification
//! - Source location (file, line, function)
//! - Operation context (what was being done)
//! - Component/asset identifiers
//! - Nested error chains
//!
//! # Example
//!
//! ```ignore
//! use quasar_core::error::*;
//!
//! // Simple error
//! return Err(QuasarError::asset("texture.png")
//!     .with_operation("loading")
//!     .with_reason("file not found"));
//!
//! // Error with context
//! let err = QuasarError::ecs("Entity 42")
//!     .with_operation("spawn")
//!     .with_reason("archetype limit exceeded")
//!     .with_code(ErrorCode::EcsArchetypeLimit);
//!
//! // From a result
//! let result = some_operation().map_err(|e|
//!     QuasarError::from_source(e)
//!         .with_operation("initialization")
//!         .with_context("renderer"));
//! ```

use std::fmt;
use std::sync::Arc;

/// Error codes for categorizing and identifying errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    // Render errors (1xxx)
    RenderInitialization = 1001,
    RenderDeviceLost = 1002,
    RenderOutOfMemory = 1003,
    RenderShaderCompilation = 1004,
    RenderPipelineCreation = 1005,

    // Window errors (2xxx)
    WindowCreation = 2001,
    WindowSwapChain = 2002,
    WindowSurface = 2003,

    // Asset errors (3xxx)
    AssetNotFound = 3001,
    AssetLoadFailed = 3002,
    AssetParseFailed = 3003,
    AssetInvalidFormat = 3004,
    AssetDependencyMissing = 3005,

    // ECS errors (4xxx)
    EcsEntityNotFound = 4001,
    EcsComponentMissing = 4002,
    EcsArchetypeLimit = 4003,
    EcsCommandBufferFull = 4004,

    // Script errors (5xxx)
    ScriptCompilation = 5001,
    ScriptRuntime = 5002,
    ScriptTimeout = 5003,
    ScriptPermissionDenied = 5004,

    // Network errors (6xxx)
    NetworkConnectionFailed = 6001,
    NetworkTimeout = 6002,
    NetworkProtocol = 6003,
    NetworkAuthFailed = 6004,

    // Physics errors (7xxx)
    PhysicsInitialization = 7001,
    PhysicsSimulation = 7002,

    // Audio errors (8xxx)
    AudioInitialization = 8001,
    AudioDeviceLost = 8002,

    // IO errors (9xxx)
    IoNotFound = 9001,
    IoPermissionDenied = 9002,
    IoInvalidData = 9003,

    // Generic
    Unknown = 9999,
}

impl ErrorCode {
    pub fn category(&self) -> &'static str {
        match self {
            Self::RenderInitialization
            | Self::RenderDeviceLost
            | Self::RenderOutOfMemory
            | Self::RenderShaderCompilation
            | Self::RenderPipelineCreation => "render",

            Self::WindowCreation | Self::WindowSwapChain | Self::WindowSurface => "window",

            Self::AssetNotFound
            | Self::AssetLoadFailed
            | Self::AssetParseFailed
            | Self::AssetInvalidFormat
            | Self::AssetDependencyMissing => "asset",

            Self::EcsEntityNotFound
            | Self::EcsComponentMissing
            | Self::EcsArchetypeLimit
            | Self::EcsCommandBufferFull => "ecs",

            Self::ScriptCompilation
            | Self::ScriptRuntime
            | Self::ScriptTimeout
            | Self::ScriptPermissionDenied => "script",

            Self::NetworkConnectionFailed
            | Self::NetworkTimeout
            | Self::NetworkProtocol
            | Self::NetworkAuthFailed => "network",

            Self::PhysicsInitialization | Self::PhysicsSimulation => "physics",

            Self::AudioInitialization | Self::AudioDeviceLost => "audio",

            Self::IoNotFound | Self::IoPermissionDenied | Self::IoInvalidData => "io",

            Self::Unknown => "unknown",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::RenderInitialization => "Failed to initialize renderer",
            Self::RenderDeviceLost => "GPU device was lost",
            Self::RenderOutOfMemory => "GPU out of memory",
            Self::RenderShaderCompilation => "Shader compilation failed",
            Self::RenderPipelineCreation => "Pipeline creation failed",

            Self::WindowCreation => "Failed to create window",
            Self::WindowSwapChain => "Swap chain error",
            Self::WindowSurface => "Surface error",

            Self::AssetNotFound => "Asset not found",
            Self::AssetLoadFailed => "Asset loading failed",
            Self::AssetParseFailed => "Asset parsing failed",
            Self::AssetInvalidFormat => "Invalid asset format",
            Self::AssetDependencyMissing => "Asset dependency missing",

            Self::EcsEntityNotFound => "Entity not found",
            Self::EcsComponentMissing => "Component missing",
            Self::EcsArchetypeLimit => "Archetype limit exceeded",
            Self::EcsCommandBufferFull => "Command buffer full",

            Self::ScriptCompilation => "Script compilation failed",
            Self::ScriptRuntime => "Script runtime error",
            Self::ScriptTimeout => "Script execution timeout",
            Self::ScriptPermissionDenied => "Script permission denied",

            Self::NetworkConnectionFailed => "Connection failed",
            Self::NetworkTimeout => "Network timeout",
            Self::NetworkProtocol => "Protocol error",
            Self::NetworkAuthFailed => "Authentication failed",

            Self::PhysicsInitialization => "Physics initialization failed",
            Self::PhysicsSimulation => "Physics simulation error",

            Self::AudioInitialization => "Audio initialization failed",
            Self::AudioDeviceLost => "Audio device lost",

            Self::IoNotFound => "File not found",
            Self::IoPermissionDenied => "Permission denied",
            Self::IoInvalidData => "Invalid data",

            Self::Unknown => "Unknown error",
        }
    }
}

/// Source location information.
#[derive(Debug, Clone)]
pub struct SourceLocation {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub function: String,
}

impl fmt::Display for SourceLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}:{} in {}",
            self.file, self.line, self.column, self.function
        )
    }
}

/// Rich error context.
#[derive(Debug, Clone)]
#[derive(Default)]
pub struct ErrorContext {
    /// The subsystem or component this error relates to.
    pub context: String,
    /// The operation being performed.
    pub operation: Option<String>,
    /// The specific item being operated on.
    pub target: Option<String>,
    /// Human-readable reason for the error.
    pub reason: Option<String>,
    /// Source location where the error occurred.
    pub location: Option<SourceLocation>,
    /// Stack backtrace (if captured).
    pub backtrace: Option<String>,
    /// Nested error chain.
    pub source: Option<Arc<QuasarError>>,
}


/// Top-level engine error with rich context.
#[derive(Debug, Clone)]
pub struct QuasarError {
    /// Error category and code.
    pub code: ErrorCode,
    /// Primary error message.
    pub message: String,
    /// Rich error context.
    pub context: ErrorContext,
}

impl QuasarError {
    /// Create a new error with code and message.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            context: ErrorContext::default(),
        }
    }

    // --- Convenience constructors ---

    /// Create a render error.
    pub fn render(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::RenderInitialization, message)
    }

    /// Create a window error.
    pub fn window(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::WindowCreation, message)
    }

    /// Create an asset error.
    pub fn asset(target: impl Into<String>) -> Self {
        Self::new(
            ErrorCode::AssetNotFound,
            format!("Asset: {}", target.into()),
        )
    }

    /// Create an ECS error.
    pub fn ecs(target: impl Into<String>) -> Self {
        Self::new(
            ErrorCode::EcsEntityNotFound,
            format!("Entity: {}", target.into()),
        )
    }

    /// Create a script error.
    pub fn script(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::ScriptRuntime, message)
    }

    /// Create a network error.
    pub fn network(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::NetworkConnectionFailed, message)
    }

    /// Create a physics error.
    pub fn physics(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::PhysicsSimulation, message)
    }

    /// Create an audio error.
    pub fn audio(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::AudioInitialization, message)
    }

    /// Create an I/O error.
    pub fn io(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::IoNotFound, message)
    }

    /// Create a generic error.
    pub fn other(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Unknown, message)
    }

    // --- Builder methods ---

    /// Set the error code.
    pub fn with_code(mut self, code: ErrorCode) -> Self {
        self.code = code;
        self
    }

    /// Set the subsystem context.
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context.context = context.into();
        self
    }

    /// Set the operation being performed.
    pub fn with_operation(mut self, operation: impl Into<String>) -> Self {
        self.context.operation = Some(operation.into());
        self
    }

    /// Set the target item.
    pub fn with_target(mut self, target: impl Into<String>) -> Self {
        self.context.target = Some(target.into());
        self
    }

    /// Set the reason for the error.
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.context.reason = Some(reason.into());
        self
    }

    /// Set the source location.
    pub fn with_location(mut self, file: &str, line: u32, column: u32, function: &str) -> Self {
        self.context.location = Some(SourceLocation {
            file: file.to_string(),
            line,
            column,
            function: function.to_string(),
        });
        self
    }

    /// Set the source (nested) error.
    pub fn with_source(mut self, source: QuasarError) -> Self {
        self.context.source = Some(Arc::new(source));
        self
    }

    /// Create from a source error.
    pub fn from_source(source: impl std::error::Error + 'static) -> Self {
        Self::other(source.to_string())
    }

    /// Get the error code.
    pub fn code(&self) -> ErrorCode {
        self.code
    }

    /// Get the error message.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Check if this is a specific error code.
    pub fn is_code(&self, code: ErrorCode) -> bool {
        self.code == code
    }

    /// Format the error as a detailed string.
    pub fn to_detailed_string(&self) -> String {
        let mut output = format!("[{}] {}", self.code as i32, self.message);

        if !self.context.context.is_empty() {
            output.push_str(&format!("\n  Context: {}", self.context.context));
        }

        if let Some(ref op) = self.context.operation {
            output.push_str(&format!("\n  Operation: {}", op));
        }

        if let Some(ref target) = self.context.target {
            output.push_str(&format!("\n  Target: {}", target));
        }

        if let Some(ref reason) = self.context.reason {
            output.push_str(&format!("\n  Reason: {}", reason));
        }

        if let Some(ref loc) = self.context.location {
            output.push_str(&format!("\n  Location: {}", loc));
        }

        if let Some(ref source) = self.context.source {
            output.push_str(&format!("\n  Caused by: {}", source));
        }

        output
    }
}

impl fmt::Display for QuasarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code as i32, self.message)
    }
}

impl std::error::Error for QuasarError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.context.source.as_ref().map(|e| e.as_ref() as _)
    }
}

impl From<std::io::Error> for QuasarError {
    fn from(err: std::io::Error) -> Self {
        let code = match err.kind() {
            std::io::ErrorKind::NotFound => ErrorCode::IoNotFound,
            std::io::ErrorKind::PermissionDenied => ErrorCode::IoPermissionDenied,
            std::io::ErrorKind::InvalidData => ErrorCode::IoInvalidData,
            _ => ErrorCode::Unknown,
        };

        Self::new(code, err.to_string())
    }
}

impl From<serde_json::Error> for QuasarError {
    fn from(err: serde_json::Error) -> Self {
        Self::new(ErrorCode::AssetParseFailed, err.to_string())
    }
}

/// Convenience alias used throughout the engine.
pub type QuasarResult<T> = Result<T, QuasarError>;

/// Macro to create an error with location information.
#[macro_export]
macro_rules! err {
    ($code:expr, $msg:expr) => {
        $crate::error::QuasarError::new($code, $msg)
            .with_location(file!(), line!(), column!(), function!())
    };
    ($code:expr, $msg:expr, $($key:ident = $val:expr),+ $(,)?) => {{
        let mut err = $crate::error::QuasarError::new($code, $msg)
            .with_location(file!(), line!(), column!(), function!());
        $(
            err = err.$key($val);
        )+
        err
    }};
}

/// Macro to bail with an error.
#[macro_export]
macro_rules! bail {
    ($code:expr, $msg:expr) => {
        return Err($crate::err!($code, $msg))
    };
    ($code:expr, $msg:expr, $($key:ident = $val:expr),+ $(,)?) => {
        return Err($crate::err!($code, $msg, $($key = $val),+))
    };
}

/// Macro to ensure a condition, returning an error if false.
#[macro_export]
macro_rules! ensure {
    ($cond:expr, $code:expr, $msg:expr) => {
        if !($cond) {
            return Err($crate::err!($code, $msg));
        }
    };
    ($cond:expr, $code:expr, $msg:expr, $($key:ident = $val:expr),+ $(,)?) => {
        if !($cond) {
            return Err($crate::err!($code, $msg, $($key = $val),+));
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_code_category() {
        assert_eq!(ErrorCode::RenderInitialization.category(), "render");
        assert_eq!(ErrorCode::AssetNotFound.category(), "asset");
        assert_eq!(ErrorCode::EcsEntityNotFound.category(), "ecs");
    }

    #[test]
    fn error_builder() {
        let err = QuasarError::asset("texture.png")
            .with_operation("loading")
            .with_reason("file not found")
            .with_code(ErrorCode::AssetNotFound);

        assert_eq!(err.code(), ErrorCode::AssetNotFound);
        assert_eq!(err.context.operation, Some("loading".to_string()));
        assert_eq!(err.context.reason, Some("file not found".to_string()));
    }

    #[test]
    fn error_display() {
        let err = QuasarError::new(ErrorCode::AssetNotFound, "texture.png not found");
        let display = format!("{}", err);
        assert!(display.contains("3001"));
        assert!(display.contains("texture.png"));
    }

    #[test]
    fn error_detailed_string() {
        let err = QuasarError::asset("model.gltf")
            .with_operation("loading")
            .with_context("scene initialization");

        let detailed = err.to_detailed_string();
        assert!(detailed.contains("Operation: loading"));
        assert!(detailed.contains("Context: scene initialization"));
    }

    #[test]
    fn error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let err: QuasarError = io_err.into();

        assert_eq!(err.code(), ErrorCode::IoNotFound);
    }

    #[test]
    fn error_is_code() {
        let err = QuasarError::asset("test").with_code(ErrorCode::AssetNotFound);
        assert!(err.is_code(ErrorCode::AssetNotFound));
        assert!(!err.is_code(ErrorCode::AssetLoadFailed));
    }
}
