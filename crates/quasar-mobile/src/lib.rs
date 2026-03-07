//! # Quasar Mobile
//!
//! Android & iOS build-target support for the Quasar engine.
//!
//! Provides:
//! - [`TouchInput`]: multi-touch state tracking (up to [`MAX_TOUCH_POINTERS`]).
//! - [`Gesture`] / [`GestureRecognizer`]: tap, swipe, pinch-zoom, rotation.
//! - [`Gyroscope`]: orientation / angular-velocity resource.
//! - [`MobileConfig`]: mobile-specific settings (safe-area, haptics, etc.).
//! - [`MobilePlatform`]: per-platform (Android/iOS) helpers.
//!
//! `winit` already exposes `Touch` events which work on both Android
//! (via `android-activity`) and iOS (via UIKit). This crate wraps them
//! in engine-friendly types and adds gesture recognition.

pub mod gesture;
pub mod gyroscope;
pub mod haptics;
pub mod runner;
pub mod touch;

pub use gesture::{Gesture, GestureRecognizer, SwipeDirection};
pub use gyroscope::Gyroscope;
pub use haptics::{HapticEngine, HapticStyle};
pub use runner::{MobileRunner, run_mobile};
pub use touch::{TouchInput, TouchPhase, TouchPointer, MAX_TOUCH_POINTERS};

/// Mobile-specific configuration.
#[derive(Debug, Clone)]
pub struct MobileConfig {
    /// Logical safe-area insets (top, right, bottom, left) in dp.
    pub safe_area: [f32; 4],
    /// Minimum DPI scale factor (defaults to 1.0).
    pub min_scale_factor: f64,
    /// Enable haptic feedback integration.
    pub haptics_enabled: bool,
    /// Keep screen awake while running.
    pub keep_screen_on: bool,
}

impl Default for MobileConfig {
    fn default() -> Self {
        Self {
            safe_area: [0.0; 4],
            min_scale_factor: 1.0,
            haptics_enabled: true,
            keep_screen_on: true,
        }
    }
}

/// Identifiers for the host platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MobilePlatform {
    Android,
    Ios,
    Unknown,
}

impl MobilePlatform {
    /// Detect the current platform at compile time.
    pub fn current() -> Self {
        if cfg!(target_os = "android") {
            Self::Android
        } else if cfg!(target_os = "ios") {
            Self::Ios
        } else {
            Self::Unknown
        }
    }

    pub fn is_mobile(&self) -> bool {
        matches!(self, Self::Android | Self::Ios)
    }
}
