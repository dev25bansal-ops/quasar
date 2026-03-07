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
        use jni::objects::{JObject, JValue};
        use jni::JNIEnv;

        // Obtain a JNIEnv from the current thread's JavaVM.
        // In practice the VM handle is provided by android-activity at startup
        // and you'd store it in e.g. a static or inside HapticEngine.
        // Here we use the thread-local attach helper.
        let ctx = ndk_context::android_context();
        let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) };
        let Ok(vm) = vm else {
            log::warn!("haptics: could not obtain JavaVM");
            return;
        };
        let mut env = match vm.attach_current_thread() {
            Ok(e) => e,
            Err(e) => {
                log::warn!("haptics: JNI attach failed: {e}");
                return;
            }
        };

        let effect_id: i32 = match style {
            HapticStyle::Light => 1,  // EFFECT_TICK
            HapticStyle::Medium => 0, // EFFECT_CLICK
            HapticStyle::Heavy => 5,  // EFFECT_HEAVY_CLICK
            HapticStyle::Rigid => 1,  // EFFECT_TICK
            HapticStyle::Soft => 0,   // EFFECT_CLICK
            HapticStyle::Success | HapticStyle::Warning | HapticStyle::Error => 0,
        };

        // Get the Vibrator service: context.getSystemService("vibrator")
        let activity = unsafe { JObject::from_raw(ctx.context().cast()) };
        let vibrator_str = match env.new_string("vibrator") {
            Ok(s) => s,
            Err(_) => return,
        };

        let vibrator = match env.call_method(
            &activity,
            "getSystemService",
            "(Ljava/lang/String;)Ljava/lang/Object;",
            &[JValue::Object(&vibrator_str.into())],
        ) {
            Ok(v) => match v.l() {
                Ok(o) => o,
                Err(_) => return,
            },
            Err(_) => return,
        };

        if vibrator.is_null() {
            log::warn!("haptics: Vibrator service not available");
            return;
        }

        // VibrationEffect.createPredefined(effectId) — API 29+
        let vib_effect_class = match env.find_class("android/os/VibrationEffect") {
            Ok(c) => c,
            Err(_) => return,
        };
        let effect = match env.call_static_method(
            vib_effect_class,
            "createPredefined",
            "(I)Landroid/os/VibrationEffect;",
            &[JValue::Int(effect_id)],
        ) {
            Ok(v) => match v.l() {
                Ok(o) => o,
                Err(_) => return,
            },
            Err(_) => {
                // Fallback for older APIs: vibrate(long milliseconds)
                let duration_ms: i64 = match style {
                    HapticStyle::Light | HapticStyle::Rigid => 20,
                    HapticStyle::Medium | HapticStyle::Soft => 40,
                    HapticStyle::Heavy => 80,
                    HapticStyle::Success | HapticStyle::Warning | HapticStyle::Error => 60,
                };
                let _ = env.call_method(
                    &vibrator,
                    "vibrate",
                    "(J)V",
                    &[JValue::Long(duration_ms)],
                );
                return;
            }
        };

        // vibrator.vibrate(effect)
        let _ = env.call_method(
            &vibrator,
            "vibrate",
            "(Landroid/os/VibrationEffect;)V",
            &[JValue::Object(&effect)],
        );
    }

    #[cfg(target_os = "ios")]
    fn trigger_ios(&self, style: HapticStyle) {
        use objc2_ui_kit::{
            UIImpactFeedbackGenerator, UIImpactFeedbackStyle,
            UINotificationFeedbackGenerator, UINotificationFeedbackType,
        };

        match style {
            HapticStyle::Light => unsafe {
                let gen = UIImpactFeedbackGenerator::initWithStyle(
                    UIImpactFeedbackGenerator::alloc(),
                    UIImpactFeedbackStyle::Light,
                );
                gen.impactOccurred();
            },
            HapticStyle::Medium | HapticStyle::Soft => unsafe {
                let gen = UIImpactFeedbackGenerator::initWithStyle(
                    UIImpactFeedbackGenerator::alloc(),
                    UIImpactFeedbackStyle::Medium,
                );
                gen.impactOccurred();
            },
            HapticStyle::Heavy | HapticStyle::Rigid => unsafe {
                let gen = UIImpactFeedbackGenerator::initWithStyle(
                    UIImpactFeedbackGenerator::alloc(),
                    UIImpactFeedbackStyle::Heavy,
                );
                gen.impactOccurred();
            },
            HapticStyle::Success => unsafe {
                let gen = UINotificationFeedbackGenerator::new();
                gen.notificationOccurred(UINotificationFeedbackType::Success);
            },
            HapticStyle::Warning => unsafe {
                let gen = UINotificationFeedbackGenerator::new();
                gen.notificationOccurred(UINotificationFeedbackType::Warning);
            },
            HapticStyle::Error => unsafe {
                let gen = UINotificationFeedbackGenerator::new();
                gen.notificationOccurred(UINotificationFeedbackType::Error);
            },
        }
    }
}

impl Default for HapticEngine {
    fn default() -> Self {
        Self::new(true)
    }
}
