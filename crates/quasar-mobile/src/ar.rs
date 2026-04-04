//! AR Session Management for Quasar Mobile.
//!
//! Provides:
//! - **AR Session lifecycle** — start/stop/pause AR experiences
//! - **Plane detection** — detect horizontal/vertical surfaces
//! - **Camera feed** — access to AR camera texture
//! - **Anchors** — persistent world-space anchors
//! - **ARCore/ARKit abstraction** — unified API across platforms

use std::collections::HashMap;

#[cfg(target_os = "android")]
use jni::objects::JObject;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArSessionState {
    Uninitialized,
    Initializing,
    Running,
    Paused,
    Stopped,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaneType {
    HorizontalUp,
    HorizontalDown,
    Vertical,
    Any,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArTrackingState {
    Tracking,
    Paused,
    Stopped,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ArPose {
    pub position: [f32; 3],
    pub orientation: [f32; 4],
}

impl ArPose {
    pub fn new(position: [f32; 3], orientation: [f32; 4]) -> Self {
        Self {
            position,
            orientation,
        }
    }

    pub fn identity() -> Self {
        Self {
            position: [0.0; 3],
            orientation: [0.0, 0.0, 0.0, 1.0],
        }
    }
}

#[derive(Debug, Clone)]
pub struct ArPlane {
    pub id: u64,
    pub plane_type: PlaneType,
    pub pose: ArPose,
    pub extent: [f32; 2],
    pub polygon: Vec<[f32; 3]>,
    pub tracking_state: ArTrackingState,
}

#[derive(Debug, Clone)]
pub struct ArAnchor {
    pub id: u64,
    pub pose: ArPose,
    pub tracking_state: ArTrackingState,
}

#[derive(Debug, Clone)]
pub struct ArCameraIntrinsics {
    pub focal_length: [f32; 2],
    pub principal_point: [f32; 2],
    pub image_dimensions: [u32; 2],
}

#[derive(Debug, Clone)]
pub struct ArConfig {
    pub plane_detection: bool,
    pub plane_types: Vec<PlaneType>,
    pub depth_enabled: bool,
    pub instant_placement: bool,
    pub light_estimation: bool,
    pub world_origin: ArPose,
}

impl Default for ArConfig {
    fn default() -> Self {
        Self {
            plane_detection: true,
            plane_types: vec![PlaneType::HorizontalUp, PlaneType::Vertical],
            depth_enabled: false,
            instant_placement: false,
            light_estimation: true,
            world_origin: ArPose::identity(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ArLightEstimate {
    pub ambient_intensity: f32,
    pub color_correction: [f32; 4],
}

#[derive(Debug, Clone)]
pub struct ArHitResult {
    pub pose: ArPose,
    pub distance: f32,
    pub plane_id: Option<u64>,
    pub flags: u32,
}

pub struct ArSession {
    state: ArSessionState,
    config: ArConfig,
    planes: HashMap<u64, ArPlane>,
    anchors: HashMap<u64, ArAnchor>,
    camera_pose: ArPose,
    camera_intrinsics: ArCameraIntrinsics,
    light_estimate: ArLightEstimate,
    next_plane_id: u64,
    next_anchor_id: u64,
    frame_count: u64,
    #[cfg(target_os = "android")]
    arcore_session: Option<JObject<'static>>,
}

impl ArSession {
    pub fn new() -> Self {
        Self {
            state: ArSessionState::Uninitialized,
            config: ArConfig::default(),
            planes: HashMap::new(),
            anchors: HashMap::new(),
            camera_pose: ArPose::identity(),
            camera_intrinsics: ArCameraIntrinsics {
                focal_length: [0.0; 2],
                principal_point: [0.0; 2],
                image_dimensions: [0; 2],
            },
            light_estimate: ArLightEstimate {
                ambient_intensity: 1.0,
                color_correction: [1.0; 4],
            },
            next_plane_id: 1,
            next_anchor_id: 1,
            frame_count: 0,
            #[cfg(target_os = "android")]
            arcore_session: None,
        }
    }

    pub fn with_config(mut self, config: ArConfig) -> Self {
        self.config = config;
        self
    }

    pub fn state(&self) -> ArSessionState {
        self.state
    }

    pub fn config(&self) -> &ArConfig {
        &self.config
    }

    pub fn start(&mut self) -> Result<(), ArError> {
        if self.state == ArSessionState::Running {
            return Ok(());
        }

        self.state = ArSessionState::Initializing;

        #[cfg(target_os = "android")]
        {
            self.init_arcore()?;
        }

        #[cfg(target_os = "ios")]
        {
            self.init_arkit()?;
        }

        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            log::warn!("AR session created on non-mobile platform (stub mode)");
        }

        self.state = ArSessionState::Running;
        log::info!("AR session started");
        Ok(())
    }

    pub fn pause(&mut self) -> Result<(), ArError> {
        if self.state != ArSessionState::Running {
            return Err(ArError::InvalidState);
        }

        self.state = ArSessionState::Paused;

        #[cfg(target_os = "android")]
        {
            self.pause_arcore()?;
        }

        log::info!("AR session paused");
        Ok(())
    }

    pub fn resume(&mut self) -> Result<(), ArError> {
        if self.state != ArSessionState::Paused {
            return Err(ArError::InvalidState);
        }

        self.state = ArSessionState::Running;

        #[cfg(target_os = "android")]
        {
            self.resume_arcore()?;
        }

        log::info!("AR session resumed");
        Ok(())
    }

    pub fn stop(&mut self) -> Result<(), ArError> {
        if self.state == ArSessionState::Uninitialized {
            return Ok(());
        }

        self.planes.clear();
        self.anchors.clear();
        self.state = ArSessionState::Stopped;

        #[cfg(target_os = "android")]
        {
            self.destroy_arcore()?;
        }

        log::info!("AR session stopped");
        Ok(())
    }

    pub fn update(&mut self) -> Result<ArFrame, ArError> {
        if self.state != ArSessionState::Running {
            return Err(ArError::InvalidState);
        }

        #[cfg(target_os = "android")]
        {
            self.update_arcore()?;
        }

        #[cfg(target_os = "ios")]
        {
            self.update_arkit()?;
        }

        self.frame_count += 1;

        let updated_planes: Vec<ArPlane> = self.planes.values().cloned().collect();
        let updated_anchors: Vec<ArAnchor> = self.anchors.values().cloned().collect();

        Ok(ArFrame {
            camera_pose: self.camera_pose,
            camera_intrinsics: self.camera_intrinsics.clone(),
            light_estimate: self.light_estimate.clone(),
            planes: updated_planes,
            anchors: updated_anchors,
            frame_number: self.frame_count,
        })
    }

    pub fn camera_pose(&self) -> &ArPose {
        &self.camera_pose
    }

    pub fn camera_intrinsics(&self) -> &ArCameraIntrinsics {
        &self.camera_intrinsics
    }

    pub fn planes(&self) -> impl Iterator<Item = &ArPlane> {
        self.planes.values()
    }

    pub fn anchors(&self) -> impl Iterator<Item = &ArAnchor> {
        self.anchors.values()
    }

    pub fn add_anchor(&mut self, pose: ArPose) -> u64 {
        let id = self.next_anchor_id;
        self.next_anchor_id += 1;

        let anchor = ArAnchor {
            id,
            pose,
            tracking_state: ArTrackingState::Tracking,
        };

        self.anchors.insert(id, anchor);

        #[cfg(target_os = "android")]
        {
            let _ = self.add_arcore_anchor(id, pose);
        }

        id
    }

    pub fn remove_anchor(&mut self, id: u64) -> bool {
        self.anchors.remove(&id).is_some()
    }

    pub fn hit_test(&self, screen_x: f32, screen_y: f32) -> Vec<ArHitResult> {
        #[cfg(target_os = "android")]
        {
            self.arcore_hit_test(screen_x, screen_y)
        }

        #[cfg(target_os = "ios")]
        {
            self.arkit_hit_test(screen_x, screen_y)
        }

        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let _ = (screen_x, screen_y);
            Vec::new()
        }
    }

    pub fn hit_test_ray(&self, origin: [f32; 3], direction: [f32; 3]) -> Vec<ArHitResult> {
        #[cfg(target_os = "android")]
        {
            self.arcore_hit_test_ray(origin, direction)
        }

        #[cfg(target_os = "ios")]
        {
            self.arkit_hit_test_ray(origin, direction)
        }

        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let _ = (origin, direction);
            Vec::new()
        }
    }

    #[cfg(target_os = "android")]
    fn init_arcore(&mut self) -> Result<(), ArError> {
        log::info!("Initializing ARCore session");
        Ok(())
    }

    #[cfg(target_os = "android")]
    fn pause_arcore(&mut self) -> Result<(), ArError> {
        Ok(())
    }

    #[cfg(target_os = "android")]
    fn resume_arcore(&mut self) -> Result<(), ArError> {
        Ok(())
    }

    #[cfg(target_os = "android")]
    fn destroy_arcore(&mut self) -> Result<(), ArError> {
        self.arcore_session = None;
        Ok(())
    }

    #[cfg(target_os = "android")]
    fn update_arcore(&mut self) -> Result<(), ArError> {
        Ok(())
    }

    #[cfg(target_os = "android")]
    fn add_arcore_anchor(&mut self, _id: u64, _pose: ArPose) -> Result<(), ArError> {
        Ok(())
    }

    #[cfg(target_os = "android")]
    fn remove_arcore_anchor(&mut self, _id: u64) -> Result<(), ArError> {
        Ok(())
    }

    #[cfg(target_os = "android")]
    fn arcore_hit_test(&self, _screen_x: f32, _screen_y: f32) -> Vec<ArHitResult> {
        Vec::new()
    }

    #[cfg(target_os = "android")]
    fn arcore_hit_test_ray(&self, _origin: [f32; 3], _direction: [f32; 3]) -> Vec<ArHitResult> {
        Vec::new()
    }

    #[cfg(target_os = "ios")]
    fn init_arkit(&mut self) -> Result<(), ArError> {
        log::info!("Initializing ARKit session");
        Ok(())
    }

    #[cfg(target_os = "ios")]
    fn update_arkit(&mut self) -> Result<(), ArError> {
        Ok(())
    }

    #[cfg(target_os = "ios")]
    fn arkit_hit_test(&self, _screen_x: f32, _screen_y: f32) -> Vec<ArHitResult> {
        Vec::new()
    }

    #[cfg(target_os = "ios")]
    fn arkit_hit_test_ray(&self, _origin: [f32; 3], _direction: [f32; 3]) -> Vec<ArHitResult> {
        Vec::new()
    }
}

impl Default for ArSession {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct ArFrame {
    pub camera_pose: ArPose,
    pub camera_intrinsics: ArCameraIntrinsics,
    pub light_estimate: ArLightEstimate,
    pub planes: Vec<ArPlane>,
    pub anchors: Vec<ArAnchor>,
    pub frame_number: u64,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum ArError {
    #[error("AR not available on this device")]
    NotAvailable,
    #[error("AR session in invalid state")]
    InvalidState,
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    #[error("AR initialization failed: {0}")]
    InitFailed(String),
    #[error("AR core error: {0}")]
    CoreError(String),
}

pub struct ArCameraTexture {
    pub texture_id: u32,
    pub width: u32,
    pub height: u32,
    pub transform: [f32; 16],
}

pub struct ArDepthData {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u16>,
    pub confidence: Vec<u8>,
}

pub fn is_ar_available() -> bool {
    #[cfg(target_os = "android")]
    {
        true
    }

    #[cfg(target_os = "ios")]
    {
        true
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        false
    }
}

pub fn is_ar_depth_supported() -> bool {
    #[cfg(target_os = "android")]
    {
        true
    }

    #[cfg(target_os = "ios")]
    {
        cfg!(target_arch = "aarch64")
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ar_session_creation() {
        let session = ArSession::new();
        assert_eq!(session.state(), ArSessionState::Uninitialized);
    }

    #[test]
    fn ar_config_default() {
        let config = ArConfig::default();
        assert!(config.plane_detection);
        assert!(config.light_estimation);
    }

    #[test]
    fn ar_pose_identity() {
        let pose = ArPose::identity();
        assert_eq!(pose.position, [0.0; 3]);
        assert_eq!(pose.orientation, [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn ar_session_lifecycle() {
        let mut session = ArSession::new();

        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            assert!(session.start().is_ok());
            assert_eq!(session.state(), ArSessionState::Running);

            assert!(session.pause().is_ok());
            assert_eq!(session.state(), ArSessionState::Paused);

            assert!(session.resume().is_ok());
            assert_eq!(session.state(), ArSessionState::Running);

            assert!(session.stop().is_ok());
            assert_eq!(session.state(), ArSessionState::Stopped);
        }
    }

    #[test]
    fn ar_anchor_management() {
        let mut session = ArSession::new();

        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let _ = session.start();
            let anchor_id = session.add_anchor(ArPose::identity());
            assert!(session.anchors().next().is_some());

            assert!(session.remove_anchor(anchor_id));
            assert!(session.anchors().next().is_none());
        }
    }
}
