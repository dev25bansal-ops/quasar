//! Haptic feedback abstraction for mobile platforms.
//!
//! Provides a platform-agnostic interface for triggering haptic events.
//! On Android the implementation would call `Vibrator` via JNI; on iOS
//! it would use `UIImpactFeedbackGenerator` via Objective-C FFI.
//! When running on desktop, haptic calls are silently ignored.

/// Intensity / style of a haptic event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HapticStyle {
    /// A light tap — e.g. UI button press.
    Light,
    /// A medium impact — e.g. selection change.
    Medium,
    /// A heavy thud — e.g. collision or error.
    Heavy,
    /// A short, crisp tick — e.g. slider detent.
    Rigid,
    /// A soft, elastic bump.
    Soft,
    /// A success notification pattern.
    Success,
    /// A warning notification pattern.
    Warning,
    /// An error notification pattern.
    Error,
}

/// Haptic feedback controller.
///
/// Create one per application and keep it alive for the app's lifetime.
/// Individual `trigger` calls are cheap.
pub struct HapticEngine {
    enabled: bool,
}

impl HapticEngine {
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    /// Enable or disable haptics at runtime.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Fire a haptic event with the given style.
    pub fn trigger(&self, style: HapticStyle) {
        if !self.enabled {
            return;
        }

        #[cfg(target_os = "android")]
        self.trigger_android(style);

        #[cfg(target_os = "ios")]
        self.trigger_ios(style);

        // On non-mobile platforms, log at trace level for debugging.
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let _ = style;
            log::trace!("Haptic trigger (no-op on desktop): {:?}", style);
        }
    }

    /// Fire a custom vibration pattern (durations in milliseconds,
    /// alternating vibrate/pause starting with vibrate).
    pub fn trigger_pattern(&self, pattern_ms: &[u32]) {
        if !self.enabled || pattern_ms.is_empty() {
            return;
        }

        #[cfg(target_os = "android")]
        {
            let _ = pattern_ms;
            log::trace!("Android custom vibration pattern: {:?}", pattern_ms);
        }

        #[cfg(target_os = "ios")]
        {
            let _ = pattern_ms;
            log::trace!("iOS custom vibration pattern: {:?}", pattern_ms);
        }

        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let _ = pattern_ms;
            log::trace!("Haptic pattern (no-op on desktop): {:?}", pattern_ms);
        }
    }

    #[cfg(target_os = "android")]
    fn trigger_android(&self, style: HapticStyle) {
        // Android haptics via android-activity / JNI would go here.
        // For now, log the intent; actual JNI calls require the
        // `android_activity::AndroidApp` handle at runtime.
        let _ = style;
        log::trace!("Android haptic: {:?}", style);
    }

    #[cfg(target_os = "ios")]
    fn trigger_ios(&self, style: HapticStyle) {
        // iOS haptics via UIKit feedback generators would go here.
        let _ = style;
        log::trace!("iOS haptic: {:?}", style);
    }
}

impl Default for HapticEngine {
    fn default() -> Self {
        Self::new(true)
    }
}
