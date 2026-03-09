//! Physics plugin — integrates the physics world into the ECS runtime.
//!
//! # How it works
//!
//! 1. In **PostUpdate**, the plugin steps the Rapier simulation using a fixed timestep.
//! 2. After stepping, it reads the new positions/rotations from Rapier and
//!    writes them back into the ECS `Transform` components.
//! 3. Collision events are collected and routed through `App.events`.
//! 4. Transform mutations from ECS are written back to physics bodies before each step.

use quasar_core::ecs::{Entity, System, World};
use quasar_core::Events;
use quasar_math::Transform;

use crate::character_controller::CharacterControllerComponent;
use crate::collider::ColliderComponent;
use crate::events::{CollisionEvent, CollisionEventType, TriggerTracker};
use crate::rigidbody::RigidBodyComponent;
use crate::world::PhysicsWorld;

/// Fixed physics timestep in seconds (60 Hz).
pub const PHYSICS_FIXED_DT: f32 = 1.0 / 60.0;

/// Resource holding physics accumulator state for fixed timestep.
pub struct PhysicsResource {
    pub physics: PhysicsWorld,
    /// Time accumulator for fixed timestep physics.
    pub accumulator: f32,
}

impl PhysicsResource {
    pub fn new() -> Self {
        Self {
            physics: PhysicsWorld::new(),
            accumulator: 0.0,
        }
    }

    pub fn with_gravity(gx: f32, gy: f32, gz: f32) -> Self {
        Self {
            physics: PhysicsWorld::with_gravity(gx, gy, gz),
            accumulator: 0.0,
        }
    }
}

impl Default for PhysicsResource {
    fn default() -> Self {
        Self::new()
    }
}

/// System that writes ECS Transform changes back to physics bodies.
///
/// This should run BEFORE physics step to capture any teleportations
/// or scripted movements.
pub struct TransformWritebackSystem;

impl System for TransformWritebackSystem {
    fn name(&self) -> &str {
        "transform_writeback"
    }

    fn run(&mut self, world: &mut World) {
        if !quasar_core::simulation_active(world) { return; }
        if let Some(p) = world.resource_mut::<quasar_core::Profiler>() { p.begin_scope("transform_writeback"); }
        let handles: Vec<(rapier3d::prelude::RigidBodyHandle, Transform)> = world
            .query2::<RigidBodyComponent, Transform>()
            .into_iter()
            .map(|(_, rb, tf)| (rb.handle, *tf))
            .collect();

        if handles.is_empty() {
            return;
        }

        if let Some(resource) = world.resource_mut::<PhysicsResource>() {
            for (handle, tf) in handles {
                resource
                    .physics
                    .set_body_position(handle, tf.position.into());
                resource
                    .physics
                    .set_body_rotation(handle, tf.rotation.into());
            }
        }
        if let Some(p) = world.resource_mut::<quasar_core::Profiler>() { p.end_scope("transform_writeback"); }
    }
}

/// System that steps physics with fixed timestep accumulator.
///
/// Uses the accumulator pattern:
/// ```ignore
/// accumulator += delta_time;
/// while accumulator >= FIXED_DT {
///     physics.step();
///     accumulator -= FIXED_DT;
/// }
/// ```
pub struct PhysicsStepSystem;

impl System for PhysicsStepSystem {
    fn name(&self) -> &str {
        "physics_step"
    }

    fn run(&mut self, world: &mut World) {
        if !quasar_core::simulation_active(world) { return; }
        if let Some(p) = world.resource_mut::<quasar_core::Profiler>() { p.begin_scope("physics_step"); }
        let delta = world
            .resource::<quasar_core::TimeSnapshot>()
            .map(|t| t.delta_seconds)
            .unwrap_or(1.0 / 60.0);

        let handles: Vec<(Entity, rapier3d::prelude::RigidBodyHandle)> = world
            .query::<RigidBodyComponent>()
            .into_iter()
            .map(|(e, rbc)| (e, rbc.handle))
            .collect();

        if handles.is_empty() {
            return;
        }

        // Fixed timestep physics with accumulator
        if let Some(resource) = world.resource_mut::<PhysicsResource>() {
            resource.accumulator += delta;

            while resource.accumulator >= PHYSICS_FIXED_DT {
                resource.physics.step();
                resource.accumulator -= PHYSICS_FIXED_DT;
            }
        }

        // Read physics positions back to transforms
        let mut positions_to_write: Vec<(Entity, [f32; 3], [f32; 4])> = Vec::new();
        if let Some(resource) = world.resource::<PhysicsResource>() {
            for &(entity, handle) in &handles {
                if let Some(pos) = resource.physics.body_position(handle) {
                    if let Some(rot) = resource.physics.body_rotation(handle) {
                        positions_to_write.push((entity, pos, rot));
                    }
                }
            }
        }

        for (entity, pos, rot) in positions_to_write {
            if let Some(transform) = world.get_mut::<Transform>(entity) {
                transform.position = glam::Vec3::new(pos[0], pos[1], pos[2]);
                transform.rotation = glam::Quat::from_xyzw(rot[0], rot[1], rot[2], rot[3]);
            }
        }
        if let Some(p) = world.resource_mut::<quasar_core::Profiler>() { p.end_scope("physics_step"); }
    }
}

pub struct CollisionEventSystem {
    sender: crossbeam::channel::Sender<rapier3d::prelude::CollisionEvent>,
    receiver: crossbeam::channel::Receiver<rapier3d::prelude::CollisionEvent>,
}

impl CollisionEventSystem {
    pub fn new() -> Self {
        let (sender, receiver) = crossbeam::channel::unbounded();
        Self { sender, receiver }
    }

    pub fn channel(&self) -> crossbeam::channel::Sender<rapier3d::prelude::CollisionEvent> {
        self.sender.clone()
    }
}

impl Default for CollisionEventSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl System for CollisionEventSystem {
    fn name(&self) -> &str {
        "collision_events"
    }

    fn run(&mut self, world: &mut World) {
        if !quasar_core::simulation_active(world) { return; }
        if let Some(p) = world.resource_mut::<quasar_core::Profiler>() { p.begin_scope("collision_events"); }
        let collider_to_entity: std::collections::HashMap<
            rapier3d::prelude::ColliderHandle,
            Entity,
        > = world
            .query::<ColliderComponent>()
            .into_iter()
            .map(|(e, collider)| (collider.handle, e))
            .collect();

        // Use Events from world resource (which is synced from App.events)
        if let Some(events) = world.resource_mut::<Events>() {
            use rapier3d::prelude::CollisionEvent as RapierCollisionEvent;

            while let Ok(collision_event) = self.receiver.try_recv() {
                match collision_event {
                    RapierCollisionEvent::Started(collider1, collider2, _) => {
                        if let (Some(&entity1), Some(&entity2)) = (
                            collider_to_entity.get(&collider1),
                            collider_to_entity.get(&collider2),
                        ) {
                            events.send(CollisionEvent {
                                entity1,
                                entity2,
                                event_type: CollisionEventType::Started,
                                contact_point: None,
                                normal: None,
                            });
                        }
                    }
                    RapierCollisionEvent::Stopped(collider1, collider2, _) => {
                        if let (Some(&entity1), Some(&entity2)) = (
                            collider_to_entity.get(&collider1),
                            collider_to_entity.get(&collider2),
                        ) {
                            events.send(CollisionEvent {
                                entity1,
                                entity2,
                                event_type: CollisionEventType::Stopped,
                                contact_point: None,
                                normal: None,
                            });
                        }
                    }
                }
            }
        }
        if let Some(p) = world.resource_mut::<quasar_core::Profiler>() { p.end_scope("collision_events"); }
    }
}

/// System that creates / destroys Rapier joints for `JointComponent` entities.
pub struct JointSyncSystem;

impl System for JointSyncSystem {
    fn name(&self) -> &str {
        "joint_sync"
    }

    fn run(&mut self, world: &mut World) {
        if !quasar_core::simulation_active(world) { return; }
        if let Some(p) = world.resource_mut::<quasar_core::Profiler>() { p.begin_scope("joint_sync"); }
        use crate::joints::{build_rapier_joint, JointComponent};

        // Collect joints that still need a Rapier handle.
        let pending: Vec<(Entity, JointComponent)> = world
            .query::<JointComponent>()
            .into_iter()
            .filter(|(_, j)| j.handle.is_none())
            .map(|(e, j)| (e, j.clone()))
            .collect();

        if pending.is_empty() {
            return;
        }

        // Create joints in Rapier.
        let mut created: Vec<(Entity, rapier3d::prelude::ImpulseJointHandle)> = Vec::new();
        if let Some(resource) = world.resource_mut::<PhysicsResource>() {
            for (entity, joint) in &pending {
                let data = build_rapier_joint(&joint.kind);
                let handle = resource.physics.impulse_joints.insert(
                    joint.body_a,
                    joint.body_b,
                    data,
                    true,
                );
                created.push((*entity, handle));
            }
        }

        // Write handles back.
        for (entity, handle) in created {
            if let Some(j) = world.get_mut::<JointComponent>(entity) {
                j.handle = Some(handle);
            }
        }
        if let Some(p) = world.resource_mut::<quasar_core::Profiler>() { p.end_scope("joint_sync"); }
    }
}

pub struct CharacterControllerSystem;

impl System for CharacterControllerSystem {
    fn name(&self) -> &str {
        "character_controller"
    }

    fn run(&mut self, world: &mut World) {
        if !quasar_core::simulation_active(world) { return; }
        if let Some(p) = world.resource_mut::<quasar_core::Profiler>() { p.begin_scope("character_controller"); }
        let delta = world
            .resource::<quasar_core::TimeSnapshot>()
            .map(|t| t.delta_seconds)
            .unwrap_or(1.0 / 60.0);

        // Collect character controllers with their body handles and desired movement.
        let controllers: Vec<(
            Entity,
            rapier3d::prelude::RigidBodyHandle,
            rapier3d::prelude::ColliderHandle,
            crate::character_controller::CharacterControllerConfig,
            [f32; 3],
        )> = world
            .query2::<CharacterControllerComponent, RigidBodyComponent>()
            .into_iter()
            .map(|(e, cc, rb)| {
                (
                    e,
                    rb.handle,
                    cc.collider_handle,
                    cc.config.clone(),
                    cc.effective_velocity,
                )
            })
            .collect();

        if controllers.is_empty() {
            return;
        }

        // Move each character through the physics world.
        let mut results: Vec<(Entity, crate::character_controller::CharacterMovementResult)> =
            Vec::new();

        if let Some(resource) = world.resource_mut::<PhysicsResource>() {
            for (entity, body_handle, collider_handle, config, velocity) in &controllers {
                let result = resource.physics.move_character(
                    *body_handle,
                    *collider_handle,
                    *velocity,
                    config,
                    delta,
                );
                results.push((*entity, result));
            }
        }

        // Write back grounded state.
        for (entity, result) in results {
            if let Some(cc) = world.get_mut::<CharacterControllerComponent>(entity) {
                cc.grounded = result.grounded;
            }
        }
        if let Some(p) = world.resource_mut::<quasar_core::Profiler>() { p.end_scope("character_controller"); }
    }
}

/// System that creates Rapier colliders from [`PendingCollider`] components.
///
/// For each entity with a `PendingCollider` the system calls
/// `PhysicsWorld::add_collider` (or `add_static_collider`), inserts a
/// `ColliderComponent`, and removes the pending marker.
pub struct ColliderSyncSystem;

impl System for ColliderSyncSystem {
    fn name(&self) -> &str {
        "collider_sync"
    }

    fn run(&mut self, world: &mut World) {
        if let Some(p) = world.resource_mut::<quasar_core::Profiler>() { p.begin_scope("collider_sync"); }
        use crate::collider::PendingCollider;

        let pending: Vec<(Entity, PendingCollider)> = world
            .query::<PendingCollider>()
            .into_iter()
            .map(|(e, pc)| (e, pc.clone()))
            .collect();

        if pending.is_empty() {
            return;
        }

        let mut created: Vec<(Entity, rapier3d::prelude::ColliderHandle)> = Vec::new();

        if let Some(resource) = world.resource_mut::<PhysicsResource>() {
            for (entity, pc) in &pending {
                let handle = match pc.parent_body {
                    Some(body) => resource.physics.add_collider(
                        body,
                        &pc.shape,
                        pc.restitution,
                        pc.friction,
                    ),
                    None => resource.physics.add_static_collider(&pc.shape, pc.position),
                };
                created.push((*entity, handle));
            }
        }

        for (entity, handle) in created {
            world.insert(entity, ColliderComponent::new(handle));
            world.remove_raw(entity, std::any::TypeId::of::<PendingCollider>());
        }
        if let Some(p) = world.resource_mut::<quasar_core::Profiler>() { p.end_scope("collider_sync"); }
    }
}

/// System that dispatches trigger Enter/Stay/Exit events into the ECS event bus.
///
/// After the physics step, iterates over Rapier colliders marked as sensors
/// and feeds overlapping pairs into a [`TriggerTracker`]. The resulting events
/// are sent through `Events`.
pub struct TriggerEventSystem {
    tracker: TriggerTracker,
}

impl TriggerEventSystem {
    pub fn new() -> Self {
        Self {
            tracker: TriggerTracker::new(),
        }
    }
}

impl Default for TriggerEventSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl System for TriggerEventSystem {
    fn name(&self) -> &str {
        "trigger_events"
    }

    fn run(&mut self, world: &mut World) {
        if let Some(p) = world.resource_mut::<quasar_core::Profiler>() { p.begin_scope("trigger_events"); }
        // Build collider_handle → Entity map.
        let collider_to_entity: std::collections::HashMap<
            rapier3d::prelude::ColliderHandle,
            Entity,
        > = world
            .query::<ColliderComponent>()
            .into_iter()
            .map(|(e, cc)| (cc.handle, e))
            .collect();

        // Collect current sensor overlap pairs from Rapier.
        let mut current_pairs: Vec<(
            Entity,
            Entity,
            rapier3d::prelude::ColliderHandle,
            rapier3d::prelude::ColliderHandle,
        )> = Vec::new();

        if let Some(resource) = world.resource::<PhysicsResource>() {
            // Iterate over narrow-phase contact pairs where at least one
            // collider is a sensor.
            for pair in resource.physics.narrow_phase.intersection_pairs() {
                let (h1, h2, intersecting) = pair;
                if !intersecting {
                    continue;
                }
                if let (Some(&e1), Some(&e2)) =
                    (collider_to_entity.get(&h1), collider_to_entity.get(&h2))
                {
                    // Determine which is the sensor (trigger).
                    let c1_sensor = resource
                        .physics
                        .colliders
                        .get(h1)
                        .map(|c| c.is_sensor())
                        .unwrap_or(false);
                    if c1_sensor {
                        current_pairs.push((e1, e2, h1, h2));
                    } else {
                        current_pairs.push((e2, e1, h2, h1));
                    }
                }
            }
        }

        let (enters, stays, exits) = self.tracker.update(&current_pairs);

        // Dispatch into ECS events.
        if let Some(events) = world.resource_mut::<quasar_core::Events>() {
            for ev in enters {
                events.send(ev);
            }
            for ev in stays {
                events.send(ev);
            }
            for ev in exits {
                events.send(ev);
            }
        }
        if let Some(p) = world.resource_mut::<quasar_core::Profiler>() { p.end_scope("trigger_events"); }
    }
}

pub struct PhysicsPlugin {
    enable_collision_events: bool,
    enable_trigger_events: bool,
}

impl PhysicsPlugin {
    pub fn new() -> Self {
        Self {
            enable_collision_events: true,
            enable_trigger_events: true,
        }
    }

    pub fn without_collision_events() -> Self {
        Self {
            enable_collision_events: false,
            enable_trigger_events: true,
        }
    }
}

impl Default for PhysicsPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl quasar_core::Plugin for PhysicsPlugin {
    fn name(&self) -> &str {
        "PhysicsPlugin"
    }

    fn build(&self, app: &mut quasar_core::App) {
        // Insert physics resource with accumulator
        app.world.insert_resource(PhysicsResource::new());

        // PreUpdate: Write transforms back to physics (before step)
        app.schedule.add_system(
            quasar_core::ecs::SystemStage::PreUpdate,
            Box::new(TransformWritebackSystem),
        );

        // FixedUpdate: Step physics with fixed timestep
        app.schedule.add_system(
            quasar_core::ecs::SystemStage::FixedUpdate,
            Box::new(PhysicsStepSystem),
        );

        // FixedUpdate: Character controller movement (after physics step)
        app.schedule.add_system(
            quasar_core::ecs::SystemStage::FixedUpdate,
            Box::new(CharacterControllerSystem),
        );

        // PreUpdate: Sync ECS joint components → Rapier joints
        app.schedule.add_system(
            quasar_core::ecs::SystemStage::PreUpdate,
            Box::new(JointSyncSystem),
        );

        // PreUpdate: Create colliders from PendingCollider components
        app.schedule.add_system(
            quasar_core::ecs::SystemStage::PreUpdate,
            Box::new(ColliderSyncSystem),
        );

        if self.enable_collision_events {
            app.schedule.add_system(
                quasar_core::ecs::SystemStage::PostUpdate,
                Box::new(CollisionEventSystem::new()),
            );
        }

        if self.enable_trigger_events {
            app.schedule.add_system(
                quasar_core::ecs::SystemStage::PostUpdate,
                Box::new(TriggerEventSystem::new()),
            );
        }

        log::info!(
            "PhysicsPlugin loaded — Rapier3D simulation active (fixed timestep {}Hz)",
            (1.0 / PHYSICS_FIXED_DT) as u32
        );
    }
}
