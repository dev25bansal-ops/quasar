//! Logic graph — visual scripting for game logic.
//!
//! A node-based graph where each node represents a logic operation
//! (event listener, math, branch, state machine transition, ECS query,
//! component read/write, etc.).  The graph compiles down to a Lua script
//! that the scripting engine executes.

use std::fmt::Write;

// ── Node types ─────────────────────────────────────────────────────

pub type NodeId = u32;
pub type SlotIndex = u32;

/// Categories of logic-graph nodes.
#[derive(Debug, Clone, PartialEq)]
pub enum LogicNodeKind {
    // ─── Events ───
    /// Fires every frame. Output 0 = dt (float).
    OnUpdate,
    /// Fires once on start. No inputs.
    OnStart,
    /// Fires on a named event. Parameter: event name.
    OnEvent { event_name: String },
    /// Fires when a key is pressed. Parameter: key name.
    OnKeyPressed { key: String },

    // ─── Flow control ───
    /// If-else branch. Input 0 = condition (bool). Exec out 0 = true, 1 = false.
    Branch,
    /// For-each over query results. Input 0 = exec in. Exec out 0 = loop body, 1 = done.
    ForEach { component: String },
    /// Sequence: executes outputs in order.
    Sequence { count: u32 },

    // ─── ECS ───
    /// Get component value. Input 0 = entity_id. Parameter: component name, field name.
    GetComponent { component: String, field: String },
    /// Set component value. Input 0 = exec, 1 = entity_id, 2 = value.
    SetComponent { component: String, field: String },
    /// Spawn entity. Exec out 0 = continue. Output 1 = new entity id.
    SpawnEntity,
    /// Despawn entity. Input 0 = exec, 1 = entity_id.
    DespawnEntity,
    /// Get self entity id (for per-entity scripts).
    SelfEntity,

    // ─── Math ───
    Add,
    Subtract,
    Multiply,
    Divide,
    /// Comparison. Input 0, 1. Output = bool.
    GreaterThan,
    LessThan,
    Equals,
    And,
    Or,
    Not,
    /// Constant float value.
    FloatLiteral(f32),
    /// Constant string value.
    StringLiteral(String),
    /// Constant bool value.
    BoolLiteral(bool),
    Vec3Construct,

    // ─── Actions ───
    /// Log a message. Input 0 = exec, 1 = message (string).
    Print,
    /// Play audio. Input 0 = exec, 1 = path (string).
    PlayAudio,
    /// Apply force. Input 0 = exec, 1 = entity, 2 = x, 3 = y, 4 = z.
    ApplyForce,
    /// Set position. Input 0 = exec, 1 = entity, 2 = x, 3 = y, 4 = z.
    SetPosition,

    // ─── Variables ───
    /// Get a named variable from the graph's local state.
    GetVariable { name: String },
    /// Set a named variable. Input 0 = exec, 1 = value.
    SetVariable { name: String },
}

/// A single node in the logic graph.
#[derive(Debug, Clone)]
pub struct LogicNode {
    pub id: NodeId,
    pub kind: LogicNodeKind,
    pub label: String,
    pub editor_pos: [f32; 2],
}

impl LogicNode {
    pub fn new(id: NodeId, kind: LogicNodeKind) -> Self {
        let label = match &kind {
            LogicNodeKind::OnUpdate => "On Update".into(),
            LogicNodeKind::OnStart => "On Start".into(),
            LogicNodeKind::OnEvent { event_name } => format!("On Event: {}", event_name),
            LogicNodeKind::OnKeyPressed { key } => format!("Key: {}", key),
            LogicNodeKind::Branch => "Branch".into(),
            LogicNodeKind::ForEach { component } => format!("For Each {}", component),
            LogicNodeKind::Sequence { count } => format!("Sequence ({})", count),
            LogicNodeKind::GetComponent { component, field } => format!("Get {}.{}", component, field),
            LogicNodeKind::SetComponent { component, field } => format!("Set {}.{}", component, field),
            LogicNodeKind::SpawnEntity => "Spawn Entity".into(),
            LogicNodeKind::DespawnEntity => "Despawn Entity".into(),
            LogicNodeKind::SelfEntity => "Self Entity".into(),
            LogicNodeKind::Add => "Add".into(),
            LogicNodeKind::Subtract => "Subtract".into(),
            LogicNodeKind::Multiply => "Multiply".into(),
            LogicNodeKind::Divide => "Divide".into(),
            LogicNodeKind::GreaterThan => ">".into(),
            LogicNodeKind::LessThan => "<".into(),
            LogicNodeKind::Equals => "==".into(),
            LogicNodeKind::And => "AND".into(),
            LogicNodeKind::Or => "OR".into(),
            LogicNodeKind::Not => "NOT".into(),
            LogicNodeKind::FloatLiteral(v) => format!("{:.2}", v),
            LogicNodeKind::StringLiteral(s) => format!("\"{}\"", s),
            LogicNodeKind::BoolLiteral(b) => format!("{}", b),
            LogicNodeKind::Vec3Construct => "Vec3".into(),
            LogicNodeKind::Print => "Print".into(),
            LogicNodeKind::PlayAudio => "Play Audio".into(),
            LogicNodeKind::ApplyForce => "Apply Force".into(),
            LogicNodeKind::SetPosition => "Set Position".into(),
            LogicNodeKind::GetVariable { name } => format!("Get {}", name),
            LogicNodeKind::SetVariable { name } => format!("Set {}", name),
        };
        Self { id, kind, label, editor_pos: [0.0, 0.0] }
    }

    /// Number of execution (flow) input slots.
    pub fn exec_input_count(&self) -> u32 {
        match &self.kind {
            LogicNodeKind::OnUpdate | LogicNodeKind::OnStart
            | LogicNodeKind::OnEvent { .. } | LogicNodeKind::OnKeyPressed { .. }
            | LogicNodeKind::SelfEntity
            | LogicNodeKind::FloatLiteral(_) | LogicNodeKind::StringLiteral(_) | LogicNodeKind::BoolLiteral(_)
            | LogicNodeKind::Add | LogicNodeKind::Subtract | LogicNodeKind::Multiply | LogicNodeKind::Divide
            | LogicNodeKind::GreaterThan | LogicNodeKind::LessThan | LogicNodeKind::Equals
            | LogicNodeKind::And | LogicNodeKind::Or | LogicNodeKind::Not
            | LogicNodeKind::Vec3Construct
            | LogicNodeKind::GetComponent { .. } | LogicNodeKind::GetVariable { .. } => 0,
            _ => 1,
        }
    }

    /// Number of data input slots.
    pub fn data_input_count(&self) -> u32 {
        match &self.kind {
            LogicNodeKind::OnUpdate | LogicNodeKind::OnStart
            | LogicNodeKind::OnEvent { .. } | LogicNodeKind::SelfEntity => 0,
            LogicNodeKind::OnKeyPressed { .. } => 0,
            LogicNodeKind::Branch => 1,              // condition
            LogicNodeKind::ForEach { .. } => 0,
            LogicNodeKind::Sequence { .. } => 0,
            LogicNodeKind::GetComponent { .. } => 1, // entity_id
            LogicNodeKind::SetComponent { .. } => 2, // entity_id, value
            LogicNodeKind::SpawnEntity => 0,
            LogicNodeKind::DespawnEntity => 1,       // entity_id
            LogicNodeKind::Add | LogicNodeKind::Subtract
            | LogicNodeKind::Multiply | LogicNodeKind::Divide
            | LogicNodeKind::GreaterThan | LogicNodeKind::LessThan | LogicNodeKind::Equals
            | LogicNodeKind::And | LogicNodeKind::Or => 2,
            LogicNodeKind::Not => 1,
            LogicNodeKind::FloatLiteral(_) | LogicNodeKind::StringLiteral(_) | LogicNodeKind::BoolLiteral(_) => 0,
            LogicNodeKind::Vec3Construct => 3,       // x, y, z
            LogicNodeKind::Print => 1,               // message
            LogicNodeKind::PlayAudio => 1,           // path
            LogicNodeKind::ApplyForce => 4,          // entity, x, y, z
            LogicNodeKind::SetPosition => 4,         // entity, x, y, z
            LogicNodeKind::GetVariable { .. } => 0,
            LogicNodeKind::SetVariable { .. } => 1,  // value
        }
    }

    /// Number of execution (flow) output slots.
    pub fn exec_output_count(&self) -> u32 {
        match &self.kind {
            LogicNodeKind::OnUpdate | LogicNodeKind::OnStart
            | LogicNodeKind::OnEvent { .. } | LogicNodeKind::OnKeyPressed { .. } => 1,
            LogicNodeKind::Branch => 2,               // true, false
            LogicNodeKind::ForEach { .. } => 2,       // body, done
            LogicNodeKind::Sequence { count } => *count,
            LogicNodeKind::SpawnEntity => 1,
            LogicNodeKind::DespawnEntity => 1,
            LogicNodeKind::SetComponent { .. } => 1,
            LogicNodeKind::Print | LogicNodeKind::PlayAudio
            | LogicNodeKind::ApplyForce | LogicNodeKind::SetPosition => 1,
            LogicNodeKind::SetVariable { .. } => 1,
            _ => 0,
        }
    }

    /// Number of data output slots.
    pub fn data_output_count(&self) -> u32 {
        match &self.kind {
            LogicNodeKind::OnUpdate => 1,             // dt
            LogicNodeKind::ForEach { .. } => 1,       // current entity_id
            LogicNodeKind::GetComponent { .. } => 1,  // value
            LogicNodeKind::SpawnEntity => 1,          // new entity id
            LogicNodeKind::SelfEntity => 1,           // entity id
            LogicNodeKind::Add | LogicNodeKind::Subtract
            | LogicNodeKind::Multiply | LogicNodeKind::Divide => 1,
            LogicNodeKind::GreaterThan | LogicNodeKind::LessThan | LogicNodeKind::Equals
            | LogicNodeKind::And | LogicNodeKind::Or | LogicNodeKind::Not => 1,
            LogicNodeKind::FloatLiteral(_) | LogicNodeKind::StringLiteral(_) | LogicNodeKind::BoolLiteral(_) => 1,
            LogicNodeKind::Vec3Construct => 3,        // x, y, z
            LogicNodeKind::GetVariable { .. } => 1,
            _ => 0,
        }
    }
}

// ── Connection ─────────────────────────────────────────────────────

/// Connection type: exec (flow) vs data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConnectionKind {
    Exec,
    Data,
}

/// A directed edge in the logic graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LogicConnection {
    pub kind: ConnectionKind,
    pub from_node: NodeId,
    pub from_slot: SlotIndex,
    pub to_node: NodeId,
    pub to_slot: SlotIndex,
}

// ── Graph ──────────────────────────────────────────────────────────

/// The logic graph — a DAG of [`LogicNode`]s compiled to Lua.
#[derive(Debug, Clone)]
pub struct LogicGraph {
    pub name: String,
    pub nodes: Vec<LogicNode>,
    pub connections: Vec<LogicConnection>,
    next_id: NodeId,
}

impl LogicGraph {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            nodes: Vec::new(),
            connections: Vec::new(),
            next_id: 0,
        }
    }

    pub fn add_node(&mut self, kind: LogicNodeKind) -> NodeId {
        let id = self.next_id;
        self.next_id += 1;
        self.nodes.push(LogicNode::new(id, kind));
        id
    }

    pub fn connect(&mut self, kind: ConnectionKind, from_node: NodeId, from_slot: SlotIndex, to_node: NodeId, to_slot: SlotIndex) {
        self.connections.push(LogicConnection { kind, from_node, from_slot, to_node, to_slot });
    }

    pub fn node(&self, id: NodeId) -> Option<&LogicNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// Find the data connection driving a specific input slot.
    pub fn find_data_input(&self, node_id: NodeId, slot: SlotIndex) -> Option<&LogicConnection> {
        self.connections.iter().find(|c| {
            c.kind == ConnectionKind::Data && c.to_node == node_id && c.to_slot == slot
        })
    }

    /// Find exec connections from a specific output slot.
    pub fn find_exec_outputs(&self, node_id: NodeId, slot: SlotIndex) -> Vec<&LogicConnection> {
        self.connections.iter().filter(|c| {
            c.kind == ConnectionKind::Exec && c.from_node == node_id && c.from_slot == slot
        }).collect()
    }
}

// ── Compiler: Logic Graph → Lua ────────────────────────────────────

/// Compiles a [`LogicGraph`] into a Lua script string that the scripting
/// engine can execute as a per-entity behaviour table.
pub struct LogicGraphCompiler;

impl LogicGraphCompiler {
    pub fn compile(graph: &LogicGraph) -> Result<String, String> {
        let mut out = String::new();

        writeln!(out, "-- Auto-generated from logic graph: {}", graph.name).unwrap();
        writeln!(out, "local _state = {{}}").unwrap();
        writeln!(out, "local behaviour = {{}}").unwrap();
        writeln!(out).unwrap();

        // Find entry-point nodes.
        let on_start_nodes: Vec<&LogicNode> = graph.nodes.iter()
            .filter(|n| matches!(n.kind, LogicNodeKind::OnStart))
            .collect();
        let on_update_nodes: Vec<&LogicNode> = graph.nodes.iter()
            .filter(|n| matches!(n.kind, LogicNodeKind::OnUpdate))
            .collect();
        let on_event_nodes: Vec<&LogicNode> = graph.nodes.iter()
            .filter(|n| matches!(n.kind, LogicNodeKind::OnEvent { .. }))
            .collect();

        // on_start
        if !on_start_nodes.is_empty() {
            writeln!(out, "function behaviour.on_start(entity_id)").unwrap();
            for node in &on_start_nodes {
                Self::compile_exec_chain(graph, node.id, 0, &mut out, 1)?;
            }
            writeln!(out, "end").unwrap();
            writeln!(out).unwrap();
        }

        // on_update
        if !on_update_nodes.is_empty() {
            writeln!(out, "function behaviour.on_update(entity_id, dt)").unwrap();
            for node in &on_update_nodes {
                Self::compile_exec_chain(graph, node.id, 0, &mut out, 1)?;
            }
            writeln!(out, "end").unwrap();
            writeln!(out).unwrap();
        }

        // on_event handlers → registered via on_start
        if !on_event_nodes.is_empty() {
            // Append event registrations to on_start
            writeln!(out, "local _orig_start = behaviour.on_start or function() end").unwrap();
            writeln!(out, "function behaviour.on_start(entity_id)").unwrap();
            writeln!(out, "  _orig_start(entity_id)").unwrap();
            for node in &on_event_nodes {
                if let LogicNodeKind::OnEvent { event_name } = &node.kind {
                    writeln!(out, "  quasar.on_event(\"{}\", function(data)", event_name).unwrap();
                    Self::compile_exec_chain(graph, node.id, 0, &mut out, 2)?;
                    writeln!(out, "  end)").unwrap();
                }
            }
            writeln!(out, "end").unwrap();
            writeln!(out).unwrap();
        }

        writeln!(out, "return behaviour").unwrap();
        Ok(out)
    }

    /// Recursively compile exec chain starting from an exec output slot.
    fn compile_exec_chain(
        graph: &LogicGraph,
        from_node: NodeId,
        from_slot: SlotIndex,
        out: &mut String,
        indent: usize,
    ) -> Result<(), String> {
        let _pad = "  ".repeat(indent);
        let targets = graph.find_exec_outputs(from_node, from_slot);
        for conn in targets {
            let node = graph.node(conn.to_node)
                .ok_or_else(|| format!("Missing node {}", conn.to_node))?;
            Self::compile_node(graph, node, out, indent)?;
        }
        Ok(())
    }

    /// Compile a single node's execution and recurse.
    fn compile_node(
        graph: &LogicGraph,
        node: &LogicNode,
        out: &mut String,
        indent: usize,
    ) -> Result<(), String> {
        let pad = "  ".repeat(indent);
        match &node.kind {
            LogicNodeKind::Print => {
                let msg = Self::compile_data_input(graph, node.id, 0)?;
                writeln!(out, "{}quasar.log(tostring({}))", pad, msg).unwrap();
                Self::compile_exec_chain(graph, node.id, 0, out, indent)?;
            }
            LogicNodeKind::SetPosition => {
                let eid = Self::compile_data_input(graph, node.id, 0)?;
                let x = Self::compile_data_input(graph, node.id, 1)?;
                let y = Self::compile_data_input(graph, node.id, 2)?;
                let z = Self::compile_data_input(graph, node.id, 3)?;
                writeln!(out, "{}quasar.set_position({}, {}, {}, {})", pad, eid, x, y, z).unwrap();
                Self::compile_exec_chain(graph, node.id, 0, out, indent)?;
            }
            LogicNodeKind::ApplyForce => {
                let eid = Self::compile_data_input(graph, node.id, 0)?;
                let x = Self::compile_data_input(graph, node.id, 1)?;
                let y = Self::compile_data_input(graph, node.id, 2)?;
                let z = Self::compile_data_input(graph, node.id, 3)?;
                writeln!(out, "{}quasar.apply_force({}, {}, {}, {})", pad, eid, x, y, z).unwrap();
                Self::compile_exec_chain(graph, node.id, 0, out, indent)?;
            }
            LogicNodeKind::PlayAudio => {
                let path = Self::compile_data_input(graph, node.id, 0)?;
                writeln!(out, "{}quasar.play_audio({})", pad, path).unwrap();
                Self::compile_exec_chain(graph, node.id, 0, out, indent)?;
            }
            LogicNodeKind::SpawnEntity => {
                writeln!(out, "{}quasar.spawn()", pad).unwrap();
                Self::compile_exec_chain(graph, node.id, 0, out, indent)?;
            }
            LogicNodeKind::DespawnEntity => {
                let eid = Self::compile_data_input(graph, node.id, 0)?;
                writeln!(out, "{}quasar.despawn({})", pad, eid).unwrap();
                Self::compile_exec_chain(graph, node.id, 0, out, indent)?;
            }
            LogicNodeKind::SetComponent { component, field } => {
                let eid = Self::compile_data_input(graph, node.id, 0)?;
                let val = Self::compile_data_input(graph, node.id, 1)?;
                writeln!(out, "{}quasar.add_component({}, \"{}\", {{ {} = {} }})", pad, eid, component, field, val).unwrap();
                Self::compile_exec_chain(graph, node.id, 0, out, indent)?;
            }
            LogicNodeKind::Branch => {
                let cond = Self::compile_data_input(graph, node.id, 0)?;
                writeln!(out, "{}if {} then", pad, cond).unwrap();
                Self::compile_exec_chain(graph, node.id, 0, out, indent + 1)?;
                writeln!(out, "{}else", pad).unwrap();
                Self::compile_exec_chain(graph, node.id, 1, out, indent + 1)?;
                writeln!(out, "{}end", pad).unwrap();
            }
            LogicNodeKind::ForEach { component } => {
                writeln!(out, "{}for _, _row in ipairs(quasar.query(\"{}\")) do", pad, component).unwrap();
                writeln!(out, "{}  local _foreach_entity = _row.entity", pad).unwrap();
                Self::compile_exec_chain(graph, node.id, 0, out, indent + 1)?;
                writeln!(out, "{}end", pad).unwrap();
                Self::compile_exec_chain(graph, node.id, 1, out, indent)?;
            }
            LogicNodeKind::Sequence { count } => {
                for i in 0..*count {
                    Self::compile_exec_chain(graph, node.id, i, out, indent)?;
                }
            }
            LogicNodeKind::SetVariable { name } => {
                let val = Self::compile_data_input(graph, node.id, 0)?;
                writeln!(out, "{}_state[\"{}\"] = {}", pad, name, val).unwrap();
                Self::compile_exec_chain(graph, node.id, 0, out, indent)?;
            }
            _ => {
                // Pure data nodes are not compiled as exec — they are inlined.
            }
        }
        Ok(())
    }

    /// Compile a data input expression by tracing back through data connections.
    fn compile_data_input(graph: &LogicGraph, node_id: NodeId, slot: SlotIndex) -> Result<String, String> {
        if let Some(conn) = graph.find_data_input(node_id, slot) {
            let source = graph.node(conn.from_node)
                .ok_or_else(|| format!("Missing source node {}", conn.from_node))?;
            Self::compile_data_expr(graph, source, conn.from_slot)
        } else {
            Ok("nil".to_string())
        }
    }

    /// Compile a data-producing node as an inline expression.
    fn compile_data_expr(graph: &LogicGraph, node: &LogicNode, output_slot: SlotIndex) -> Result<String, String> {
        match &node.kind {
            LogicNodeKind::FloatLiteral(v) => Ok(format!("{}", v)),
            LogicNodeKind::StringLiteral(s) => Ok(format!("\"{}\"", s)),
            LogicNodeKind::BoolLiteral(b) => Ok(if *b { "true".into() } else { "false".into() }),
            LogicNodeKind::SelfEntity => Ok("entity_id".into()),
            LogicNodeKind::OnUpdate => Ok("dt".into()),
            LogicNodeKind::ForEach { .. } => Ok("_foreach_entity".into()),
            LogicNodeKind::GetVariable { name } => Ok(format!("_state[\"{}\"]", name)),
            LogicNodeKind::GetComponent { component, field } => {
                let eid = Self::compile_data_input(graph, node.id, 0)?;
                Ok(format!("(quasar.query(\"{}\")[{}] or {{}}).{}", component, eid, field))
            }
            LogicNodeKind::Add => {
                let a = Self::compile_data_input(graph, node.id, 0)?;
                let b = Self::compile_data_input(graph, node.id, 1)?;
                Ok(format!("({} + {})", a, b))
            }
            LogicNodeKind::Subtract => {
                let a = Self::compile_data_input(graph, node.id, 0)?;
                let b = Self::compile_data_input(graph, node.id, 1)?;
                Ok(format!("({} - {})", a, b))
            }
            LogicNodeKind::Multiply => {
                let a = Self::compile_data_input(graph, node.id, 0)?;
                let b = Self::compile_data_input(graph, node.id, 1)?;
                Ok(format!("({} * {})", a, b))
            }
            LogicNodeKind::Divide => {
                let a = Self::compile_data_input(graph, node.id, 0)?;
                let b = Self::compile_data_input(graph, node.id, 1)?;
                Ok(format!("({} / {})", a, b))
            }
            LogicNodeKind::GreaterThan => {
                let a = Self::compile_data_input(graph, node.id, 0)?;
                let b = Self::compile_data_input(graph, node.id, 1)?;
                Ok(format!("({} > {})", a, b))
            }
            LogicNodeKind::LessThan => {
                let a = Self::compile_data_input(graph, node.id, 0)?;
                let b = Self::compile_data_input(graph, node.id, 1)?;
                Ok(format!("({} < {})", a, b))
            }
            LogicNodeKind::Equals => {
                let a = Self::compile_data_input(graph, node.id, 0)?;
                let b = Self::compile_data_input(graph, node.id, 1)?;
                Ok(format!("({} == {})", a, b))
            }
            LogicNodeKind::And => {
                let a = Self::compile_data_input(graph, node.id, 0)?;
                let b = Self::compile_data_input(graph, node.id, 1)?;
                Ok(format!("({} and {})", a, b))
            }
            LogicNodeKind::Or => {
                let a = Self::compile_data_input(graph, node.id, 0)?;
                let b = Self::compile_data_input(graph, node.id, 1)?;
                Ok(format!("({} or {})", a, b))
            }
            LogicNodeKind::Not => {
                let a = Self::compile_data_input(graph, node.id, 0)?;
                Ok(format!("(not {})", a))
            }
            LogicNodeKind::Vec3Construct => {
                let x = Self::compile_data_input(graph, node.id, 0)?;
                let y = Self::compile_data_input(graph, node.id, 1)?;
                let z = Self::compile_data_input(graph, node.id, 2)?;
                match output_slot {
                    0 => Ok(x),
                    1 => Ok(y),
                    2 => Ok(z),
                    _ => Ok("nil".into()),
                }
            }
            _ => Ok("nil".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compile_simple_on_update_print() {
        let mut graph = LogicGraph::new("test");
        let on_update = graph.add_node(LogicNodeKind::OnUpdate);
        let msg = graph.add_node(LogicNodeKind::StringLiteral("hello".into()));
        let print = graph.add_node(LogicNodeKind::Print);

        // on_update exec→ print
        graph.connect(ConnectionKind::Exec, on_update, 0, print, 0);
        // msg data→ print input 0
        graph.connect(ConnectionKind::Data, msg, 0, print, 0);

        let lua = LogicGraphCompiler::compile(&graph).unwrap();
        assert!(lua.contains("quasar.log"));
        assert!(lua.contains("\"hello\""));
    }

    #[test]
    fn compile_branch() {
        let mut graph = LogicGraph::new("branch_test");
        let on_update = graph.add_node(LogicNodeKind::OnUpdate);
        let cond = graph.add_node(LogicNodeKind::BoolLiteral(true));
        let branch = graph.add_node(LogicNodeKind::Branch);
        let print_t = graph.add_node(LogicNodeKind::Print);
        let print_f = graph.add_node(LogicNodeKind::Print);
        let msg_t = graph.add_node(LogicNodeKind::StringLiteral("yes".into()));
        let msg_f = graph.add_node(LogicNodeKind::StringLiteral("no".into()));

        graph.connect(ConnectionKind::Exec, on_update, 0, branch, 0);
        graph.connect(ConnectionKind::Data, cond, 0, branch, 0);
        graph.connect(ConnectionKind::Exec, branch, 0, print_t, 0);
        graph.connect(ConnectionKind::Exec, branch, 1, print_f, 0);
        graph.connect(ConnectionKind::Data, msg_t, 0, print_t, 0);
        graph.connect(ConnectionKind::Data, msg_f, 0, print_f, 0);

        let lua = LogicGraphCompiler::compile(&graph).unwrap();
        assert!(lua.contains("if true then"));
        assert!(lua.contains("\"yes\""));
        assert!(lua.contains("\"no\""));
    }
}
