//! Perspective camera for 3D rendering.

use glam::{Mat4, Vec3};

/// A perspective camera that produces view and projection matrices.
pub struct Camera {
    /// Camera world position.
    pub position: Vec3,
    /// Point the camera is looking at.
    pub target: Vec3,
    /// Up vector (usually Y-up).
    pub up: Vec3,
    /// Vertical field of view in radians.
    pub fov_y: f32,
    /// Viewport aspect ratio (width / height).
    pub aspect: f32,
    /// Near clipping plane distance.
    pub near: f32,
    /// Far clipping plane distance.
    pub far: f32,
    /// Sub-pixel jitter in NDC applied to the projection matrix (TAA).
    pub jitter: (f32, f32),
}

impl Camera {
    /// Create a default camera looking at the origin from a diagonal.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            position: Vec3::new(2.0, 2.0, 3.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            fov_y: std::f32::consts::FRAC_PI_4, // 45°
            aspect: width as f32 / height.max(1) as f32,
            near: 0.1,
            far: 1000.0,
            jitter: (0.0, 0.0),
        }
    }

    /// View matrix (world → camera space).
    pub fn view_matrix(&self) -> Mat4 {
        Mat4::look_at_rh(self.position, self.target, self.up)
    }

    /// Projection matrix (camera → clip space), including TAA jitter if set.
    pub fn projection_matrix(&self) -> Mat4 {
        let mut proj = Mat4::perspective_rh(self.fov_y, self.aspect, self.near, self.far);
        // Apply sub-pixel jitter to the projection translation (col 2, rows 0 & 1).
        proj.col_mut(2).x += self.jitter.0;
        proj.col_mut(2).y += self.jitter.1;
        proj
    }

    /// Combined view-projection matrix.
    pub fn view_projection(&self) -> Mat4 {
        self.projection_matrix() * self.view_matrix()
    }

    /// Update aspect ratio (call on window resize).
    pub fn set_aspect(&mut self, width: u32, height: u32) {
        self.aspect = width as f32 / height.max(1) as f32;
    }

    /// Orbit the camera around the target on the XZ plane.
    pub fn orbit(&mut self, angle: f32) {
        let offset = self.position - self.target;
        let radius = (offset.x * offset.x + offset.z * offset.z).sqrt();
        let current_angle = offset.z.atan2(offset.x);
        let new_angle = current_angle + angle;
        self.position.x = self.target.x + radius * new_angle.cos();
        self.position.z = self.target.z + radius * new_angle.sin();
    }
}

/// The uniform data structure sent to the GPU for camera transforms.
///
/// Must match the layout in the WGSL shader.
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
    pub model: [[f32; 4]; 4],
    pub normal_matrix: [[f32; 4]; 4],
    pub prev_view_proj: [[f32; 4]; 4],
}

impl CameraUniform {
    pub fn new() -> Self {
        Self {
            view_proj: Mat4::IDENTITY.to_cols_array_2d(),
            model: Mat4::IDENTITY.to_cols_array_2d(),
            normal_matrix: Mat4::IDENTITY.to_cols_array_2d(),
            prev_view_proj: Mat4::IDENTITY.to_cols_array_2d(),
        }
    }

    pub fn update(&mut self, camera: &Camera, model: Mat4) {
        // Store previous frame's view_proj before updating
        self.prev_view_proj = self.view_proj;
        self.view_proj = camera.view_projection().to_cols_array_2d();
        self.model = model.to_cols_array_2d();
        self.normal_matrix = model.inverse().transpose().to_cols_array_2d();
    }

    /// Update without storing previous frame (for first frame or reset)
    pub fn update_first_frame(&mut self, camera: &Camera, model: Mat4) {
        self.view_proj = camera.view_projection().to_cols_array_2d();
        self.prev_view_proj = self.view_proj;
        self.model = model.to_cols_array_2d();
        self.normal_matrix = model.inverse().transpose().to_cols_array_2d();
    }

    pub fn from_camera(camera: &Camera) -> Self {
        let mut uniform = Self::new();
        uniform.view_proj = camera.view_projection().to_cols_array_2d();
        uniform
    }
}

impl Default for CameraUniform {
    fn default() -> Self {
        Self::new()
    }
}
