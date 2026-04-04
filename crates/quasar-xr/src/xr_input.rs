//! XR Input — controller and hand tracking.
//!
//! Provides:
//! - Tracked controller input
//! - Hand tracking with skeletal animation
//! - Haptic feedback
//! - Gesture recognition

use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};

/// 6DOF pose for XR tracking.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct XrPose {
    pub position: Vec3,
    pub orientation: Quat,
}

/// Tracked controller state.
#[derive(Debug, Clone, Default)]
pub struct XrController {
    pub is_active: bool,
    pub grip_pose: XrPose,
    pub aim_pose: XrPose,
    pub buttons: ControllerButtons,
    pub axes: ControllerAxes,
    pub haptic_intensity: f32,
    pub haptic_duration: f32,
}

/// Digital button states.
#[derive(Debug, Clone, Copy, Default)]
pub struct ButtonState {
    pub is_pressed: bool,
    pub was_pressed: bool,
    pub value: f32,
}

impl ButtonState {
    pub fn pressed() -> Self {
        Self {
            is_pressed: true,
            was_pressed: false,
            value: 1.0,
        }
    }

    pub fn just_pressed(&self) -> bool {
        self.is_pressed && !self.was_pressed
    }

    pub fn just_released(&self) -> bool {
        !self.is_pressed && self.was_pressed
    }
}

/// Controller button inputs.
#[derive(Debug, Clone, Default)]
pub struct ControllerButtons {
    /// Primary button (A/X).
    pub a: ButtonState,
    /// Secondary button (B/Y).
    pub b: ButtonState,
    /// System/menu button.
    pub system: ButtonState,
    /// Grip/hold button.
    pub grip: ButtonState,
    /// Trigger button.
    pub trigger: ButtonState,
    /// Thumbstick click.
    pub thumbstick: ButtonState,
}

/// Analog axis inputs.
#[derive(Debug, Clone, Copy, Default)]
pub struct ControllerAxes {
    /// Thumbstick X axis (-1 to 1).
    pub thumbstick_x: f32,
    /// Thumbstick Y axis (-1 to 1).
    pub thumbstick_y: f32,
    /// Trigger value (0 to 1).
    pub trigger: f32,
    /// Grip value (0 to 1).
    pub grip: f32,
}

/// Hand tracking data with skeletal joints.
#[derive(Debug, Clone)]
pub struct XrHand {
    /// Whether hand tracking is active.
    pub is_active: bool,
    /// Joint poses (26 joints per hand).
    pub joints: [XrPose; 26],
    /// Joint radii for collision.
    pub joint_radii: [f32; 26],
    /// Handedness.
    pub handedness: Hand,
    /// Pinch strength (0-1).
    pub pinch_strength: f32,
    /// Grip strength (0-1).
    pub grip_strength: f32,
}

impl Default for XrHand {
    fn default() -> Self {
        Self {
            is_active: false,
            joints: [XrPose::default(); 26],
            joint_radii: [0.01; 26],
            handedness: Hand::Right,
            pinch_strength: 0.0,
            grip_strength: 0.0,
        }
    }
}

/// Hand identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Hand {
    Left,
    Right,
}

/// Joint indices in the hand skeleton.
#[repr(usize)]
pub enum HandJoint {
    Palm = 0,
    Wrist = 1,
    ThumbMetacarpal = 2,
    ThumbProximal = 3,
    ThumbDistal = 4,
    ThumbTip = 5,
    IndexMetacarpal = 6,
    IndexProximal = 7,
    IndexIntermediate = 8,
    IndexDistal = 9,
    IndexTip = 10,
    MiddleMetacarpal = 11,
    MiddleProximal = 12,
    MiddleIntermediate = 13,
    MiddleDistal = 14,
    MiddleTip = 15,
    RingMetacarpal = 16,
    RingProximal = 17,
    RingIntermediate = 18,
    RingDistal = 19,
    RingTip = 20,
    LittleMetacarpal = 21,
    LittleProximal = 22,
    LittleIntermediate = 23,
    LittleDistal = 24,
    LittleTip = 25,
}

impl XrHand {
    /// Get the tip position of a finger.
    pub fn finger_tip(&self, finger: Finger) -> Vec3 {
        let joint_idx = match finger {
            Finger::Thumb => HandJoint::ThumbTip as usize,
            Finger::Index => HandJoint::IndexTip as usize,
            Finger::Middle => HandJoint::MiddleTip as usize,
            Finger::Ring => HandJoint::RingTip as usize,
            Finger::Little => HandJoint::LittleTip as usize,
        };
        self.joints[joint_idx].position
    }

    /// Check if the hand is making a pinch gesture.
    pub fn is_pinching(&self) -> bool {
        self.pinch_strength > 0.8
    }

    /// Check if the hand is making a fist.
    pub fn is_fist(&self) -> bool {
        self.grip_strength > 0.9
    }

    /// Check if pointing with index finger.
    pub fn is_pointing(&self) -> bool {
        let index_extended = self.joints[HandJoint::IndexTip as usize]
            .position
            .distance(self.joints[HandJoint::IndexProximal as usize].position)
            > 0.05;

        let middle_curled = self.joints[HandJoint::MiddleTip as usize]
            .position
            .distance(self.joints[HandJoint::MiddleProximal as usize].position)
            < 0.04;

        index_extended && middle_curled
    }
}

/// Finger identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finger {
    Thumb,
    Index,
    Middle,
    Ring,
    Little,
}

/// Haptic feedback profile.
#[derive(Debug, Clone)]
pub struct HapticProfile {
    /// Name of the haptic effect.
    pub name: String,
    /// Amplitude over time (0-1).
    pub amplitude: Vec<f32>,
    /// Frequency modulation over time.
    pub frequency: Vec<f32>,
    /// Duration in seconds.
    pub duration: f32,
}

impl HapticProfile {
    /// Simple pulse haptic.
    pub fn pulse(intensity: f32, duration: f32) -> Self {
        Self {
            name: "pulse".to_string(),
            amplitude: vec![intensity],
            frequency: vec![1.0],
            duration,
        }
    }

    /// Double tap pattern.
    pub fn double_tap() -> Self {
        Self {
            name: "double_tap".to_string(),
            amplitude: vec![0.8, 0.0, 0.8],
            frequency: vec![1.0, 0.0, 1.0],
            duration: 0.15,
        }
    }

    /// Ramp up vibration.
    pub fn ramp_up(duration: f32) -> Self {
        Self {
            name: "ramp_up".to_string(),
            amplitude: vec![0.0, 0.25, 0.5, 0.75, 1.0],
            frequency: vec![1.0; 5],
            duration,
        }
    }
}

/// Input system for XR controllers and hands.
pub struct XrInputSystem {
    /// Right controller state.
    pub right_controller: XrController,
    /// Left controller state.
    pub left_controller: XrController,
    /// Right hand tracking.
    pub right_hand: XrHand,
    /// Left hand tracking.
    pub left_hand: XrHand,
    /// Whether hand tracking is available.
    pub hand_tracking_available: bool,
}

impl XrInputSystem {
    /// Create a new input system.
    pub fn new() -> Self {
        Self {
            right_controller: XrController::default(),
            left_controller: XrController::default(),
            right_hand: XrHand::default(),
            left_hand: XrHand::default(),
            hand_tracking_available: false,
        }
    }

    /// Update button states (call each frame).
    pub fn update(&mut self) {
        self.right_controller.buttons.update();
        self.left_controller.buttons.update();
    }

    /// Trigger haptic feedback on a controller.
    pub fn trigger_haptic(&mut self, hand: Hand, profile: &HapticProfile) {
        let controller = match hand {
            Hand::Right => &mut self.right_controller,
            Hand::Left => &mut self.left_controller,
        };

        if !profile.amplitude.is_empty() {
            controller.haptic_intensity = profile.amplitude[0];
            controller.haptic_duration = profile.duration;
        }
    }

    /// Get the active controller for a hand.
    pub fn controller(&self, hand: Hand) -> &XrController {
        match hand {
            Hand::Right => &self.right_controller,
            Hand::Left => &self.left_controller,
        }
    }

    /// Get mutable controller for a hand.
    pub fn controller_mut(&mut self, hand: Hand) -> &mut XrController {
        match hand {
            Hand::Right => &mut self.right_controller,
            Hand::Left => &mut self.left_controller,
        }
    }
}

impl Default for XrInputSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl ControllerButtons {
    /// Update button states (call each frame before polling).
    pub fn update(&mut self) {
        self.a.was_pressed = self.a.is_pressed;
        self.b.was_pressed = self.b.is_pressed;
        self.system.was_pressed = self.system.is_pressed;
        self.grip.was_pressed = self.grip.is_pressed;
        self.trigger.was_pressed = self.trigger.is_pressed;
        self.thumbstick.was_pressed = self.thumbstick.is_pressed;
    }
}
