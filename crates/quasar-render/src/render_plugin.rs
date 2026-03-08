//! Render plugin — integrates rendering systems into the ECS.

use quasar_core::ecs::System;
use quasar_core::ecs::World;
use quasar_core::AssetServer;

use crate::mesh::MeshShape;
use crate::ParticleEmitter;
use crate::render_graph::{
    Attachment, RenderContext, RenderGraph, RenderPass,
    attachment_ids, pass_ids,
};

/// Staging resource written by [`RenderSyncSystem`] each frame.
///
/// The runner reads this after `tick()` and uploads the matrices to the GPU
/// instance buffer — keeping the `Renderer` out of the ECS world.
pub struct RenderSyncOutput {
    pub instance_transforms: Vec<glam::Mat4>,
}

/// System that syncs transforms to GPU buffers and updates render state.
pub struct RenderSyncSystem;

impl System for RenderSyncSystem {
    fn name(&self) -> &str {
        "render_sync"
    }

    fn run(&mut self, world: &mut World) {
        use quasar_math::Transform;

        // Collect model matrices from all entities with MeshShape + Transform.
        let pairs = world.query2::<MeshShape, Transform>();
        let transforms: Vec<glam::Mat4> = pairs
            .iter()
            .map(|(_, _shape, t)| t.matrix())
            .collect();

        world.insert_resource(RenderSyncOutput {
            instance_transforms: transforms,
        });
    }
}

/// System that updates particle emitters and simulates particles.
pub struct ParticleUpdateSystem;

impl System for ParticleUpdateSystem {
    fn name(&self) -> &str {
        "particle_update"
    }

    fn run(&mut self, world: &mut World) {
        let dt = world
            .resource::<quasar_core::time::Time>()
            .map(|t| t.delta_seconds())
            .unwrap_or(1.0 / 60.0);
        let gravity = glam::Vec3::new(0.0, -9.81, 0.0);

        world.for_each_mut(|_entity, emitter: &mut ParticleEmitter| {
            emitter.update(dt, gravity);
        });
    }
}

/// System that processes asset reload events and updates GPU resources.
/// This system listens for AssetEvent::Reloaded events from the AssetServer
/// and triggers GPU resource recreation when assets change.
pub struct GpuAssetSyncSystem {
    pending_reloads: Vec<(u64, String)>,
}

impl GpuAssetSyncSystem {
    pub fn new() -> Self {
        Self {
            pending_reloads: Vec::new(),
        }
    }
}

impl Default for GpuAssetSyncSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl System for GpuAssetSyncSystem {
    fn name(&self) -> &str {
        "gpu_asset_sync"
    }

    fn run(&mut self, world: &mut World) {
        // Check for asset reload events from AssetServer
        if let Some(asset_server) = world.resource::<AssetServer>() {
            // Poll events directly — process_reloads() consumed its own events
            // internally, so we poll raw events and handle both reloading and
            // GPU resource tracking in one pass.
            let events = asset_server.poll_events();

            for event in events {
                match event {
                    quasar_core::AssetEvent::Reloaded { handle, path } => {
                        log::info!("GPU asset reloaded: {:?} (handle {})", path, handle.id);
                        self.pending_reloads
                            .push((handle.id, path.to_string_lossy().to_string()));
                    }
                    quasar_core::AssetEvent::Loaded { handle, path } => {
                        log::debug!("GPU asset loaded: {:?} (handle {})", path, handle.id);
                    }
                    quasar_core::AssetEvent::Failed { path, error } => {
                        log::error!("GPU asset failed: {:?} - {}", path, error);
                    }
                }
            }
        }

        // Process pending GPU resource updates
        // In a full implementation, this would:
        // 1. Get the device/queue from a render resource
        // 2. Recreate textures/meshes based on the asset type
        // 3. Update bind groups

        if !self.pending_reloads.is_empty() {
            log::debug!(
                "Processing {} pending GPU asset reloads",
                self.pending_reloads.len()
            );
            self.pending_reloads.clear();
        }
    }
}

/// Render plugin that adds all rendering systems.
pub struct RenderPlugin {
    pub particles_enabled: bool,
    pub asset_sync_enabled: bool,
}

impl Default for RenderPlugin {
    fn default() -> Self {
        Self {
            particles_enabled: true,
            asset_sync_enabled: true,
        }
    }
}

// ── Render passes ──────────────────────────────────────────────────
//
// Each pass retrieves GPU resources from `RenderContext::resources` via
// `Arc<dyn Any>` downcasting.  The runner is responsible for populating
// the context before the graph is executed.

use crate::shadow::ShadowMap;
use crate::hdr::TonemappingPass;

/// Resource key constants for `RenderContext::resources`.
pub mod resource_keys {
    pub const SHADOW_MAP: &str = "shadow_map";
    pub const RENDER_PIPELINE: &str = "render_pipeline";
    pub const MESH_DRAW_LIST: &str = "mesh_draw_list";
    pub const POST_PROCESS: &str = "post_process";
    pub const TONEMAPPING: &str = "tonemapping";
    pub const UI_RENDER_PASS: &str = "ui_render_pass";
}

/// A single draw command for the opaque/transparent passes.
pub struct MeshDrawItem {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
    pub material_bind_group: wgpu::BindGroup,
    pub texture_bind_group: wgpu::BindGroup,
    pub camera_offset: u32,
}

/// Shared draw list populated by the runner before graph execution.
pub struct MeshDrawList {
    pub opaque: Vec<MeshDrawItem>,
    pub transparent: Vec<MeshDrawItem>,
}

struct ShadowRenderPass;
impl RenderPass for ShadowRenderPass {
    fn name(&self) -> &str { "shadow" }
    fn execute(&self, _device: &wgpu::Device, _queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, ctx: &RenderContext) {
        // Retrieve the ShadowMap from the resource context.
        let shadow_map = match ctx.resources.get(resource_keys::SHADOW_MAP) {
            Some(res) => res,
            None => {
                log::trace!("ShadowPass: no shadow_map resource — skipping");
                return;
            }
        };
        let shadow_map = match shadow_map.downcast_ref::<ShadowMap>() {
            Some(sm) => sm,
            None => {
                log::warn!("ShadowPass: shadow_map resource has wrong type");
                return;
            }
        };

        // Depth-only pass into the shadow map texture.
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Shadow Render Pass"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &shadow_map.view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(&shadow_map.pipeline);
        pass.set_bind_group(0, &shadow_map.camera_bind_group, &[]);

        // Draw opaque meshes from the draw list.
        if let Some(draw_list) = ctx.resources.get(resource_keys::MESH_DRAW_LIST) {
            if let Some(list) = draw_list.downcast_ref::<MeshDrawList>() {
                for item in &list.opaque {
                    pass.set_vertex_buffer(0, item.vertex_buffer.slice(..));
                    pass.set_index_buffer(item.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..item.index_count, 0, 0..1);
                }
            }
        }

        log::trace!("ShadowPass::execute — completed");
    }
}

struct OpaqueRenderPass;
impl RenderPass for OpaqueRenderPass {
    fn name(&self) -> &str { "opaque" }
    fn execute(&self, _device: &wgpu::Device, _queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, ctx: &RenderContext) {
        let hdr_view = match ctx.hdr_texture.as_ref() {
            Some(v) => v,
            None => {
                log::trace!("OpaquePass: no HDR target — skipping");
                return;
            }
        };

        let pipeline = match ctx.resources.get(resource_keys::RENDER_PIPELINE) {
            Some(res) => match res.downcast_ref::<wgpu::RenderPipeline>() {
                Some(p) => p,
                None => return,
            },
            None => {
                log::trace!("OpaquePass: no render_pipeline resource — skipping");
                return;
            }
        };

        let draw_list = match ctx.resources.get(resource_keys::MESH_DRAW_LIST) {
            Some(res) => match res.downcast_ref::<MeshDrawList>() {
                Some(dl) => dl,
                None => return,
            },
            None => return,
        };

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Opaque Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: hdr_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.05, g: 0.05, b: 0.08, a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &ctx.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(pipeline);
        pass.set_bind_group(2, &ctx.light_bind_group, &[]);

        for item in &draw_list.opaque {
            pass.set_bind_group(0, &ctx.camera_bind_group, &[item.camera_offset]);
            pass.set_bind_group(1, &item.material_bind_group, &[]);
            pass.set_bind_group(3, &item.texture_bind_group, &[]);
            pass.set_vertex_buffer(0, item.vertex_buffer.slice(..));
            pass.set_index_buffer(item.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..item.index_count, 0, 0..1);
        }

        log::trace!("OpaquePass::execute — {} items drawn", draw_list.opaque.len());
    }
}

struct TransparentRenderPass;
impl RenderPass for TransparentRenderPass {
    fn name(&self) -> &str { "transparent" }
    fn execute(&self, _device: &wgpu::Device, _queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, ctx: &RenderContext) {
        let hdr_view = match ctx.hdr_texture.as_ref() {
            Some(v) => v,
            None => return,
        };

        let pipeline = match ctx.resources.get(resource_keys::RENDER_PIPELINE) {
            Some(res) => res.downcast_ref::<wgpu::RenderPipeline>(),
            None => None,
        };
        let pipeline = match pipeline {
            Some(p) => p,
            None => return,
        };

        let draw_list = match ctx.resources.get(resource_keys::MESH_DRAW_LIST) {
            Some(res) => res.downcast_ref::<MeshDrawList>(),
            None => None,
        };
        let draw_list = match draw_list {
            Some(dl) => dl,
            None => return,
        };

        if draw_list.transparent.is_empty() {
            return;
        }

        // Transparent pass — loads existing color/depth, blends alpha.
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Transparent Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: hdr_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &ctx.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(pipeline);
        pass.set_bind_group(2, &ctx.light_bind_group, &[]);

        for item in &draw_list.transparent {
            pass.set_bind_group(0, &ctx.camera_bind_group, &[item.camera_offset]);
            pass.set_bind_group(1, &item.material_bind_group, &[]);
            pass.set_bind_group(3, &item.texture_bind_group, &[]);
            pass.set_vertex_buffer(0, item.vertex_buffer.slice(..));
            pass.set_index_buffer(item.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..item.index_count, 0, 0..1);
        }

        log::trace!("TransparentPass::execute — {} items drawn", draw_list.transparent.len());
    }
}

struct PostProcessRenderPass;
impl RenderPass for PostProcessRenderPass {
    fn name(&self) -> &str { "post_process" }
    fn execute(&self, _device: &wgpu::Device, _queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, ctx: &RenderContext) {
        let post = match ctx.resources.get(resource_keys::POST_PROCESS) {
            Some(res) => match res.downcast_ref::<crate::post_process::PostProcessPass>() {
                Some(pp) => pp,
                None => return,
            },
            None => {
                log::trace!("PostProcessPass: no post_process resource — skipping");
                return;
            }
        };

        // Run SSAO with bilateral blur.
        post.render_ssao_with_blur(encoder);

        log::trace!("PostProcessPass::execute — SSAO + blur completed");
    }
}

struct TonemapRenderPass;
impl RenderPass for TonemapRenderPass {
    fn name(&self) -> &str { "tonemap" }
    fn execute(&self, _device: &wgpu::Device, _queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, ctx: &RenderContext) {
        let tonemap = match ctx.resources.get(resource_keys::TONEMAPPING) {
            Some(res) => match res.downcast_ref::<TonemappingPass>() {
                Some(tp) => tp,
                None => return,
            },
            None => {
                log::trace!("TonemapPass: no tonemapping resource — skipping");
                return;
            }
        };

        // Tonemap from HDR target to a swapchain-compatible surface view.
        // The runner must provide a "surface_view" resource for the final output.
        let surface_view = match ctx.resources.get("surface_view") {
            Some(res) => match res.downcast_ref::<wgpu::TextureView>() {
                Some(v) => v,
                None => return,
            },
            None => {
                log::trace!("TonemapPass: no surface_view resource — skipping");
                return;
            }
        };

        tonemap.execute(encoder, surface_view);
        log::trace!("TonemapPass::execute — completed");
    }
}

struct UiRenderPass;
impl RenderPass for UiRenderPass {
    fn name(&self) -> &str { "ui" }
    fn execute(&self, _device: &wgpu::Device, _queue: &wgpu::Queue, _encoder: &mut wgpu::CommandEncoder, _ctx: &RenderContext) {
        // UI is rendered through egui/editor overlay in the runner.
        // This pass serves as a graph placeholder for dependency ordering.
        log::trace!("UiPass::execute — handled by editor overlay");
    }
}

impl quasar_core::Plugin for RenderPlugin {
    fn name(&self) -> &str {
        "RenderPlugin"
    }

    fn build(&self, app: &mut quasar_core::App) {
        // ── Build the render graph ────────────────────────────────
        let mut graph = RenderGraph::new();

        // Attachments
        graph.add_attachment(
            attachment_ids::HDR_COLOR,
            Attachment {
                name: "HDR Color".into(),
                format: wgpu::TextureFormat::Rgba16Float,
                size: (1920, 1080),
                texture: None,
                view: None,
            },
        );
        graph.add_attachment(
            attachment_ids::DEPTH,
            Attachment {
                name: "Depth".into(),
                format: wgpu::TextureFormat::Depth32Float,
                size: (1920, 1080),
                texture: None,
                view: None,
            },
        );
        graph.add_attachment(
            attachment_ids::SHADOW_MAP,
            Attachment {
                name: "Shadow Map".into(),
                format: wgpu::TextureFormat::Depth32Float,
                size: (2048, 2048),
                texture: None,
                view: None,
            },
        );

        // Passes (declared order = topological fallback)
        graph.add_pass(pass_ids::SHADOW, Box::new(ShadowRenderPass));
        graph.add_pass(pass_ids::OPAQUE, Box::new(OpaqueRenderPass));
        graph.add_pass(pass_ids::TRANSPARENT, Box::new(TransparentRenderPass));
        graph.add_pass(pass_ids::POST_PROCESS, Box::new(PostProcessRenderPass));
        graph.add_pass(pass_ids::TONEMAP, Box::new(TonemapRenderPass));
        graph.add_pass(pass_ids::UI, Box::new(UiRenderPass));

        // Dependencies
        graph.add_dependency(pass_ids::OPAQUE, pass_ids::SHADOW);
        graph.add_dependency(pass_ids::TRANSPARENT, pass_ids::OPAQUE);
        graph.add_dependency(pass_ids::POST_PROCESS, pass_ids::TRANSPARENT);
        graph.add_dependency(pass_ids::TONEMAP, pass_ids::POST_PROCESS);
        graph.add_dependency(pass_ids::UI, pass_ids::TONEMAP);

        // Output attachments
        graph.add_output(pass_ids::SHADOW, attachment_ids::SHADOW_MAP);
        graph.add_output(pass_ids::OPAQUE, attachment_ids::HDR_COLOR);
        graph.add_output(pass_ids::OPAQUE, attachment_ids::DEPTH);
        graph.add_output(pass_ids::TRANSPARENT, attachment_ids::HDR_COLOR);
        graph.add_output(pass_ids::POST_PROCESS, attachment_ids::HDR_COLOR);

        app.world.insert_resource(graph);

        // ── Systems ───────────────────────────────────────────────
        app.schedule.add_system(
            quasar_core::ecs::SystemStage::PostUpdate,
            Box::new(RenderSyncSystem),
        );

        if self.particles_enabled {
            app.schedule.add_system(
                quasar_core::ecs::SystemStage::Update,
                Box::new(ParticleUpdateSystem),
            );
        }

        if self.asset_sync_enabled {
            app.schedule.add_system(
                quasar_core::ecs::SystemStage::PostUpdate,
                Box::new(GpuAssetSyncSystem::new()),
            );
        }

        // ── Override registry for prefab system ────────────────
        {
            if app.world.resource::<quasar_core::OverrideRegistry>().is_none() {
                app.world.insert_resource(quasar_core::OverrideRegistry::new());
            }
            let registry = app
                .world
                .resource_mut::<quasar_core::OverrideRegistry>()
                .unwrap();

            registry.register("PointLight", |world, entity, field, value| {
                if let Some(light) = world.get_mut::<crate::light::PointLight>(entity) {
                    match field {
                        "intensity" => if let Some(v) = value.as_f64() { light.intensity = v as f32; },
                        "range" => if let Some(v) = value.as_f64() { light.range = v as f32; },
                        "falloff" => if let Some(v) = value.as_f64() { light.falloff = v as f32; },
                        "color.r" => if let Some(v) = value.as_f64() { light.color.x = v as f32; },
                        "color.g" => if let Some(v) = value.as_f64() { light.color.y = v as f32; },
                        "color.b" => if let Some(v) = value.as_f64() { light.color.z = v as f32; },
                        "position.x" => if let Some(v) = value.as_f64() { light.position.x = v as f32; },
                        "position.y" => if let Some(v) = value.as_f64() { light.position.y = v as f32; },
                        "position.z" => if let Some(v) = value.as_f64() { light.position.z = v as f32; },
                        _ => log::warn!("Unknown override path '{}' for PointLight", field),
                    }
                }
            });

            registry.register("DirectionalLight", |world, entity, field, value| {
                if let Some(light) = world.get_mut::<crate::light::DirectionalLight>(entity) {
                    match field {
                        "intensity" => if let Some(v) = value.as_f64() { light.intensity = v as f32; },
                        "color.r" => if let Some(v) = value.as_f64() { light.color.x = v as f32; },
                        "color.g" => if let Some(v) = value.as_f64() { light.color.y = v as f32; },
                        "color.b" => if let Some(v) = value.as_f64() { light.color.z = v as f32; },
                        "direction.x" => if let Some(v) = value.as_f64() { light.direction.x = v as f32; },
                        "direction.y" => if let Some(v) = value.as_f64() { light.direction.y = v as f32; },
                        "direction.z" => if let Some(v) = value.as_f64() { light.direction.z = v as f32; },
                        _ => log::warn!("Unknown override path '{}' for DirectionalLight", field),
                    }
                }
            });

            registry.register("SpotLight", |world, entity, field, value| {
                if let Some(light) = world.get_mut::<crate::light::SpotLight>(entity) {
                    match field {
                        "intensity" => if let Some(v) = value.as_f64() { light.intensity = v as f32; },
                        "range" => if let Some(v) = value.as_f64() { light.range = v as f32; },
                        "inner_angle" => if let Some(v) = value.as_f64() { light.inner_angle = v as f32; },
                        "outer_angle" => if let Some(v) = value.as_f64() { light.outer_angle = v as f32; },
                        "color.r" => if let Some(v) = value.as_f64() { light.color.x = v as f32; },
                        "color.g" => if let Some(v) = value.as_f64() { light.color.y = v as f32; },
                        "color.b" => if let Some(v) = value.as_f64() { light.color.z = v as f32; },
                        _ => log::warn!("Unknown override path '{}' for SpotLight", field),
                    }
                }
            });
        }

        log::info!("RenderPlugin loaded — render graph + systems active");
    }
}
