//! Render plugin — integrates rendering systems into the ECS.

use quasar_core::ecs::System;
use quasar_core::ecs::World;

use crate::ParticleEmitter;

/// System that syncs transforms to GPU buffers and updates render state.
pub struct RenderSyncSystem;

impl System for RenderSyncSystem {
    fn name(&self) -> &str {
        "render_sync"
    }

    fn run(&mut self, _world: &mut World) {
        // In a full implementation, this would:
        // 1. Update instance buffers for instanced rendering
        // 2. Update bone matrices for skinned meshes
        // 3. Update particle emitter positions
        // 4. Frustum culling
    }
}

/// System that updates particle emitters and simulates particles.
pub struct ParticleUpdateSystem;

impl System for ParticleUpdateSystem {
    fn name(&self) -> &str {
        "particle_update"
    }

    fn run(&mut self, world: &mut World) {
        // Note: Particle emitters need mutable access, which requires
        // a different query pattern. This is a placeholder.
        let _count = world.query::<ParticleEmitter>().count();
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
        // Note: This requires the AssetServer to be registered as a resource
        // and its event channel to be accessible

        // Process pending GPU resource updates
        // In a full implementation, this would:
        // 1. Get the device/queue from a render resource
        // 2. Recreate textures/meshes based on the asset type
        // 3. Update bind groups

        // For now, we check if there are any assets that need GPU sync
        // by looking at the asset storage in the world
        let _has_assets = world
            .resource::<quasar_core::asset::AssetManager>()
            .is_some();

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

impl quasar_core::Plugin for RenderPlugin {
    fn name(&self) -> &str {
        "RenderPlugin"
    }

    fn build(&self, app: &mut quasar_core::App) {
        // Add render sync system
        app.schedule.add_system(
            quasar_core::ecs::SystemStage::PostUpdate,
            Box::new(RenderSyncSystem),
        );

        // Add particle update system
        if self.particles_enabled {
            app.schedule.add_system(
                quasar_core::ecs::SystemStage::Update,
                Box::new(ParticleUpdateSystem),
            );
        }

        // Add GPU asset sync system
        if self.asset_sync_enabled {
            app.schedule.add_system(
                quasar_core::ecs::SystemStage::PostUpdate,
                Box::new(GpuAssetSyncSystem::new()),
            );
        }

        log::info!("RenderPlugin loaded — render systems active");
    }
}
