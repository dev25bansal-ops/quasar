//! Reusable camera controllers for common movement patterns.
//!
//! * [`OrbitController`] — orbits around a target point (great for model
//!   viewers, editors, third-person cameras).
//! * [`FpsCameraController`] — typical first-person shooter camera with WASD +
//!   mouse look.
//!
//! Both controllers operate on a [`Camera`] reference and a delta-time value.

use glam::Vec3;

use super::Camera;

// ---------------------------------------------------------------------------
// Orbit Controller
// ---------------------------------------------------------------------------

/// Orbits a camera around a target point, controlled by yaw/pitch/zoom deltas.
///
/// # Example
///
/// ```rust,no_run
/// # use quasar_render::camera::Camera;
/// # use quasar_render::camera_controller::OrbitController;
/// let mut camera = Camera::new(1280, 720);
/// let mut orbit = OrbitController::new(5.0);
/// // Per frame:
/// orbit.rotate(0.01, 0.005);   // mouse drag
/// orbit.zoom(-0.5);            // scroll wheel
/// orbit.apply(&mut camera);
/// ```
pub struct OrbitController {
    /// Horizontal angle (radians), 0 = +X axis.
    pub yaw: f32,
    /// Vertical angle (radians), 0 = horizon.
    pub pitch: f32,
    /// Distance from the target point.
    pub distance: f32,
    /// Point the camera orbits around.
    pub target: Vec3,

    /// Mouse-drag sensitivity for yaw / pitch.
    pub rotate_speed: f32,
    /// Scroll sensitivity for zoom.
    pub zoom_speed: f32,

    /// Minimum distance (prevents camera going inside the object).
    pub min_distance: f32,
    /// Maximum distance.
    pub max_distance: f32,

    /// Minimum pitch (radians) – prevents flipping.
    pub min_pitch: f32,
    /// Maximum pitch (radians).
    pub max_pitch: f32,
}

impl OrbitController {
    /// Create a controller with a given initial distance from the target.
    pub fn new(distance: f32) -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.3, // slightly above horizon
            distance,
            target: Vec3::ZERO,
            rotate_speed: 0.005,
            zoom_speed: 0.5,
            min_distance: 0.5,
            max_distance: 200.0,
            min_pitch: -std::f32::consts::FRAC_PI_2 + 0.01,
            max_pitch: std::f32::consts::FRAC_PI_2 - 0.01,
        }
    }

    /// Add yaw (horizontal) and pitch (vertical) in **pixels** or raw deltas.
    /// Internally scaled by `rotate_speed`.
    pub fn rotate(&mut self, dx: f32, dy: f32) {
        self.yaw += dx * self.rotate_speed;
        self.pitch = (self.pitch - dy * self.rotate_speed).clamp(self.min_pitch, self.max_pitch);
    }

    /// Zoom by a scroll delta.  Positive = closer, negative = farther.
    pub fn zoom(&mut self, delta: f32) {
        self.distance =
            (self.distance - delta * self.zoom_speed).clamp(self.min_distance, self.max_distance);
    }

    /// Pan the target by a screen-space delta (right, up).
    pub fn pan(&mut self, right: f32, up: f32) {
        let forward = Vec3::new(self.yaw.cos(), 0.0, self.yaw.sin()).normalize();
        let right_dir = forward.cross(Vec3::Y).normalize();
        self.target += right_dir * right * 0.01;
        self.target += Vec3::Y * up * 0.01;
    }

    /// Write the computed position into the [`Camera`].
    pub fn apply(&self, camera: &mut Camera) {
        let x = self.distance * self.pitch.cos() * self.yaw.cos();
        let y = self.distance * self.pitch.sin();
        let z = self.distance * self.pitch.cos() * self.yaw.sin();
        camera.position = self.target + Vec3::new(x, y, z);
        camera.target = self.target;
    }
}

// ---------------------------------------------------------------------------
// FPS Camera Controller
// ---------------------------------------------------------------------------

/// First-person camera controller with WASD movement and mouse look.
///
/// The controller doesn't read input directly — instead you feed it logical
/// movement and look deltas each frame.
///
/// # Example
///
/// ```rust,no_run
/// # use quasar_render::camera::Camera;
/// # use quasar_render::camera_controller::FpsCameraController;
/// # let mouse_dx = 0.0f32;
/// # let mouse_dy = 0.0f32;
/// # let dt = 0.016f32;
/// let mut camera = Camera::new(1280, 720);
/// let mut fps = FpsCameraController::new();
/// fps.move_speed = 10.0;
/// // Per frame:
/// fps.mouse_look(mouse_dx, mouse_dy);
/// fps.move_forward(dt);
/// fps.apply(&mut camera);
/// ```
pub struct FpsCameraController {
    /// Camera position in world space.
    pub position: Vec3,
    /// Yaw angle (radians), 0 = looking toward -Z.
    pub yaw: f32,
    /// Pitch angle (radians).
    pub pitch: f32,

    /// Movement speed in units/second.
    pub move_speed: f32,
    /// Mouse sensitivity.
    pub look_sensitivity: f32,

    /// Minimum pitch.
    pub min_pitch: f32,
    /// Maximum pitch.
    pub max_pitch: f32,
}

impl FpsCameraController {
    pub fn new() -> Self {
        Self {
            position: Vec3::new(0.0, 1.6, 5.0), // eye-height
            yaw: -std::f32::consts::FRAC_PI_2,  // facing -Z
            pitch: 0.0,
            move_speed: 5.0,
            look_sensitivity: 0.003,
            min_pitch: -std::f32::consts::FRAC_PI_2 + 0.01,
            max_pitch: std::f32::consts::FRAC_PI_2 - 0.01,
        }
    }

    /// Rotate the view based on mouse deltas.
    pub fn mouse_look(&mut self, dx: f32, dy: f32) {
        self.yaw += dx * self.look_sensitivity;
        self.pitch =
            (self.pitch - dy * self.look_sensitivity).clamp(self.min_pitch, self.max_pitch);
    }

    /// Forward direction vector (on the XZ plane).
    pub fn forward(&self) -> Vec3 {
        Vec3::new(self.yaw.cos(), 0.0, self.yaw.sin()).normalize()
    }

    /// Right direction vector.
    pub fn right(&self) -> Vec3 {
        self.forward().cross(Vec3::Y).normalize()
    }

    /// Move forward (positive dt) or backward (negative dt).
    pub fn move_forward(&mut self, dt: f32) {
        self.position += self.forward() * self.move_speed * dt;
    }

    /// Move right (positive dt) or left (negative dt).
    pub fn move_right(&mut self, dt: f32) {
        self.position += self.right() * self.move_speed * dt;
    }

    /// Move up (positive dt) or down (negative dt) — world Y axis.
    pub fn move_up(&mut self, dt: f32) {
        self.position += Vec3::Y * self.move_speed * dt;
    }

    /// Calculate the full 3D look direction (including pitch).
    pub fn look_direction(&self) -> Vec3 {
        Vec3::new(
            self.yaw.cos() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.sin() * self.pitch.cos(),
        )
        .normalize()
    }

    /// Apply the controller state to a [`Camera`].
    pub fn apply(&self, camera: &mut Camera) {
        camera.position = self.position;
        camera.target = self.position + self.look_direction();
    }
}

impl Default for FpsCameraController {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orbit_apply_sets_camera_position() {
        let mut camera = Camera::new(800, 600);
        let mut orbit = OrbitController::new(10.0);
        orbit.target = Vec3::ZERO;
        orbit.yaw = 0.0;
        orbit.pitch = 0.0;
        orbit.apply(&mut camera);

        // At yaw=0, pitch=0, camera should be at (distance, 0, 0).
        assert!((camera.position.x - 10.0).abs() < 0.001);
        assert!(camera.position.y.abs() < 0.001);
        assert!(camera.position.z.abs() < 0.001);
        assert_eq!(camera.target, Vec3::ZERO);
    }

    #[test]
    fn orbit_zoom_clamps() {
        let mut orbit = OrbitController::new(5.0);
        orbit.min_distance = 1.0;
        orbit.max_distance = 50.0;
        orbit.zoom(1000.0); // way too much
        assert!(orbit.distance >= orbit.min_distance);
        orbit.zoom(-1000.0);
        assert!(orbit.distance <= orbit.max_distance);
    }

    #[test]
    fn orbit_pitch_clamps() {
        let mut orbit = OrbitController::new(5.0);
        orbit.rotate(0.0, -100000.0); // extreme up
        assert!(orbit.pitch <= orbit.max_pitch);
        orbit.rotate(0.0, 100000.0); // extreme down
        assert!(orbit.pitch >= orbit.min_pitch);
    }

    #[test]
    fn fps_forward_backward() {
        let mut fps = FpsCameraController::new();
        fps.yaw = 0.0; // looking +X
        let start = fps.position;
        fps.move_forward(1.0);
        assert!(fps.position.x > start.x, "should move in +X direction");
    }

    #[test]
    fn fps_apply_sets_camera() {
        let mut camera = Camera::new(800, 600);
        let fps = FpsCameraController::new();
        fps.apply(&mut camera);
        assert_eq!(camera.position, fps.position);
        // target should be ahead of position
        assert!((camera.target - camera.position).length() > 0.5);
    }

    #[test]
    fn fps_mouse_look_clamps_pitch() {
        let mut fps = FpsCameraController::new();
        fps.mouse_look(0.0, -1_000_000.0);
        assert!(fps.pitch <= fps.max_pitch);
        fps.mouse_look(0.0, 1_000_000.0);
        assert!(fps.pitch >= fps.min_pitch);
    }
}
