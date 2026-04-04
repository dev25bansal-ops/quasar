//! XR Types — common types for VR/AR applications.

use serde::{Deserialize, Serialize};

pub use crate::xr_input::{ControllerAxes, ControllerButtons, Hand, XrController, XrHand, XrPose};
pub use crate::xr_rendering::{FoveationLevel, FoveationParams, XrRenderTarget, XrViewUniform};
pub use crate::XrFov;

pub use crate::XrView;

/// Form factor for XR devices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FormFactor {
    HeadMountedDisplay,
    Handheld,
}

/// View configuration type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewConfigurationType {
    Mono,
    Stereo,
}

/// Reference space type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReferenceSpace {
    Local,
    Stage,
    View,
}

/// XR device capabilities.
#[derive(Debug, Clone, Default)]
pub struct XrCapabilities {
    pub positional_tracking: bool,
    pub orientation_tracking: bool,
    pub hand_tracking: bool,
    pub eye_tracking: bool,
    pub passthrough: bool,
    pub haptics: bool,
    pub max_resolution: [u32; 2],
    pub recommended_resolution: [u32; 2],
    pub refresh_rate: f32,
}

/// Performance metrics for XR rendering.
#[derive(Debug, Clone, Default)]
pub struct XrPerformanceMetrics {
    pub frame_time_ms: f32,
    pub gpu_time_ms: f32,
    pub cpu_time_ms: f32,
    pub dropped_frames: u64,
    pub reprojection_ratio: f32,
}
