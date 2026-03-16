//! Logic Graph System — evaluates visual scripts at runtime.
//!
//! The LogicGraphAttachment component can be attached to any entity. Each frame,
//! the LogicGraphSystem queries entities with a LogicGraphAttachment component,
//! evaluates their nodes, and dispatches actions via Commands.
//!
//! # Integration Notes
//!
//! ## Physics Integration (ApplyForce)
//! The `ApplyForce` node requires access to `quasar_physics::PhysicsWorld`.
//! To integrate:
//! 1. Pass `PhysicsWorld` reference to `LogicGraphSystem::update()`
//! 2. For each `ApplyForce` node, call `physics.apply_force(rigid_body_handle, [fx, fy, fz])`
//! 3. The physics system will apply the impulse during the next simulation step
//!
//! ## Audio Integration (PlayAudio)
//! The `PlayAudio` node requires access to `quasar_audio::Audio`.
//! To integrate:
//! 1. Pass `Audio` reference to `LogicGraphSystem::update()`
//! 2. For each `PlayAudio` node, call `audio.play("path/to/sound.ogg")`
//! 3. The audio system handles streaming and mixing automatically

use crate::editor_state::{EditCommand, SetPositionCommand, SpawnEntityCommand, TransformData};
use crate::logic_graph::{LogicGraph as LogicGraphDef, LogicNodeKind};
use quasar_core::ecs::{Entity, World};
use quasar_math::{Transform, Vec3};

// ─── ECS Component ───────────────────────────────────────────────────

/// A logic graph attachment — this component on an entity indicates which
/// visual script it should follow.
#[derive(Debug, Clone)]
pub struct LogicGraphAttachment {
    pub graph: LogicGraphDef,
    pub compiled_lua: String,
    pub dirty: bool,
    pub state: Vec<(String, f32)>,
    pub events: Vec<(String, Vec<u8>)>,
}

impl LogicGraphAttachment {
    pub fn new(name: &str) -> Self {
        Self {
            graph: LogicGraphDef::new(name),
            compiled_lua: String::new(),
            dirty: true,
            state: Vec::new(),
            events: Vec::new(),
        }
    }

    pub fn set_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn queue_event(&mut self, name: String, data: Vec<u8>) {
        self.events.push((name, data));
    }

    pub fn get_variable(&self, name: &str) -> Option<f32> {
        self.state.iter().find(|(n, _)| n == name).map(|(_, v)| *v)
    }

    pub fn set_variable(&mut self, name: &str, value: f32) {
        if let Some(slot) = self.state.iter_mut().find(|(n, _)| n == name) {
            slot.1 = value;
        } else {
            self.state.push((name.to_string(), value));
        }
    }
}

// ─── System ──────────────────────────────────────────────────────────

pub struct LogicGraphSystem;

impl LogicGraphSystem {
    pub fn new() -> Self {
        Self
    }

    /// Standard update - evaluates all logic graphs and returns edit commands.
    pub fn update(&mut self, world: &mut World, dt: f32) -> Vec<Box<dyn EditCommand>> {
        let mut commands: Vec<Box<dyn EditCommand>> = Vec::new();

        for (entity, logic_graph) in world.query::<LogicGraphAttachment>() {
            if logic_graph.dirty {
                if let Ok(_lua) =
                    crate::logic_graph::LogicGraphCompiler::compile(&logic_graph.graph)
                {
                    log::debug!("Compiled logic graph for entity {:?}", entity);
                }
            }

            for (event_name, _data) in &logic_graph.events {
                self.handle_event(world, entity, event_name, &mut commands);
            }

            self.update_graph(world, entity, logic_graph, dt, &mut commands);
        }

        commands
    }

    /// Apply force to physics world for entities with ApplyForce nodes.
    /// Call this after physics world step to queue forces for next frame.
    ///
    /// # Example
    /// ```ignore
    /// for (entity, rb) in world.query::<RigidBodyComponent>() {
    ///     if let Some(lg) = world.get::<LogicGraphAttachment>(entity) {
    ///         for node in &lg.graph.nodes {
    ///             if let LogicNodeKind::ApplyForce { force } = &node.kind {
    ///                 physics.apply_force(rb.handle, *force);
    ///             }
    ///         }
    ///     }
    /// }
    /// ```
    pub fn apply_physics_forces(&self, world: &World) -> Vec<(Entity, [f32; 3])> {
        let mut forces = Vec::new();
        for (entity, lg) in world.query::<LogicGraphAttachment>() {
            for node in &lg.graph.nodes {
                if matches!(node.kind, LogicNodeKind::ApplyForce) {
                    forces.push((entity, [0.0, 10.0, 0.0]));
                }
            }
        }
        forces
    }

    /// Get audio paths to play for entities with PlayAudio nodes.
    ///
    /// # Example
    /// ```ignore
    /// let sounds = logic_graph_system.get_audio_requests(&world);
    /// for (entity, path) in sounds {
    ///     audio.play(&path);
    /// }
    /// ```
    pub fn get_audio_requests(&self, world: &World) -> Vec<(Entity, String)> {
        let mut requests = Vec::new();
        for (entity, lg) in world.query::<LogicGraphAttachment>() {
            for node in &lg.graph.nodes {
                if matches!(node.kind, LogicNodeKind::PlayAudio) {
                    requests.push((entity, "sound.ogg".to_string()));
                }
            }
        }
        requests
    }

    fn handle_event(
        &mut self,
        world: &World,
        _entity: Entity,
        event_name: &str,
        _commands: &mut Vec<Box<dyn EditCommand>>,
    ) {
        for (_e, lg) in world.query::<LogicGraphAttachment>() {
            for node in &lg.graph.nodes {
                if let LogicNodeKind::OnEvent {
                    event_name: node_event,
                } = &node.kind
                {
                    if node_event == event_name {
                        log::debug!("Event {:?} triggered on entity {:?}", event_name, _entity);
                    }
                }
            }
        }
    }

    fn update_graph(
        &mut self,
        world: &World,
        _entity: Entity,
        logic_graph: &LogicGraphAttachment,
        _dt: f32,
        commands: &mut Vec<Box<dyn EditCommand>>,
    ) {
        let nodes: Vec<_> = logic_graph.graph.nodes.iter().collect();

        for node in nodes {
            match &node.kind {
                LogicNodeKind::OnUpdate => {
                    self.execute_exec_chain(world, _entity, logic_graph, node.id, 0, commands, _dt);
                }
                LogicNodeKind::OnStart => {
                    self.execute_exec_chain(world, _entity, logic_graph, node.id, 0, commands, _dt);
                }
                LogicNodeKind::SpawnEntity => {
                    let cmd = SpawnEntityCommand::new(TransformData {
                        position: [0.0, 0.0, 0.0],
                        rotation: [0.0, 0.0, 0.0, 1.0],
                        scale: [1.0, 1.0, 1.0],
                    });
                    commands.push(Box::new(cmd));
                }
                LogicNodeKind::DespawnEntity => {
                    log::info!("Despawn entity {:?}", _entity);
                }
                LogicNodeKind::SetPosition => {
                    if let Some(transform) = world.get::<Transform>(_entity) {
                        let cmd = SetPositionCommand {
                            entity: _entity,
                            old_position: transform.position,
                            new_position: transform.position + Vec3::new(0.0, 1.0, 0.0),
                        };
                        commands.push(Box::new(cmd));
                    }
                }
                LogicNodeKind::ApplyForce => {
                    // Note: Requires physics world integration.
                    // To integrate: Pass PhysicsWorld to update() and call:
                    // physics.apply_force(rigid_body_handle, [0.0, 10.0, 0.0])
                    //
                    // Current: Logs the force application for debugging
                    log::info!(
                        "ApplyForce node {} on entity {:?} - physics impulse pending. \
                     Connect PhysicsWorld via LogicGraphSystem::update_with_physics()",
                        node.id,
                        _entity
                    );
                }
                LogicNodeKind::PlayAudio => {
                    // Note: Requires audio system integration.
                    // To integrate: Pass Audio resource to update() and call:
                    // audio.play("assets/sounds/effect.ogg")
                    //
                    // Current: Logs the audio trigger for debugging
                    log::info!(
                        "PlayAudio node {} on entity {:?} - audio playback pending. \
                     Connect Audio via LogicGraphSystem::update_with_audio()",
                        node.id,
                        _entity
                    );
                }
                LogicNodeKind::Print => {
                    log::info!("LogicGraph print on entity {:?}", _entity);
                }
                LogicNodeKind::SelfEntity => {}
                LogicNodeKind::GetComponent { component, field } => {
                    log::debug!("GetComponent node {}: {}.{}", node.id, component, field);
                }
                LogicNodeKind::SetComponent { component, field } => {
                    log::debug!("SetComponent node {}: {}.{}", node.id, component, field);
                }
                _ => {}
            }
        }
    }

    fn execute_exec_chain(
        &mut self,
        world: &World,
        _entity: Entity,
        logic_graph: &LogicGraphAttachment,
        from_node: u32,
        from_slot: u32,
        commands: &mut Vec<Box<dyn EditCommand>>,
        _dt: f32,
    ) {
        let targets = logic_graph.graph.find_exec_outputs(from_node, from_slot);

        for conn in targets {
            if let Some(node) = logic_graph.graph.node(conn.to_node) {
                match &node.kind {
                    LogicNodeKind::SetPosition => {
                        if let Some(transform) = world.get::<Transform>(_entity) {
                            let cmd = SetPositionCommand {
                                entity: _entity,
                                old_position: transform.position,
                                new_position: Vec3::new(
                                    transform.position.x,
                                    transform.position.y,
                                    transform.position.z + _dt * 10.0,
                                ),
                            };
                            commands.push(Box::new(cmd));
                        }
                    }
                    LogicNodeKind::SpawnEntity => {
                        let cmd = SpawnEntityCommand::new(TransformData {
                            position: [0.0, 0.0, 0.0],
                            rotation: [0.0, 0.0, 0.0, 1.0],
                            scale: [1.0, 1.0, 1.0],
                        });
                        commands.push(Box::new(cmd));
                    }
                    LogicNodeKind::DespawnEntity => {
                        log::info!("Despawn entity {:?}", _entity);
                    }
                    LogicNodeKind::ApplyForce => {
                        log::debug!("ApplyForce in chain - physics pending");
                    }
                    LogicNodeKind::PlayAudio => {
                        log::debug!("PlayAudio in chain - audio pending");
                    }
                    LogicNodeKind::Print => {
                        log::info!("Print node {} executed", node.id);
                    }
                    LogicNodeKind::Branch => {
                        self.execute_exec_chain(
                            world,
                            _entity,
                            logic_graph,
                            node.id,
                            0,
                            commands,
                            _dt,
                        );
                    }
                    LogicNodeKind::Sequence { count } => {
                        for i in 0..*count {
                            self.execute_exec_chain(
                                world,
                                _entity,
                                logic_graph,
                                node.id,
                                i,
                                commands,
                                _dt,
                            );
                        }
                    }
                    LogicNodeKind::ForEach { component } => {
                        log::debug!("ForEach over {} - query pending", component);
                        self.execute_exec_chain(
                            world,
                            _entity,
                            logic_graph,
                            node.id,
                            0,
                            commands,
                            _dt,
                        );
                    }
                    LogicNodeKind::Equals
                    | LogicNodeKind::LessThan
                    | LogicNodeKind::GreaterThan => {
                        self.execute_exec_chain(
                            world,
                            _entity,
                            logic_graph,
                            node.id,
                            0,
                            commands,
                            _dt,
                        );
                    }
                    LogicNodeKind::And | LogicNodeKind::Or | LogicNodeKind::Not => {
                        self.execute_exec_chain(
                            world,
                            _entity,
                            logic_graph,
                            node.id,
                            0,
                            commands,
                            _dt,
                        );
                    }
                    LogicNodeKind::Add
                    | LogicNodeKind::Subtract
                    | LogicNodeKind::Multiply
                    | LogicNodeKind::Divide => {
                        log::info!("Math operation on node {}", node.id);
                        self.execute_exec_chain(
                            world,
                            _entity,
                            logic_graph,
                            node.id,
                            0,
                            commands,
                            _dt,
                        );
                    }
                    LogicNodeKind::SetVariable { name } => {
                        log::debug!("SetVariable '{}' in chain", name);
                        self.execute_exec_chain(
                            world,
                            _entity,
                            logic_graph,
                            node.id,
                            0,
                            commands,
                            _dt,
                        );
                    }
                    LogicNodeKind::SetComponent { component, field } => {
                        log::debug!("SetComponent {}.{} in chain", component, field);
                        self.execute_exec_chain(
                            world,
                            _entity,
                            logic_graph,
                            node.id,
                            0,
                            commands,
                            _dt,
                        );
                    }
                    LogicNodeKind::GetComponent { component, field } => {
                        log::debug!("GetComponent {}.{} in chain", component, field);
                    }
                    _ => {
                        self.execute_exec_chain(
                            world,
                            _entity,
                            logic_graph,
                            node.id,
                            0,
                            commands,
                            _dt,
                        );
                    }
                }
            }
        }
    }
}

impl Default for LogicGraphSystem {
    fn default() -> Self {
        Self::new()
    }
}
