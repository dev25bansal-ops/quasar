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

/// Render plugin that adds all rendering systems.
pub struct RenderPlugin {
    pub particles_enabled: bool,
}

impl Default for RenderPlugin {
    fn default() -> Self {
        Self {
            particles_enabled: true,
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

        log::info!("RenderPlugin loaded — render systems active");
    }
}
