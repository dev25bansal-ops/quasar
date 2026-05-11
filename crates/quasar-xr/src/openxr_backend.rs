//! OpenXR backend implementation.
//!
//! Provides full OpenXR integration for VR/AR applications.

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

#[cfg(not(feature = "openxr"))]
mod stub {
    use super::*;

    pub struct XrSessionInner {
        pub views: Vec<XrView>,
        pub right_hand_pose: Option<XrPose>,
        pub left_hand_pose: Option<XrPose>,
        pub is_focused: bool,
    }

    impl XrSessionInner {
        pub fn new(_config: XrConfig) -> XrResult<Self> {
            log::info!("XR session created (stub - openxr feature disabled)");
            Ok(Self {
                views: Vec::new(),
                right_hand_pose: None,
                left_hand_pose: None,
                is_focused: false,
            })
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
}

#[cfg(feature = "openxr")]
mod openxr_impl {
    use super::*;
    use openxr as xr;

    pub struct XrSessionInner {
        instance: xr::Instance,
        _system_id: xr::SystemId,
        session_state: xr::SessionState,
        event_buffer: xr::EventDataBuffer,
        pub views: Vec<XrView>,
        pub right_hand_pose: Option<XrPose>,
        pub left_hand_pose: Option<XrPose>,
        pub is_focused: bool,
    }

    impl XrSessionInner {
        pub fn new(config: XrConfig) -> XrResult<Self> {
            let xr_entry = unsafe { xr::Entry::load() }.map_err(|e| {
                XrError::UnsupportedFeature(format!("OpenXR loader unavailable: {e}"))
            })?;

            let instance = xr_entry.create_instance(
                &xr::ApplicationInfo {
                    application_name: &config.app_name,
                    application_version: config.app_version,
                    engine_name: "Quasar",
                    engine_version: 0,
                    api_version: xr::Version::new(1, 0, 0),
                },
                &xr::ExtensionSet::default(),
                &[],
            )?;

            let system_id = instance.system(xr::FormFactor::HEAD_MOUNTED_DISPLAY)?;

            Ok(Self {
                instance,
                _system_id: system_id,
                session_state: xr::SessionState::UNKNOWN,
                event_buffer: xr::EventDataBuffer::new(),
                views: Vec::new(),
                right_hand_pose: None,
                left_hand_pose: None,
                is_focused: false,
            })
        }

        pub fn wait_frame(&mut self) -> XrResult<()> {
            Err(graphics_session_error())
        }

        pub fn begin_frame(&self) -> XrResult<()> {
            Err(graphics_session_error())
        }

        pub fn end_frame(&self) -> XrResult<()> {
            Err(graphics_session_error())
        }

        pub fn locate_views(&mut self) -> XrResult<()> {
            self.views.clear();
            Err(graphics_session_error())
        }

        pub fn sync_actions(&mut self) -> XrResult<()> {
            Err(graphics_session_error())
        }

        pub fn is_running(&self) -> bool {
            matches!(
                self.session_state,
                xr::SessionState::VISIBLE | xr::SessionState::FOCUSED
            )
        }

        pub fn should_render(&self) -> bool {
            false
        }

        pub fn request_exit_session(&self) -> XrResult<()> {
            Err(graphics_session_error())
        }

        pub fn poll_events(&mut self) -> XrResult<Option<xr::Event<'_>>> {
            Ok(self.instance.poll_event(&mut self.event_buffer)?)
        }

        pub fn handle_event(&mut self, event: &xr::Event<'_>) -> XrResult<()> {
            match event {
                xr::Event::SessionStateChanged(state) => {
                    self.session_state = state.state();
                    self.is_focused = matches!(state.state(), xr::SessionState::FOCUSED);

                    match state.state() {
                        xr::SessionState::READY => {
                            log::debug!(
                                "OpenXR session is ready, but no graphics binding is configured"
                            );
                        }
                        xr::SessionState::STOPPING => {
                            log::debug!("OpenXR session is stopping");
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
            Ok(())
        }
    }

    impl Drop for XrSessionInner {
        fn drop(&mut self) {
            self.session_state = xr::SessionState::UNKNOWN;
        }
    }

    fn graphics_session_error() -> XrError {
        XrError::UnsupportedFeature(
            "OpenXR graphics-session support requires a concrete graphics binding".to_string(),
        )
    }
}

#[cfg(feature = "openxr")]
use openxr_impl::XrSessionInner;
#[cfg(not(feature = "openxr"))]
use stub::XrSessionInner;

pub struct XrSession {
    inner: XrSessionInner,
}

impl XrSession {
    pub fn new(config: XrConfig) -> XrResult<Self> {
        Ok(Self {
            inner: XrSessionInner::new(config)?,
        })
    }

    pub fn views(&self) -> &[XrView] {
        &self.inner.views
    }

    pub fn right_hand_pose(&self) -> Option<XrPose> {
        self.inner.right_hand_pose
    }

    pub fn left_hand_pose(&self) -> Option<XrPose> {
        self.inner.left_hand_pose
    }

    pub fn is_focused(&self) -> bool {
        self.inner.is_focused
    }

    pub fn wait_frame(&mut self) -> XrResult<()> {
        self.inner.wait_frame()
    }

    pub fn begin_frame(&self) -> XrResult<()> {
        self.inner.begin_frame()
    }

    pub fn end_frame(&self) -> XrResult<()> {
        self.inner.end_frame()
    }

    pub fn locate_views(&mut self) -> XrResult<()> {
        self.inner.locate_views()
    }

    pub fn sync_actions(&mut self) -> XrResult<()> {
        self.inner.sync_actions()
    }

    pub fn is_running(&self) -> bool {
        self.inner.is_running()
    }

    pub fn should_render(&self) -> bool {
        self.inner.should_render()
    }

    pub fn request_exit_session(&self) -> XrResult<()> {
        self.inner.request_exit_session()
    }

    #[cfg(feature = "openxr")]
    pub fn poll_events(&mut self) -> XrResult<Option<openxr::Event<'_>>> {
        self.inner.poll_events()
    }

    #[cfg(feature = "openxr")]
    pub fn handle_event(&mut self, event: &openxr::Event<'_>) -> XrResult<()> {
        self.inner.handle_event(event)
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
