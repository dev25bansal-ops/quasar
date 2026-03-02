//! GPU integration for the editor overlay — wires `egui` into `wgpu`.
//!
//! Wraps [`egui_winit::State`] for input handling and [`egui_wgpu::Renderer`]
//! for painting the egui output into a wgpu render pass.

use egui::ViewportId;
use winit::window::Window;

/// Manages the egui render state: input translation and GPU painting.
pub struct EditorRenderer {
    /// egui ↔ winit input state.
    pub egui_state: egui_winit::State,
    /// egui ↔ wgpu renderer.
    pub egui_renderer: egui_wgpu::Renderer,
    /// The egui context (shared with Editor::ui).
    pub egui_ctx: egui::Context,
}

impl EditorRenderer {
    /// Create a new editor renderer for the given window and GPU device.
    pub fn new(
        window: &Window,
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
    ) -> Self {
        let egui_ctx = egui::Context::default();
        let egui_state =
            egui_winit::State::new(egui_ctx.clone(), ViewportId::ROOT, window, None, None, None);
        let egui_renderer = egui_wgpu::Renderer::new(device, surface_format, None, 1, false);

        Self {
            egui_state,
            egui_renderer,
            egui_ctx,
        }
    }

    /// Forward a winit window event to egui. Returns `true` if egui consumed it.
    pub fn handle_event(&mut self, window: &Window, event: &winit::event::WindowEvent) -> bool {
        let response = self.egui_state.on_window_event(window, event);
        response.consumed
    }

    /// Begin an egui frame. Call before building UI.
    pub fn begin_frame(&mut self, window: &Window) {
        let raw_input = self.egui_state.take_egui_input(window);
        self.egui_ctx.begin_pass(raw_input);
    }

    /// End the egui frame and render into the given surface view.
    ///
    /// Creates its own command encoder and returns the finished command buffer
    /// so the caller can submit it alongside the 3D scene commands.
    pub fn end_frame_and_render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_view: &wgpu::TextureView,
        screen_descriptor: egui_wgpu::ScreenDescriptor,
        window: &Window,
    ) -> wgpu::CommandBuffer {
        let full_output = self.egui_ctx.end_pass();

        // Handle platform output (cursor, clipboard, etc.)
        self.egui_state
            .handle_platform_output(window, full_output.platform_output.clone());

        let paint_jobs = self
            .egui_ctx
            .tessellate(full_output.shapes, self.egui_ctx.pixels_per_point());

        // Upload textures.
        for (id, image_delta) in &full_output.textures_delta.set {
            self.egui_renderer
                .update_texture(device, queue, *id, image_delta);
        }

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("egui Encoder"),
        });

        self.egui_renderer.update_buffers(
            device,
            queue,
            &mut encoder,
            &paint_jobs,
            &screen_descriptor,
        );

        // Render into a pass on top of the 3D scene.
        // `forget_lifetime` converts `RenderPass<'encoder>` → `RenderPass<'static>`
        // which is what `egui_wgpu::Renderer::render` requires.
        let mut render_pass = encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: surface_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // preserve 3D scene underneath
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            })
            .forget_lifetime();

        self.egui_renderer
            .render(&mut render_pass, &paint_jobs, &screen_descriptor);

        drop(render_pass);

        // Free old textures.
        for id in &full_output.textures_delta.free {
            self.egui_renderer.free_texture(id);
        }

        encoder.finish()
    }
}
