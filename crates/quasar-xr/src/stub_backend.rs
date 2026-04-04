//! Stub backend implementation (when OpenXR feature is disabled).

use crate::{XrError, XrFov, XrPose, XrResult};

pub struct XrView {
    pub pose: XrPose,
    pub fov: XrFov,
}

pub struct XrConfig {
    pub app_name: String,
    pub app_version: u32,
}

impl Default for XrConfig {
    fn default() -> Self {
        Self {
            app_name: "Quasar XR".to_string(),
            app_version: 1,
        }
    }
}

pub struct XrSession {
    views: Vec<XrView>,
    right_hand_pose: Option<XrPose>,
    left_hand_pose: Option<XrPose>,
    is_focused: bool,
}

impl XrSession {
    pub fn new(_config: XrConfig) -> XrResult<Self> {
        log::info!("XR session created (stub - openxr feature disabled)");
        Ok(Self {
            views: Vec::new(),
            right_hand_pose: None,
            left_hand_pose: None,
            is_focused: false,
        })
    }

    pub fn views(&self) -> &[XrView] {
        &self.views
    }

    pub fn right_hand_pose(&self) -> Option<XrPose> {
        self.right_hand_pose
    }

    pub fn left_hand_pose(&self) -> Option<XrPose> {
        self.left_hand_pose
    }

    pub fn is_focused(&self) -> bool {
        self.is_focused
    }

    pub fn wait_frame(&mut self) -> XrResult<()> {
        Ok(())
    }

    pub fn begin_frame(&self) -> XrResult<()> {
        Ok(())
    }

    pub fn end_frame(&self) -> XrResult<()> {
        Ok(())
    }

    pub fn locate_views(&mut self) -> XrResult<()> {
        Ok(())
    }

    pub fn sync_actions(&mut self) -> XrResult<()> {
        Ok(())
    }

    pub fn is_running(&self) -> bool {
        false
    }

    pub fn should_render(&self) -> bool {
        false
    }

    pub fn request_exit_session(&self) -> XrResult<()> {
        Ok(())
    }
}

pub struct XrSystem;

impl quasar_core::ecs::System for XrSystem {
    fn name(&self) -> &str {
        "xr_system"
    }

    fn run(&mut self, world: &mut quasar_core::ecs::World) {
        let _ = world;
    }
}
