//! Quasar XR - VR/AR support via OpenXR.
//!
//! Provides:
//! - **OpenXR integration** — cross-platform VR/AR support
//! - **XR rendering** — stereoscopic rendering with proper projection
//! - **Controller input** — tracked controllers with haptic feedback
//! - **Hand tracking** — skeletal hand tracking when available
//! - **Passthrough** — AR passthrough camera on supported devices

#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(feature = "openxr")]
mod openxr_backend;

#[cfg(not(feature = "openxr"))]
mod stub_backend;

pub mod xr_input;
pub mod xr_rendering;
pub mod xr_types;

pub use xr_input::XrPose;

#[cfg(feature = "openxr")]
pub use openxr_backend::{XrConfig, XrSession, XrSystem, XrView};

#[cfg(not(feature = "openxr"))]
pub use stub_backend::{XrConfig, XrSession, XrSystem, XrView};

pub use xr_input::{ControllerAxes, ControllerButtons, Hand, XrController, XrHand};
pub use xr_rendering::{FoveationLevel, FoveationParams, XrRenderTarget, XrViewUniform};
pub use xr_types::{
    FormFactor, ReferenceSpace, ViewConfigurationType, XrCapabilities, XrPerformanceMetrics,
};

/// Field of view angles.
#[derive(Debug, Clone, Copy, Default)]
pub struct XrFov {
    pub angle_left: f32,
    pub angle_right: f32,
    pub angle_up: f32,
    pub angle_down: f32,
}

/// Common types for XR applications.
pub mod prelude {
    pub use crate::xr_input::*;
    pub use crate::xr_rendering::*;
    pub use crate::xr_types::*;
    pub use crate::{XrConfig, XrFov, XrPose, XrSession, XrSystem, XrView};
}

/// Error type for XR operations.
#[derive(Debug, thiserror::Error)]
pub enum XrError {
    #[cfg(feature = "openxr")]
    #[error("OpenXR error: {0}")]
    OpenXr(#[from] openxr::sys::Result),

    #[error("Graphics error: {0}")]
    Graphics(String),

    #[error("Session not initialized")]
    NotInitialized,

    #[error("Unsupported feature: {0}")]
    UnsupportedFeature(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result type for XR operations.
pub type XrResult<T> = Result<T, XrError>;
