//! XR system unit tests.
//!
//! Tests for XR types and logic that don't require GPU or XR runtime.

use glam::{Quat, Vec3};
use quasar_xr::xr_input::{ButtonState, ControllerAxes, ControllerButtons, XrController};
use quasar_xr::xr_rendering::fov_to_projection;
use quasar_xr::{
    FormFactor, ReferenceSpace, ViewConfigurationType, XrCapabilities, XrError, XrFov,
    XrPerformanceMetrics, XrPose,
};

mod button_state {
    use super::*;

    #[test]
    fn pressed_creates_correct_state() {
        let state = ButtonState::pressed();
        assert!(state.is_pressed);
        assert!(!state.was_pressed);
        assert!((state.value - 1.0).abs() < 0.001);
    }

    #[test]
    fn just_pressed_detection() {
        let mut state = ButtonState::pressed();
        assert!(state.just_pressed());

        state.was_pressed = true;
        assert!(!state.just_pressed());
    }

    #[test]
    fn just_released_detection() {
        let state = ButtonState {
            is_pressed: false,
            was_pressed: true,
            value: 0.0,
        };
        assert!(state.just_released());

        let state = ButtonState {
            is_pressed: false,
            was_pressed: false,
            value: 0.0,
        };
        assert!(!state.just_released());
    }

    #[test]
    fn default_is_released() {
        let state = ButtonState::default();
        assert!(!state.is_pressed);
        assert!(!state.was_pressed);
        assert!(!state.just_pressed());
        assert!(!state.just_released());
    }
}

mod controller {
    use super::*;

    #[test]
    fn controller_default() {
        let controller = XrController::default();
        assert!(!controller.is_active);
        assert_eq!(controller.haptic_intensity, 0.0);
        assert_eq!(controller.haptic_duration, 0.0);
    }

    #[test]
    fn controller_axes_default() {
        let axes = ControllerAxes::default();
        assert!((axes.thumbstick_x).abs() < 0.001);
        assert!((axes.thumbstick_y).abs() < 0.001);
        assert!((axes.trigger).abs() < 0.001);
        assert!((axes.grip).abs() < 0.001);
    }

    #[test]
    fn controller_buttons_default() {
        let buttons = ControllerButtons::default();
        assert!(!buttons.a.is_pressed);
        assert!(!buttons.b.is_pressed);
        assert!(!buttons.trigger.is_pressed);
        assert!(!buttons.grip.is_pressed);
    }
}

mod xr_pose {
    use super::*;

    #[test]
    fn pose_default() {
        let pose = XrPose::default();
        assert!((pose.position - Vec3::ZERO).length() < 0.001);
        assert!((pose.orientation - Quat::IDENTITY).length() < 0.001);
    }

    #[test]
    fn pose_creation() {
        let pose = XrPose {
            position: Vec3::new(1.0, 2.0, 3.0),
            orientation: Quat::from_rotation_y(std::f32::consts::FRAC_PI_4),
        };
        assert!((pose.position.x - 1.0).abs() < 0.001);
    }
}

mod xr_fov {
    use super::*;

    #[test]
    fn fov_default() {
        let fov = XrFov::default();
        assert!((fov.angle_left).abs() < 0.001);
        assert!((fov.angle_right).abs() < 0.001);
    }

    #[test]
    fn fov_symmetric() {
        let fov = XrFov {
            angle_left: -0.8,
            angle_right: 0.8,
            angle_up: 0.9,
            angle_down: -0.9,
        };
        assert!((fov.angle_right + fov.angle_left).abs() < 0.001);
        assert!((fov.angle_up + fov.angle_down).abs() < 0.001);
    }
}

mod fov_projection {
    use super::*;

    #[test]
    fn symmetric_fov_projection() {
        let fov = XrFov {
            angle_left: -0.8,
            angle_right: 0.8,
            angle_up: 0.9,
            angle_down: -0.9,
        };

        let proj = fov_to_projection(&fov, 0.1, 100.0);

        assert!(proj.col(0).x.is_finite());
        assert!(proj.col(1).y.is_finite());
        assert!(proj.col(2).z.is_finite());
        assert!(proj.col(3).w.is_finite());
    }

    #[test]
    fn asymmetric_fov_projection() {
        let fov = XrFov {
            angle_left: -0.6,
            angle_right: 1.0,
            angle_up: 0.9,
            angle_down: -0.7,
        };

        let proj = fov_to_projection(&fov, 0.1, 100.0);

        assert!(proj.col(0).x > 0.0);
        assert!(proj.col(1).y > 0.0);
    }

    #[test]
    fn near_far_plane_values() {
        let fov = XrFov {
            angle_left: -0.8,
            angle_right: 0.8,
            angle_up: 0.9,
            angle_down: -0.9,
        };

        let proj = fov_to_projection(&fov, 0.5, 50.0);

        assert!(proj.col(2).z < 0.0);
        assert!(proj.col(3).z < 0.0);
    }
}

mod xr_types {
    use super::*;

    #[test]
    fn form_factor_variants() {
        let hmd = FormFactor::HeadMountedDisplay;
        let handheld = FormFactor::Handheld;

        assert_ne!(hmd, handheld);
    }

    #[test]
    fn view_configuration_type() {
        let mono = ViewConfigurationType::Mono;
        let stereo = ViewConfigurationType::Stereo;

        assert_ne!(mono, stereo);
    }

    #[test]
    fn reference_space_variants() {
        let local = ReferenceSpace::Local;
        let stage = ReferenceSpace::Stage;
        let view = ReferenceSpace::View;

        assert_ne!(local, stage);
        assert_ne!(stage, view);
    }

    #[test]
    fn xr_capabilities_default() {
        let caps = XrCapabilities::default();
        assert!(!caps.hand_tracking);
        assert!(!caps.eye_tracking);
    }

    #[test]
    fn xr_performance_metrics_default() {
        let metrics = XrPerformanceMetrics::default();
        assert!((metrics.frame_time_ms).abs() < 0.001);
        assert_eq!(metrics.dropped_frames, 0);
    }
}

mod xr_error {
    use super::*;

    #[test]
    fn error_display() {
        let err = XrError::Graphics("test error".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("Graphics"));
    }

    #[test]
    fn error_variants() {
        let e1 = XrError::Graphics("gpu error".to_string());
        let e2 = XrError::NotInitialized;
        let e3 = XrError::UnsupportedFeature("feature".to_string());

        assert!(matches!(e1, XrError::Graphics(_)));
        assert!(matches!(e2, XrError::NotInitialized));
        assert!(matches!(e3, XrError::UnsupportedFeature(_)));
    }
}
