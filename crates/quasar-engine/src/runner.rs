//! Application runner — owns the winit event loop and drives the engine.
//!
//! Instead of manually wiring `winit::ApplicationHandler`, users call
//! [`run`] with a configured [`App`] and a [`WindowConfig`]:
//!
//! ```ignore
//! use quasar_engine::prelude::*;
//!
//! let mut app = App::new();
//! app.add_plugin(PhysicsPlugin);
//! app.add_plugin(AudioPlugin);
//! run(app, WindowConfig::default());
//! ```

use std::sync::Arc;

use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

use quasar_core::asset::AssetManager;
use quasar_core::profiler::Profiler;
use quasar_core::scene::SceneGraph;
use quasar_core::{App, TimeSnapshot};
use quasar_editor::{renderer::EditorRenderer, Editor};
use quasar_render::deferred::{DeferredLightingPass, GBuffer};
use quasar_render::{
    gpu_profiler::GpuProfiler, AmbientLight, Camera, CascadeShadowMap, DirectionalLight,
    HdrRenderTarget, LightData, LightsUniform, MeshCache, MeshShape, OrbitController, PointLight,
    PostProcessPass, RenderConfig, Renderer, ShadowCamera, ShadowMap, SpotLight, TonemappingPass,
};
use quasar_ui::{UiRenderPass, UiResource};
use quasar_window::{Input, MouseButton, QuasarWindow, WindowConfig};

/// Runtime state created once the window is available.
#[allow(dead_code)]
struct RunnerState {
    window: Arc<Window>,
    renderer: Renderer,
    camera: Camera,
    orbit: OrbitController,
    editor: Editor,
    editor_renderer: EditorRenderer,
    mesh_cache: MeshCache,
    /// Default directional light for scenes without explicit lights
    default_light: DirectionalLight,
    /// Shadow map for directional light shadow casting
    shadow_map: ShadowMap,
    /// Camera used for the directional light shadow projection
    shadow_camera: ShadowCamera,
    /// Cascade shadow map for improved shadow quality
    cascade_shadow_map: CascadeShadowMap,
    /// Frame profiler for CPU timing
    profiler: Profiler,
    /// HDR render target for linear-space rendering
    hdr_target: HdrRenderTarget,
    /// Tonemapping pass (HDR → LDR)
    tonemap_pass: TonemappingPass,
    /// Post-processing pass (SSAO, FXAA, Bloom)
    post_process_pass: PostProcessPass,
    /// GPU timestamp profiler for render passes
    gpu_profiler: GpuProfiler,
    /// G-Buffer for deferred rendering (when enabled)
    gbuffer: Option<GBuffer>,
    /// Deferred lighting pass (when enabled)
    deferred_lighting: Option<DeferredLightingPass>,
    /// UI render pass for in-game UI
    ui_render_pass: UiRenderPass,
}

/// The winit `ApplicationHandler` that drives the Quasar engine loop.
struct QuasarRunner {
    /// The user-configured application (world, schedule, time, etc.).
    app: App,
    /// Window configuration — consumed on first `resumed()` call.
    config: Option<WindowConfig>,
    /// Lazily initialised once the event loop has a window.
    state: Option<RunnerState>,
}

impl QuasarRunner {
    fn new(app: App, config: WindowConfig) -> Self {
        Self {
            app,
            config: Some(config),
            state: None,
        }
    }
}

impl ApplicationHandler for QuasarRunner {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return; // Already initialised.
        }

        let config = self.config.take().unwrap_or_default();

        log::info!(
            "Creating window \"{}\" ({}×{})",
            config.title,
            config.width,
            config.height
        );

        let qw = QuasarWindow::new(WindowConfig {
            title: config.title.clone(),
            width: config.width,
            height: config.height,
            resizable: config.resizable,
            vsync: config.vsync,
            network: config.network.clone(),
        });

        let window = Arc::new(
            event_loop
                .create_window(qw.window_attributes())
                .expect("Failed to create window"),
        );

        let size = window.inner_size();
        let renderer = pollster::block_on(Renderer::new(
            window.clone(),
            size.width,
            size.height,
            RenderConfig::default(),
        ))
        .expect("Failed to initialise GPU renderer");

        let camera = Camera::new(size.width, size.height);

        let editor = Editor::new();
        let editor_renderer =
            EditorRenderer::new(&window, &renderer.device, renderer.config.format);

        // Insert engine resources into the world.
        self.app.world.insert_resource(Input::new());
        self.app.world.insert_resource(AssetManager::new());
        self.app.world.insert_resource(SceneGraph::new());

        // Register NetworkPlugin if network configuration is provided.
        if let Some(network_config) = config.network.clone() {
            log::info!(
                "Registering NetworkPlugin in {:?} mode on port {}",
                network_config.role,
                network_config.port
            );
            self.app
                .add_plugin(quasar_core::NetworkPlugin::new(network_config));
        }

        // Register UiPlugin for in-game UI
        self.app.add_plugin(quasar_ui::UiPlugin);

        let shadow_map = ShadowMap::new(&renderer.device, 2048);
        let shadow_camera = ShadowCamera::default();
        let cascade_shadow_map = CascadeShadowMap::new(&renderer.device);

        let hdr_target = HdrRenderTarget::new(&renderer.device, size.width, size.height);
        let tonemap_pass = TonemappingPass::new(&renderer.device, renderer.config.format);
        let post_process_pass = PostProcessPass::new(
            &renderer.device,
            size.width,
            size.height,
            renderer.config.format,
        );
        let gpu_profiler =
            GpuProfiler::new(&renderer.device, renderer.queue.get_timestamp_period());

        // Create G-Buffer and deferred lighting pass if enabled
        let (gbuffer, deferred_lighting) = if renderer.render_config.deferred_enabled {
            let gbuffer = GBuffer::new(&renderer.device, size.width, size.height);
            let deferred = DeferredLightingPass::new(
                &renderer.device,
                renderer.config.format,
                &gbuffer.read_bind_group_layout,
            );
            (Some(gbuffer), Some(deferred))
        } else {
            (None, None)
        };

        // Create UI render pass for in-game UI
        let ui_render_pass = UiRenderPass::new(&renderer.device, renderer.config.format);

        self.state = Some(RunnerState {
            window,
            renderer,
            camera,
            orbit: OrbitController::new(5.0),
            editor,
            editor_renderer,
            mesh_cache: MeshCache::new(),
            default_light: DirectionalLight::default(),
            shadow_map,
            shadow_camera,
            cascade_shadow_map,
            profiler: Profiler::new(),
            hdr_target,
            tonemap_pass,
            post_process_pass,
            gpu_profiler,
            gbuffer,
            deferred_lighting,
            ui_render_pass,
        });

        log::info!(
            "Engine initialised — rendering at {}×{}",
            size.width,
            size.height
        );
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

        // ── Let egui have first crack at the event ────────────────
        let egui_consumed = state.editor_renderer.handle_event(&state.window, &event);

        match event {
            // ── Close ─────────────────────────────────────────────
            WindowEvent::CloseRequested => {
                log::info!("Close requested — shutting down.");
                event_loop.exit();
            }

            // ── Keyboard ──────────────────────────────────────────
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(key) = event.physical_key {
                    // F12 toggles the editor regardless.
                    if key == KeyCode::F12 && event.state.is_pressed() {
                        state.editor.toggle();
                    }

                    // Only forward to game input if egui didn't consume.
                    if !egui_consumed {
                        if let Some(input) = self.app.world.resource_mut::<Input>() {
                            if event.state.is_pressed() {
                                input.key_pressed(key);
                            } else {
                                input.key_released(key);
                            }
                        }
                        // ESC closes the application.
                        if key == KeyCode::Escape && event.state.is_pressed() {
                            event_loop.exit();
                        }
                    }
                }
            }

            // ── Mouse movement ────────────────────────────────────
            WindowEvent::CursorMoved { position, .. } => {
                if !egui_consumed {
                    if let Some(input) = self.app.world.resource_mut::<Input>() {
                        // Compute per-frame delta from absolute position.
                        if let Some(prev) = input.cursor_position {
                            input.mouse_moved(position.x - prev.0, position.y - prev.1);
                        }
                        input.cursor_position = Some((position.x, position.y));
                    }
                }
            }

            // ── Mouse buttons ─────────────────────────────────────
            WindowEvent::MouseInput {
                state: btn_state,
                button,
                ..
            } => {
                if !egui_consumed {
                    if let Some(input) = self.app.world.resource_mut::<Input>() {
                        let btn = quasar_window::MouseButton::from(button);
                        if btn_state.is_pressed() {
                            input.mouse_button_pressed(btn);
                        } else {
                            input.mouse_button_released(btn);
                        }
                    }
                }
            }

            // ── Mouse scroll ──────────────────────────────────────
            WindowEvent::MouseWheel { delta, .. } => {
                if !egui_consumed {
                    if let Some(input) = self.app.world.resource_mut::<Input>() {
                        match delta {
                            winit::event::MouseScrollDelta::LineDelta(x, y) => {
                                input.mouse_scrolled(x, y);
                            }
                            winit::event::MouseScrollDelta::PixelDelta(pos) => {
                                input.mouse_scrolled(pos.x as f32, pos.y as f32);
                            }
                        }
                    }
                }
            }

            // ── Resize ────────────────────────────────────────────
            WindowEvent::Resized(new_size) => {
                if new_size.width > 0 && new_size.height > 0 {
                    state.renderer.resize(new_size.width, new_size.height);
                    state.camera.set_aspect(new_size.width, new_size.height);
                    state.hdr_target.resize(
                        &state.renderer.device,
                        new_size.width,
                        new_size.height,
                    );
                }
            }

            // ── Redraw (main frame tick) ──────────────────────────
            WindowEvent::RedrawRequested => {
                // ── Profiler: start of frame ─────────────────────
                state.profiler.begin_frame();

                // Clear per-frame input state.
                if let Some(input) = self.app.world.resource_mut::<Input>() {
                    input.begin_frame();
                }

                // Insert simulation state so physics/audio/scripting systems
                // know whether to run this frame.
                self.app
                    .world
                    .insert_resource(quasar_core::SimulationState {
                        should_tick: state.editor.state.should_tick(),
                    });

                // Run one full ECS frame (updates time, runs all systems).
                state.profiler.begin_scope("ecs_tick");
                self.app.tick();
                state.profiler.end_scope("ecs_tick");

                // Periodically check for asset changes (every ~1 second when playing)
                if state.editor.state.should_tick() {
                    let frame_count = self
                        .app
                        .world
                        .resource::<TimeSnapshot>()
                        .map(|t| t.frame_count)
                        .unwrap_or(0);
                    if frame_count.is_multiple_of(60) {
                        state.editor.check_asset_changes();
                    }
                }

                // Upload instance transforms collected by RenderSyncSystem.
                if let Some(sync) = self
                    .app
                    .world
                    .remove_resource::<quasar_render::RenderSyncOutput>()
                {
                    state
                        .renderer
                        .upload_instance_transforms(&sync.instance_transforms);
                }

                // Propagate scene-graph transforms (if user built a hierarchy).
                if let Some(scene) = self.app.world.remove_resource::<SceneGraph>() {
                    scene.propagate_transforms(&mut self.app.world);
                    self.app.world.insert_resource(scene);
                }

                // Drive orbit controller from input.
                {
                    let (mouse_dx, mouse_dy, scroll_y, rmb_held) =
                        if let Some(input) = self.app.world.resource::<Input>() {
                            (
                                input.mouse_delta.0 as f32,
                                input.mouse_delta.1 as f32,
                                input.scroll_delta.1,
                                input.is_mouse_pressed(MouseButton::Right),
                            )
                        } else {
                            (0.0, 0.0, 0.0, false)
                        };

                    if rmb_held {
                        state.orbit.rotate(mouse_dx, mouse_dy);
                    }
                    if scroll_y.abs() > 0.001 {
                        state.orbit.zoom(-scroll_y);
                    }
                    state.orbit.apply(&mut state.camera);
                }

                // Collect meshes + model matrices from the world.
                // Entities with a MeshShape component get that shape;
                // entities with only a Transform default to Cube.
                let shape_mats: Vec<(MeshShape, glam::Mat4)> = {
                    use quasar_math::Transform;

                    // Entities with both MeshShape + Transform
                    let with_shape: Vec<(MeshShape, glam::Mat4)> = self
                        .app
                        .world
                        .query2::<MeshShape, Transform>()
                        .into_iter()
                        .map(|(_, shape, t)| (*shape, t.matrix()))
                        .collect();

                    if !with_shape.is_empty() {
                        with_shape
                    } else {
                        // Fallback: render all Transform entities as cubes
                        self.app
                            .world
                            .query::<Transform>()
                            .into_iter()
                            .map(|(_, t)| (MeshShape::Cube, t.matrix()))
                            .collect()
                    }
                };

                // Ensure all needed meshes are uploaded (cached).
                for (shape, _) in &shape_mats {
                    state
                        .mesh_cache
                        .get_or_create(&state.renderer.device, *shape);
                }

                // Collect lights from the world
                let lights_uniform = {
                    let mut lights = LightsUniform::default();
                    let mut light_count = 0usize;

                    // Collect directional lights
                    for (_, light) in self.app.world.query::<DirectionalLight>() {
                        if light_count < quasar_render::MAX_LIGHTS {
                            lights.lights[light_count] = LightData::from_directional(light);
                            light_count += 1;
                        }
                    }

                    // Collect point lights
                    for (_, light) in self.app.world.query::<PointLight>() {
                        if light_count < quasar_render::MAX_LIGHTS {
                            lights.lights[light_count] = LightData::from_point(light);
                            light_count += 1;
                        }
                    }

                    // Collect spot lights
                    for (_, light) in self.app.world.query::<SpotLight>() {
                        if light_count < quasar_render::MAX_LIGHTS {
                            lights.lights[light_count] = LightData::from_spot(light);
                            light_count += 1;
                        }
                    }

                    // If no lights in scene, use default directional light
                    if light_count == 0 {
                        lights.lights[0] = LightData::from_directional(&state.default_light);
                        light_count = 1;
                    }

                    lights.count = light_count as u32;

                    // Collect ambient light
                    let ambient_iter = self.app.world.query::<AmbientLight>();
                    if let Some((_, ambient)) = ambient_iter.into_iter().next() {
                        lights.ambient = [
                            ambient.color.x,
                            ambient.color.y,
                            ambient.color.z,
                            ambient.intensity,
                        ];
                    }

                    lights
                };

                // Update light uniform buffer
                state.renderer.queue.write_buffer(
                    &state.renderer.light_buffer,
                    0,
                    bytemuck::cast_slice(&[lights_uniform]),
                );

                // ── Shadow pass (before main frame) ──────────────
                state.profiler.begin_scope("shadow_pass");
                state.gpu_profiler.begin_frame();
                {
                    // Position the shadow camera along the primary directional light
                    let dir = lights_uniform.lights[0].direction;
                    let light_dir = glam::Vec3::new(dir[0], dir[1], dir[2]).normalize_or_zero();
                    if light_dir != glam::Vec3::ZERO {
                        state.shadow_camera.position =
                            state.shadow_camera.target - light_dir * 30.0;
                    }
                    let light_vp = state.shadow_camera.view_projection();

                    let shadow_objects: Vec<(&quasar_render::Mesh, glam::Mat4)> = shape_mats
                        .iter()
                        .filter_map(|(shape, model)| {
                            state
                                .mesh_cache
                                .cache
                                .get(shape)
                                .map(|m| (m as &quasar_render::Mesh, *model))
                        })
                        .collect();

                    let shadow_cmd = state.shadow_map.render_shadow_pass(
                        &state.renderer.device,
                        &state.renderer.queue,
                        light_vp,
                        &shadow_objects,
                    );
                    state.renderer.queue.submit(std::iter::once(shadow_cmd));

                    // Update renderer's lighting bind group to use the real shadow map.
                    state.renderer.update_shadow_bindings(
                        &state.shadow_map.view,
                        &state.shadow_map.shadow_uniform_buffer,
                        &state.shadow_map.sampler,
                        &state.shadow_map.depth_sampler,
                    );

                    // ── Cascade Shadow Map rendering ──────────────────
                    // Update cascade transforms based on camera and light direction
                    state.cascade_shadow_map.update_cascades(
                        state.camera.view_projection(),
                        state.camera.position,
                        light_dir,
                        state.camera.near,
                        state.camera.far,
                    );

                    // Upload cascade data to the cascade buffer
                    let cascade_uniforms: Vec<quasar_render::CascadeUniform> = state
                        .cascade_shadow_map
                        .cascades
                        .iter()
                        .map(|c| quasar_render::CascadeUniform {
                            view_proj: c.view_projection.to_cols_array_2d(),
                            split_depth: c.split_depth,
                            _pad: [0.0; 3],
                        })
                        .collect();
                    state.renderer.queue.write_buffer(
                        &state.cascade_shadow_map.cascade_buffer,
                        0,
                        bytemuck::cast_slice(&cascade_uniforms),
                    );

                    // Render each cascade
                    for i in 0..quasar_render::CASCADE_COUNT {
                        let cascade_cmd = state.cascade_shadow_map.render_cascade(
                            &state.renderer.device,
                            &state.renderer.queue,
                            i,
                            &shadow_objects,
                        );
                        state.renderer.queue.submit(std::iter::once(cascade_cmd));
                    }

                    // Update lighting bind group with CSM resources
                    let cascade_array_view = state.cascade_shadow_map.texture.create_view(
                        &wgpu::TextureViewDescriptor {
                            label: Some("Cascade Shadow Array View"),
                            dimension: Some(wgpu::TextureViewDimension::D2Array),
                            ..Default::default()
                        },
                    );
                    state.renderer.update_csm_bindings(
                        &state.cascade_shadow_map.cascade_buffer,
                        &cascade_array_view,
                        &state.cascade_shadow_map.sampler,
                        &state.shadow_map.view,
                        &state.shadow_map.shadow_uniform_buffer,
                        &state.shadow_map.sampler,
                        &state.shadow_map.depth_sampler,
                    );
                }
                state.profiler.end_scope("shadow_pass");

                // ── Split-phase rendering: 3D → egui → present ───
                state.profiler.begin_scope("render");
                let frame_result = state.renderer.begin_frame();
                match frame_result {
                    Ok((output, view, mut encoder)) => {
                        // Build references for the 3D pass (after begin_frame
                        // to keep borrow lifetimes local).
                        let objects: Vec<(
                            &quasar_render::Mesh,
                            glam::Mat4,
                            Option<&wgpu::BindGroup>,
                            Option<u32>,
                        )> = shape_mats
                            .iter()
                            .filter_map(|(shape, model)| {
                                state
                                    .mesh_cache
                                    .cache
                                    .get(shape)
                                    .map(|m| (m, *model, None, None))
                            })
                            .collect();

                        // Apply TAA jitter to camera projection (if TAA enabled).
                        if let Some(taa) = state.renderer.taa_pass.as_ref() {
                            state.camera.jitter = taa.jitter_offset();
                        }

                        // 3D pass — render to HDR target (linear space, no tonemapping).
                        // TODO: When deferred_enabled, render to G-Buffer then run deferred lighting pass.
                        // This requires a deferred geometry shader that outputs albedo, normal, and PBR params.
                        let gpu_3d = state.gpu_profiler.begin_pass(&mut encoder, "3d_pass");
                        state.renderer.render_3d_pass_batched(
                            &state.camera,
                            &objects,
                            &state.hdr_target.view,
                            &mut encoder,
                        );
                        if let Some(idx) = gpu_3d {
                            state.gpu_profiler.end_pass(&mut encoder, idx);
                        }

                        // Clear jitter so non-rendering code sees unjittered projection.
                        state.camera.jitter = (0.0, 0.0);

                        // SSGI compute — trace indirect diffuse from colour + depth.
                        if let Some(ssgi) = state.renderer.ssgi_pass.as_mut() {
                            let gpu_ssgi = state.gpu_profiler.begin_pass(&mut encoder, "ssgi");
                            let inv_proj = state.camera.projection_matrix().inverse();
                            ssgi.dispatch(
                                &state.renderer.device,
                                &state.renderer.queue,
                                &mut encoder,
                                &state.hdr_target.view,
                                &state.renderer.depth_view,
                                &inv_proj,
                                state.renderer.motion_vector_view.as_ref(),
                            );
                            if let Some(idx) = gpu_ssgi {
                                state.gpu_profiler.end_pass(&mut encoder, idx);
                            }

                            // Composite SSGI output into HDR target
                            let gpu_ssgi_composite = state
                                .gpu_profiler
                                .begin_pass(&mut encoder, "ssgi_composite");
                            ssgi.composite(
                                &state.renderer.device,
                                &mut encoder,
                                &state.hdr_target.view,
                            );
                            if let Some(idx) = gpu_ssgi_composite {
                                state.gpu_profiler.end_pass(&mut encoder, idx);
                            }
                        }

                        // SSAO pass — screen-space ambient occlusion with bilateral blur.
                        // Update depth binding to use the real scene depth buffer.
                        let gpu_ssao = state.gpu_profiler.begin_pass(&mut encoder, "ssao");
                        state.post_process_pass.update_depth_texture(
                            &state.renderer.device,
                            &state.renderer.depth_view,
                        );
                        state.post_process_pass.render_ssao_with_blur(&mut encoder);
                        if let Some(idx) = gpu_ssao {
                            state.gpu_profiler.end_pass(&mut encoder, idx);
                        }

                        // TAA resolve → tonemapping. Without TAA the HDR target
                        // feeds directly into the tonemapping pass.
                        let tonemap_source: &wgpu::TextureView;
                        if let Some(taa) = state.renderer.taa_pass.as_mut() {
                            let gpu_taa = state.gpu_profiler.begin_pass(&mut encoder, "taa");
                            let mv_view = state.renderer.motion_vector_view.as_ref().unwrap();
                            taa.resolve(
                                &state.renderer.device,
                                &state.renderer.queue,
                                &mut encoder,
                                &state.hdr_target.view,
                                mv_view,
                            );
                            if let Some(idx) = gpu_taa {
                                state.gpu_profiler.end_pass(&mut encoder, idx);
                            }
                            tonemap_source = taa.output_view();
                        } else {
                            tonemap_source = &state.hdr_target.view;
                        }

                        // Tonemapping pass — HDR → LDR on the surface view.
                        let gpu_tonemap = state.gpu_profiler.begin_pass(&mut encoder, "tonemap");
                        state
                            .tonemap_pass
                            .update_texture(&state.renderer.device, tonemap_source);
                        state.tonemap_pass.execute(&mut encoder, &view);
                        if let Some(idx) = gpu_tonemap {
                            state.gpu_profiler.end_pass(&mut encoder, idx);
                        }

                        // In-game UI pass (rendered after 3D/tonemapping, before editor)
                        if let Some(ui_resource) = self.app.world.resource::<UiResource>() {
                            let gpu_ui = state.gpu_profiler.begin_pass(&mut encoder, "ui");
                            let size = state.window.inner_size();
                            {
                                let mut ui_pass =
                                    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                        label: Some("UI Pass"),
                                        color_attachments: &[Some(
                                            wgpu::RenderPassColorAttachment {
                                                view: &view,
                                                resolve_target: None,
                                                ops: wgpu::Operations {
                                                    load: wgpu::LoadOp::Load,
                                                    store: wgpu::StoreOp::Store,
                                                },
                                            },
                                        )],
                                        depth_stencil_attachment: None,
                                        timestamp_writes: None,
                                        occlusion_query_set: None,
                                    });
                                state.ui_render_pass.draw(
                                    &state.renderer.queue,
                                    &mut ui_pass,
                                    &ui_resource.tree,
                                    &ui_resource.layout,
                                    size.width as f32,
                                    size.height as f32,
                                );
                            }
                            if let Some(idx) = gpu_ui {
                                state.gpu_profiler.end_pass(&mut encoder, idx);
                            }
                        }

                        // egui pass (editor overlay).
                        let egui_commands = if state.editor.enabled {
                            // Build entity name list for the hierarchy panel.
                            // Use SceneGraph names when available, fall back to generic.
                            let entity_names: Vec<(quasar_core::ecs::Entity, String)> = {
                                let scene_opt = self.app.world.remove_resource::<SceneGraph>();
                                let names: Vec<_> = self
                                    .app
                                    .world
                                    .query::<quasar_math::Transform>()
                                    .into_iter()
                                    .map(|(e, _)| {
                                        let name = scene_opt
                                            .as_ref()
                                            .and_then(|s| s.name(e))
                                            .map(|s| s.to_string())
                                            .unwrap_or_else(|| format!("Entity {}", e.index()));
                                        (e, name)
                                    })
                                    .collect();
                                if let Some(scene) = scene_opt {
                                    self.app.world.insert_resource(scene);
                                }
                                names
                            };

                            state.editor_renderer.begin_frame(&state.window);

                            // Build inspector data for the first selected entity.
                            let inspector_data =
                                state.editor.selected_entities.first().and_then(|&e| {
                                    let transform =
                                        self.app.world.get::<quasar_math::Transform>(e)?;
                                    let material = self
                                        .app
                                        .world
                                        .get::<quasar_render::MaterialOverride>(e)
                                        .copied();
                                    Some(quasar_editor::InspectorData {
                                        transform: *transform,
                                        material,
                                    })
                                });

                            // Run editor UI with command pattern
                            let (edit_commands, editor_action) = state.editor.ui(
                                &state.editor_renderer.egui_ctx,
                                &entity_names,
                                inspector_data,
                            );

                            // Execute inspector edit commands (undo/redo-capable)
                            for command in edit_commands {
                                state
                                    .editor
                                    .state
                                    .execute_command(command, &mut self.app.world);
                            }

                            // Update logic graph system when playing
                            if state.editor.state.should_tick() {
                                let lg_commands =
                                    state.editor.update_logic_graph(&mut self.app.world, 0.016);
                                for command in lg_commands {
                                    state
                                        .editor
                                        .state
                                        .execute_command(command, &mut self.app.world);
                                }

                                // Apply physics from logic graph
                                state.editor.apply_logic_graph_physics(&mut self.app.world);

                                // Play audio from logic graph
                                state.editor.play_logic_graph_audio(&mut self.app.world);
                            }

                            // Handle editor actions (Play/Pause/Stop/Undo/Redo)
                            if let Some(action) = editor_action {
                                state.editor.handle_action(action, &mut self.app.world);
                            }

                            let size = state.window.inner_size();
                            let screen = egui_wgpu::ScreenDescriptor {
                                size_in_pixels: [size.width, size.height],
                                pixels_per_point: state.window.scale_factor() as f32,
                            };

                            Some(state.editor_renderer.end_frame_and_render(
                                &state.renderer.device,
                                &state.renderer.queue,
                                &view,
                                screen,
                                &state.window,
                            ))
                        } else {
                            None
                        };

                        // Resolve GPU profiler timestamps before submit.
                        state.gpu_profiler.resolve(&mut encoder);

                        // Submit 3D + egui command buffers, then present.
                        let mut buffers = vec![encoder.finish()];
                        if let Some(egui_buf) = egui_commands {
                            buffers.push(egui_buf);
                        }
                        state.renderer.queue.submit(buffers);

                        // Collect GPU profiler results and feed to editor.
                        state.gpu_profiler.request_results();
                        if let Some(timings) =
                            state.gpu_profiler.try_collect(&state.renderer.device)
                        {
                            state.editor.gpu_pass_timings = timings.to_vec();
                        }

                        output.present();
                    }
                    Err(wgpu::SurfaceError::Lost) => {
                        let size = state.window.inner_size();
                        state.renderer.resize(size.width, size.height);
                    }
                    Err(wgpu::SurfaceError::OutOfMemory) => {
                        log::error!("GPU out of memory!");
                        event_loop.exit();
                    }
                    Err(e) => log::warn!("Render error: {e:?}"),
                }
                state.profiler.end_scope("render");

                // ── Profiler: end of frame ───────────────────────
                state.profiler.end_frame();

                state.window.request_redraw();
            }

            _ => {}
        }
    }
}

/// Launch the engine: create a window, initialise the renderer, and run the
/// ECS game loop until the window is closed.
///
/// This is the main entry point for Quasar applications. Configure your
/// [`App`] with plugins and systems, then call `run`:
///
/// ```ignore
/// use quasar_engine::prelude::*;
///
/// let mut app = App::new();
/// app.add_plugin(PhysicsPlugin);
/// run(app, WindowConfig::default());
/// ```
pub fn run(app: App, config: WindowConfig) {
    let event_loop = EventLoop::new().expect("Failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut runner = QuasarRunner::new(app, config);
    event_loop.run_app(&mut runner).expect("Event loop error");
}

// ---------------------------------------------------------------------------
// WASM async runner — requestAnimationFrame loop
// ---------------------------------------------------------------------------

#[cfg(target_arch = "wasm32")]
pub mod wasm_runner {
    use std::cell::RefCell;
    use std::rc::Rc;

    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;

    use quasar_core::App;

    /// State shared between animation‐frame callbacks.
    struct WasmAppState {
        app: App,
        /// Whether the loop should keep running.
        running: bool,
    }

    /// Kick off the engine's main loop using `requestAnimationFrame`.
    ///
    /// This is intended to be called from the WASM entry point instead of
    /// `run()` which blocks on a native winit event loop.
    pub fn run_wasm(app: App) {
        let state = Rc::new(RefCell::new(WasmAppState { app, running: true }));

        schedule_frame(state);
    }

    fn schedule_frame(state: Rc<RefCell<WasmAppState>>) {
        let window = web_sys::window().expect("no global window");
        let cb = Rc::new(RefCell::new(None::<Closure<dyn FnMut(f64)>>));
        let cb_clone = cb.clone();
        let state_clone = state.clone();

        *cb.borrow_mut() = Some(Closure::wrap(Box::new(move |_timestamp: f64| {
            {
                let mut st = state_clone.borrow_mut();
                if !st.running {
                    // Drop the closure to break the reference cycle.
                    let _ = cb_clone.borrow_mut().take();
                    return;
                }
                st.app.tick();
            }
            // Schedule the next frame.
            if let Some(ref closure) = *cb_clone.borrow() {
                let _ = web_sys::window()
                    .unwrap()
                    .request_animation_frame(closure.as_ref().unchecked_ref());
            }
        }) as Box<dyn FnMut(f64)>));

        if let Some(ref closure) = *cb.borrow() {
            let _ = window.request_animation_frame(closure.as_ref().unchecked_ref());
        }
    }
}
