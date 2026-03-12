//! Logic Graph System — evaluates visual scripts at runtime.
//!
//! The LogicGraphAttachment component can be attached to any entity. Each frame,
//! the LogicGraphSystem queries entities with a LogicGraphAttachment component,
//! evaluates their nodes, and dispatches actions via Commands.

use crate::editor_state::{EditCommand, SetPositionCommand, SpawnEntityCommand, TransformData};
use crate::logic_graph::{LogicGraph as LogicGraphDef, LogicNodeKind};
use quasar_core::ecs::{Entity, World};
use quasar_math::{Transform, Vec3};

// ─── ECS Component ───────────────────────────────────────────────────

/// A logic graph attachment — this component on an entity indicates which
/// visual script it should follow.
#[derive(Debug, Clone)]
pub struct LogicGraphAttachment {
    /// The graph definition (nodes and connections).
    pub graph: LogicGraphDef,
    /// Cached compiled Lua script (empty if dirty).
    pub compiled_lua: String,
    /// Whether the graph needs recompilation.
    pub dirty: bool,
    /// Local state for the graph (variables, timers, etc.).
    pub state: Vec<(String, f32)>,
    /// Event queue — events from other systems (key presses, collisions, etc.).
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

    /// Mark the graph as needing recompilation.
    pub fn set_dirty(&mut self) {
        self.dirty = true;
    }

    /// Queue an event for this graph to process.
    pub fn queue_event(&mut self, name: String, data: Vec<u8>) {
        self.events.push((name, data));
    }
}

// ─── System ──────────────────────────────────────────────────────────

pub struct LogicGraphSystem;

impl LogicGraphSystem {
    pub fn new() -> Self {
        Self
    }

    pub fn update(&mut self, world: &mut World, dt: f32) -> Vec<Box<dyn EditCommand>> {
        let mut commands: Vec<Box<dyn EditCommand>> = Vec::new();

        // Query all entities with LogicGraphAttachment components
        for (entity, logic_graph) in world.query::<LogicGraphAttachment>() {
            // Recompile if needed
            if logic_graph.dirty {
                if let Ok(_lua) =
                    crate::logic_graph::LogicGraphCompiler::compile(&logic_graph.graph)
                {
                    // Note: In a real implementation, we would update the compiled_lua field
                    // but since we can't mutate through the query, this is a placeholder
                    log::debug!("Compiled logic graph for entity {:?}", entity);
                }
            }

            // Process events
            for (event_name, _data) in &logic_graph.events {
                self.handle_event(world, entity, event_name, &mut commands);
            }

            // Update nodes
            self.update_graph(world, entity, logic_graph, dt, &mut commands);
        }

        commands
    }

    fn handle_event(
        &mut self,
        world: &World,
        _entity: Entity,
        event_name: &str,
        _commands: &mut Vec<Box<dyn EditCommand>>,
    ) {
        // Find OnEvent nodes matching this event
        for (_e, lg) in world.query::<LogicGraphAttachment>() {
            for node in &lg.graph.nodes {
                if let LogicNodeKind::OnEvent {
                    event_name: node_event,
                } = &node.kind
                {
                    if node_event == event_name {
                        // Trigger the event node's execution chain
                        log::debug!("Event {:?} triggered on entity {:?}", event_name, _entity);
                        // Would execute the event handler here
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
        // Find all OnUpdate and OnStart nodes in the graph
        let nodes: Vec<_> = logic_graph.graph.nodes.iter().collect();

        for node in nodes {
            match &node.kind {
                LogicNodeKind::OnUpdate => {
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
                LogicNodeKind::SetPosition => {
                    // Would need to track position through the graph
                    // This is a simplified implementation
                }
                LogicNodeKind::Print => {
                    log::info!("LogicGraph print on entity {:?}", _entity);
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
                        // Get position from data inputs (simplified: use _entity's current position)
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
                    LogicNodeKind::Branch => {
                        // Evaluate condition and choose branch
                        // This would trace data inputs in a full implementation
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
