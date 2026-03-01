//! Window configuration and creation.

/// Configuration for creating a window.
pub struct WindowConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub resizable: bool,
    pub vsync: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: "Quasar Engine".to_string(),
            width: 1280,
            height: 720,
            resizable: true,
            vsync: true,
        }
    }
}

/// A wrapper around the winit window with engine-specific configuration.
pub struct QuasarWindow {
    pub config: WindowConfig,
}

impl QuasarWindow {
    pub fn new(config: WindowConfig) -> Self {
        Self { config }
    }

    /// Create the actual winit window attributes from this config.
    pub fn window_attributes(&self) -> winit::window::WindowAttributes {
        winit::window::Window::default_attributes()
            .with_title(&self.config.title)
            .with_inner_size(winit::dpi::LogicalSize::new(
                self.config.width,
                self.config.height,
            ))
            .with_resizable(self.config.resizable)
    }
}
