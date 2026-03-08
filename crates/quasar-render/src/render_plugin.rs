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

// ── Stub render passes ─────────────────────────────────────────────

struct ShadowPass;
impl RenderPass for ShadowPass {
    fn name(&self) -> &str { "shadow" }
    fn execute(&self, _device: &wgpu::Device, _queue: &wgpu::Queue, _encoder: &mut wgpu::CommandEncoder, _ctx: &RenderContext) {
        log::trace!("ShadowPass::execute");
    }
}

struct OpaquePass;
impl RenderPass for OpaquePass {
    fn name(&self) -> &str { "opaque" }
    fn execute(&self, _device: &wgpu::Device, _queue: &wgpu::Queue, _encoder: &mut wgpu::CommandEncoder, _ctx: &RenderContext) {
        log::trace!("OpaquePass::execute");
    }
}

struct TransparentPass;
impl RenderPass for TransparentPass {
    fn name(&self) -> &str { "transparent" }
    fn execute(&self, _device: &wgpu::Device, _queue: &wgpu::Queue, _encoder: &mut wgpu::CommandEncoder, _ctx: &RenderContext) {
        log::trace!("TransparentPass::execute");
    }
}

struct PostProcessPass;
impl RenderPass for PostProcessPass {
    fn name(&self) -> &str { "post_process" }
    fn execute(&self, _device: &wgpu::Device, _queue: &wgpu::Queue, _encoder: &mut wgpu::CommandEncoder, _ctx: &RenderContext) {
        log::trace!("PostProcessPass::execute");
    }
}

struct UiPass;
impl RenderPass for UiPass {
    fn name(&self) -> &str { "ui" }
    fn execute(&self, _device: &wgpu::Device, _queue: &wgpu::Queue, _encoder: &mut wgpu::CommandEncoder, _ctx: &RenderContext) {
        log::trace!("UiPass::execute");
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
        graph.add_pass(pass_ids::SHADOW, Box::new(ShadowPass));
        graph.add_pass(pass_ids::OPAQUE, Box::new(OpaquePass));
        graph.add_pass(pass_ids::TRANSPARENT, Box::new(TransparentPass));
        graph.add_pass(pass_ids::POST_PROCESS, Box::new(PostProcessPass));
        graph.add_pass(pass_ids::UI, Box::new(UiPass));

        // Dependencies
        graph.add_dependency(pass_ids::OPAQUE, pass_ids::SHADOW);
        graph.add_dependency(pass_ids::TRANSPARENT, pass_ids::OPAQUE);
        graph.add_dependency(pass_ids::POST_PROCESS, pass_ids::TRANSPARENT);
        graph.add_dependency(pass_ids::UI, pass_ids::POST_PROCESS);

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

        log::info!("RenderPlugin loaded — render graph + systems active");
    }
}
