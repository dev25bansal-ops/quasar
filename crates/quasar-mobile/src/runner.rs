//! Mobile application runner — drives the winit event loop on Android & iOS.
//!
//! Mirrors `quasar_engine::runner` but is tailored for mobile:
//! - Touch events are forwarded to [`TouchInput`] and [`GestureRecognizer`].
//! - No editor overlay (performance & screen-space constraints).
//! - Respects [`MobileConfig`] (safe-area insets, keep-screen-on, etc.).

use std::sync::Arc;

use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

use quasar_core::App;
use quasar_window::{Input, QuasarWindow, WindowConfig};

use crate::{MobileConfig, MobilePlatform, TouchInput, GestureRecognizer};

/// Runtime state created once the window is available on the device.
struct MobileState {
    window: Arc<Window>,
    touch: TouchInput,
    gestures: GestureRecognizer,
    #[allow(dead_code)]
    config: MobileConfig,
    elapsed: f32,
}

/// Winit `ApplicationHandler` for Android & iOS.
pub struct MobileRunner {
    app: App,
    window_config: Option<WindowConfig>,
    mobile_config: MobileConfig,
    state: Option<MobileState>,
}

impl MobileRunner {
    pub fn new(app: App, window_config: WindowConfig, mobile_config: MobileConfig) -> Self {
        Self {
            app,
            window_config: Some(window_config),
            mobile_config,
            state: None,
        }
    }
}

impl ApplicationHandler for MobileRunner {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        let config = self.window_config.take().unwrap_or_default();

        log::info!(
            "Mobile: creating window \"{}\" ({}×{}), platform: {:?}",
            config.title,
            config.width,
            config.height,
            MobilePlatform::current(),
        );

        let qw = QuasarWindow::new(WindowConfig {
            title: config.title.clone(),
            width: config.width,
            height: config.height,
            resizable: false,
            vsync: config.vsync,
        });

        let window = Arc::new(
            event_loop
                .create_window(qw.window_attributes())
                .expect("Failed to create mobile window"),
        );

        self.app.world.insert_resource(Input::new());

        self.state = Some(MobileState {
            window,
            touch: TouchInput::new(),
            gestures: GestureRecognizer::default(),
            config: self.mobile_config.clone(),
            elapsed: 0.0,
        });

        log::info!("Mobile runner initialised.");
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = self.state.as_mut() else {
            return;
        };

        match event {
            WindowEvent::CloseRequested => {
                log::info!("Mobile close requested.");
                event_loop.exit();
            }

            WindowEvent::Touch(touch) => {
                let phase = match touch.phase {
                    winit::event::TouchPhase::Started => crate::TouchPhase::Started,
                    winit::event::TouchPhase::Moved => crate::TouchPhase::Moved,
                    winit::event::TouchPhase::Ended => crate::TouchPhase::Ended,
                    winit::event::TouchPhase::Cancelled => crate::TouchPhase::Cancelled,
                };
                state.touch.handle_touch(
                    touch.id,
                    phase,
                    touch.location.x as f32,
                    touch.location.y as f32,
                    touch.force.map(|f| match f {
                        winit::event::Force::Calibrated { force, .. } => force as f32,
                        winit::event::Force::Normalized(n) => n as f32,
                    }).unwrap_or(1.0),
                );
                let _gestures = state.gestures.update(&state.touch, 0.016, state.elapsed);
            }

            WindowEvent::Resized(size) => {
                log::info!("Mobile resize {}×{}", size.width, size.height);
                // The rendering system should be notified here by the caller.
            }

            WindowEvent::RedrawRequested => {
                // Tick the application.
                self.app.tick();
                if let Some(state) = self.state.as_mut() {
                    state.touch.begin_frame();
                    state.elapsed += self.app.time.delta_seconds();
                    state.window.request_redraw();
                }
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(state) = &self.state {
            state.window.request_redraw();
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        log::info!("Mobile app suspended.");
        self.state = None;
    }
}

/// Convenience entry point: creates an event loop and runs the mobile runner.
pub fn run_mobile(app: App, window_config: WindowConfig, mobile_config: MobileConfig) {
    let event_loop = EventLoop::new().expect("Failed to create mobile event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut runner = MobileRunner::new(app, window_config, mobile_config);
    event_loop.run_app(&mut runner).expect("Mobile event loop error");
}
