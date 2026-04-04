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
    use glam::{Quat, Vec3};

    pub struct XrSessionInner {
        instance: xr::Instance,
        system_id: xr::SystemId,
        session: xr::Session<xr::AnyGraphics>,
        session_state: xr::SessionState,
        action_set: xr::ActionSet,
        right_hand_action: xr::Action<xr::Posef>,
        left_hand_action: xr::Action<xr::Posef>,
        space: xr::Space,
        view_space: xr::Space,
        frame_state: Option<xr::FrameState>,
        pub views: Vec<XrView>,
        pub right_hand_pose: Option<XrPose>,
        pub left_hand_pose: Option<XrPose>,
        pub is_focused: bool,
    }

    impl XrSessionInner {
        pub fn new(config: XrConfig) -> XrResult<Self> {
            let xr_entry = xr::Entry::linked();

            let instance = xr_entry.create_instance(
                &xr::ApplicationInfo {
                    application_name: &config.app_name,
                    application_version: config.app_version,
                    engine_name: "Quasar",
                    engine_version: 0,
                },
                &[xr::ExtensionSet {
                    khr_opengl_enable: true,
                    ext_hand_tracking: true,
                    ..Default::default()
                }],
                &[],
            )?;

            let system_id = instance.system(xr::FormFactor::HEAD_MOUNTED_DISPLAY)?;

            let session = instance
                .create_session::<xr::AnyGraphics>(system_id, &xr::GraphicsBinding::AnyGraphics)?;

            let action_set = instance.create_action_set("input", "Input Action Set", 0)?;

            let right_hand_action =
                action_set.create_action::<xr::Posef>("right_hand", "Right Hand Pose", &[])?;

            let left_hand_action =
                action_set.create_action::<xr::Posef>("left_hand", "Left Hand Pose", &[])?;

            let space = session
                .create_reference_space(xr::ReferenceSpaceType::STAGE, xr::Posef::IDENTITY)?;
            let view_space = session
                .create_reference_space(xr::ReferenceSpaceType::VIEW, xr::Posef::IDENTITY)?;

            let profile = instance.string_to_path("/interaction_profiles/khr/simple_controller")?;
            let aim_path = instance.string_to_path("/user/hand/right/input/aim/pose")?;

            session.suggest_interaction_profile_bindings(
                profile,
                &[xr::ActionSuggestedBinding {
                    action: &right_hand_action,
                    binding: aim_path,
                }],
            )?;

            session.attach_action_sets(&[&action_set])?;

            Ok(Self {
                instance,
                system_id,
                session,
                session_state: xr::SessionState::UNKNOWN,
                action_set,
                right_hand_action,
                left_hand_action,
                space,
                view_space,
                frame_state: None,
                views: Vec::new(),
                right_hand_pose: None,
                left_hand_pose: None,
                is_focused: false,
            })
        }

        pub fn wait_frame(&mut self) -> XrResult<()> {
            self.frame_state = Some(self.session.wait_frame(None)?);
            Ok(())
        }

        pub fn begin_frame(&self) -> XrResult<()> {
            self.session.begin_frame()?;
            Ok(())
        }

        pub fn end_frame(&self) -> XrResult<()> {
            self.session.end_frame(None)?;
            Ok(())
        }

        pub fn locate_views(&mut self) -> XrResult<()> {
            let frame_state = self.frame_state.as_ref().ok_or(XrError::NotInitialized)?;

            if !frame_state.should_render {
                self.views.clear();
                return Ok(());
            }

            let (_, views) = self.session.locate_views(
                xr::ViewConfigurationType::PRIMARY_STEREO,
                frame_state.predicted_display_time,
                &self.space,
            )?;

            self.views = views
                .iter()
                .map(|v| XrView {
                    pose: XrPose {
                        position: Vec3::new(
                            v.pose.position.x,
                            v.pose.position.y,
                            v.pose.position.z,
                        ),
                        orientation: Quat::from_xyzw(
                            v.pose.orientation.x,
                            v.pose.orientation.y,
                            v.pose.orientation.z,
                            v.pose.orientation.w,
                        ),
                    },
                    fov: XrFov {
                        angle_left: v.fov.angle_left,
                        angle_right: v.fov.angle_right,
                        angle_up: v.fov.angle_up,
                        angle_down: v.fov.angle_down,
                    },
                })
                .collect();

            Ok(())
        }

        pub fn sync_actions(&mut self) -> XrResult<()> {
            let frame_state = self.frame_state.as_ref().ok_or(XrError::NotInitialized)?;

            self.session.sync_actions(&[&self.action_set])?;

            let right_pose = self
                .right_hand_action
                .locate(
                    &self.session,
                    frame_state.predicted_display_time,
                    &self.space,
                )
                .ok();

            let left_pose = self
                .left_hand_action
                .locate(
                    &self.session,
                    frame_state.predicted_display_time,
                    &self.space,
                )
                .ok();

            self.right_hand_pose = right_pose.map(|p| XrPose {
                position: Vec3::new(p.pose.position.x, p.pose.position.y, p.pose.position.z),
                orientation: Quat::from_xyzw(
                    p.pose.orientation.x,
                    p.pose.orientation.y,
                    p.pose.orientation.z,
                    p.pose.orientation.w,
                ),
            });

            self.left_hand_pose = left_pose.map(|p| XrPose {
                position: Vec3::new(p.pose.position.x, p.pose.position.y, p.pose.position.z),
                orientation: Quat::from_xyzw(
                    p.pose.orientation.x,
                    p.pose.orientation.y,
                    p.pose.orientation.z,
                    p.pose.orientation.w,
                ),
            });

            Ok(())
        }

        pub fn is_running(&self) -> bool {
            matches!(
                self.session_state,
                xr::SessionState::VISIBLE | xr::SessionState::FOCUSED
            )
        }

        pub fn should_render(&self) -> bool {
            self.frame_state
                .as_ref()
                .map(|f| f.should_render)
                .unwrap_or(false)
        }

        pub fn request_exit_session(&self) -> XrResult<()> {
            self.session.request_exit_session()?;
            Ok(())
        }

        pub fn poll_events(&mut self) -> XrResult<Option<xr::Event>> {
            let mut event_buffer = xr::EventBuffer::new();
            self.instance.poll_event(&mut event_buffer)?;
            Ok(event_buffer.get())
        }

        pub fn handle_event(&mut self, event: &xr::Event) -> XrResult<()> {
            match event {
                xr::Event::SessionStateChanged(state) => {
                    self.session_state = state.state();
                    self.is_focused = matches!(state.state(), xr::SessionState::FOCUSED);

                    match state.state() {
                        xr::SessionState::READY => {
                            self.session
                                .begin_session(xr::ViewConfigurationType::PRIMARY_STEREO)?;
                        }
                        xr::SessionState::STOPPING => {
                            self.session.end_session()?;
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
            if matches!(
                self.session_state,
                xr::SessionState::VISIBLE | xr::SessionState::FOCUSED
            ) {
                let _ = self.session.end_session();
            }
        }
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
    pub fn poll_events(&mut self) -> XrResult<Option<xr::Event>> {
        self.inner.poll_events()
    }

    #[cfg(feature = "openxr")]
    pub fn handle_event(&mut self, event: &xr::Event) -> XrResult<()> {
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
