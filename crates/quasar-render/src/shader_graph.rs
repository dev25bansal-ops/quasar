//! Shader graph — node-based visual material authoring.
//!
//! Provides:
//! - [`ShaderNode`] / [`ShaderNodeKind`]: individual graph nodes (texture sample,
//!   math, fresnel, lerp, if, constants, UV, etc.).
//! - [`ShaderGraph`]: directed acyclic graph of nodes with typed connections.
//! - [`ShaderGraphCompiler`]: compiles the graph into a WGSL string that can be
//!   fed into `wgpu::Device::create_shader_module`.
//! - [`ShaderGraphCache`]: caches compiled WGSL by graph content hash.
//!
//! The editor panel can build a `ShaderGraph`, and the asset pipeline calls
//! `ShaderGraphCompiler::compile()` to produce the final WGSL at import time.

use std::collections::HashMap;
use std::fmt::Write;

// ── Node types ─────────────────────────────────────────────────────

/// Unique node identifier within a graph.
pub type NodeId = u32;
/// Slot index on a node (both input & output).
pub type SlotIndex = u32;

/// The kinds of node available in the shader graph.
#[derive(Debug, Clone, PartialEq)]
pub enum ShaderNodeKind {
    // ─── Inputs ───
    /// Mesh UV coordinate set (index).
    TexCoord { set: u32 },
    /// Camera-space position of the fragment.
    WorldPosition,
    /// World-space normal.
    WorldNormal,
    /// View direction (camera → fragment).
    ViewDirection,
    /// Time (seconds) from engine clock.
    Time,

    // ─── Constants ───
    ConstFloat(f32),
    ConstVec2([f32; 2]),
    ConstVec3([f32; 3]),
    ConstVec4([f32; 4]),
    ConstColor([f32; 4]),

    // ─── Texture ───
    /// Sample a 2D texture. Input 0 = UV (vec2).
    TextureSample { binding_slot: u32 },

    // ─── Math ───
    Add,
    Subtract,
    Multiply,
    Divide,
    /// Component-wise power.
    Power,
    /// Square root.
    Sqrt,
    /// Absolute value.
    Abs,
    /// Clamp(value, min, max). Inputs: 0=value, 1=min, 2=max.
    Clamp,
    /// Dot product of two vectors.
    Dot,
    /// Cross product of two vec3s.
    Cross,
    /// Normalise a vector.
    Normalize,
    /// Length of a vector.
    Length,
    /// Saturate (clamp 0..1).
    Saturate,
    /// Negate.
    Negate,
    /// One minus value.
    OneMinus,

    // ─── Interpolation ───
    /// Lerp(a, b, t). Inputs: 0=a, 1=b, 2=t.
    Lerp,
    /// Smoothstep(edge0, edge1, x). Inputs: 0=edge0, 1=edge1, 2=x.
    Smoothstep,

    // ─── Comparison / branch ───
    /// If(condition > threshold) → A else B.
    /// Inputs: 0=condition(f32), 1=threshold(f32), 2=A, 3=B.
    If,

    // ─── Fresnel ───
    /// Schlick Fresnel. Inputs: 0=normal, 1=view, 2=exponent(f32).
    Fresnel,

    // ─── Split / combine ───
    /// Split vec4 → (x, y, z, w).
    SplitVec4,
    /// Combine (x, y, z, w) → vec4.
    CombineVec4,

    // ─── Material outputs ───
    /// The final PBR output — inputs map to base_color, normal, roughness,
    /// metallic, emissive, alpha.
    PbrOutput,
}

// ── Node ───────────────────────────────────────────────────────────

/// A single node in the shader graph.
#[derive(Debug, Clone)]
pub struct ShaderNode {
    pub id: NodeId,
    pub kind: ShaderNodeKind,
    /// Human-readable label (used in the editor).
    pub label: String,
    /// Position in the editor canvas (pixels).
    pub editor_pos: [f32; 2],
}

impl ShaderNode {
    pub fn new(id: NodeId, kind: ShaderNodeKind) -> Self {
        let label = format!("{:?}", kind);
        Self {
            id,
            kind,
            label,
            editor_pos: [0.0, 0.0],
        }
    }

    /// Number of input slots this node exposes.
    pub fn input_count(&self) -> u32 {
        match &self.kind {
            ShaderNodeKind::TexCoord { .. }
            | ShaderNodeKind::WorldPosition
            | ShaderNodeKind::WorldNormal
            | ShaderNodeKind::ViewDirection
            | ShaderNodeKind::Time
            | ShaderNodeKind::ConstFloat(_)
            | ShaderNodeKind::ConstVec2(_)
            | ShaderNodeKind::ConstVec3(_)
            | ShaderNodeKind::ConstVec4(_)
            | ShaderNodeKind::ConstColor(_) => 0,

            ShaderNodeKind::TextureSample { .. } => 1, // uv
            ShaderNodeKind::Add
            | ShaderNodeKind::Subtract
            | ShaderNodeKind::Multiply
            | ShaderNodeKind::Divide
            | ShaderNodeKind::Power
            | ShaderNodeKind::Dot
            | ShaderNodeKind::Cross => 2,

            ShaderNodeKind::Sqrt
            | ShaderNodeKind::Abs
            | ShaderNodeKind::Normalize
            | ShaderNodeKind::Length
            | ShaderNodeKind::Saturate
            | ShaderNodeKind::Negate
            | ShaderNodeKind::OneMinus
            | ShaderNodeKind::SplitVec4 => 1,

            ShaderNodeKind::Clamp
            | ShaderNodeKind::Lerp
            | ShaderNodeKind::Smoothstep
            | ShaderNodeKind::Fresnel => 3,

            ShaderNodeKind::If => 4,
            ShaderNodeKind::CombineVec4 => 4,
            ShaderNodeKind::PbrOutput => 6, // base_color, normal, roughness, metallic, emissive, alpha
        }
    }

    /// Number of output slots this node exposes.
    pub fn output_count(&self) -> u32 {
        match &self.kind {
            ShaderNodeKind::PbrOutput => 0,
            ShaderNodeKind::SplitVec4 => 4,
            _ => 1,
        }
    }
}

// ── Connection ─────────────────────────────────────────────────────

/// A directed edge from one node's output slot to another's input slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShaderConnection {
    pub from_node: NodeId,
    pub from_slot: SlotIndex,
    pub to_node: NodeId,
    pub to_slot: SlotIndex,
}

// ── Graph ──────────────────────────────────────────────────────────

/// The shader graph — a DAG of [`ShaderNode`]s connected by [`ShaderConnection`]s.
#[derive(Debug, Clone)]
pub struct ShaderGraph {
    pub name: String,
    pub nodes: Vec<ShaderNode>,
    pub connections: Vec<ShaderConnection>,
    next_id: NodeId,
}

impl ShaderGraph {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            nodes: Vec::new(),
            connections: Vec::new(),
            next_id: 0,
        }
    }

    /// Add a node and return its ID.
    pub fn add_node(&mut self, kind: ShaderNodeKind) -> NodeId {
        let id = self.next_id;
        self.next_id += 1;
        self.nodes.push(ShaderNode::new(id, kind));
        id
    }

    /// Connect `from_node:from_slot` → `to_node:to_slot`.
    pub fn connect(
        &mut self,
        from_node: NodeId,
        from_slot: SlotIndex,
        to_node: NodeId,
        to_slot: SlotIndex,
    ) {
        self.connections.push(ShaderConnection {
            from_node,
            from_slot,
            to_node,
            to_slot,
        });
    }

    /// Find the node that drives a particular input slot.
    pub fn find_input(&self, node_id: NodeId, slot: SlotIndex) -> Option<&ShaderConnection> {
        self.connections
            .iter()
            .find(|c| c.to_node == node_id && c.to_slot == slot)
    }

    /// Look up a node by ID.
    pub fn node(&self, id: NodeId) -> Option<&ShaderNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// Find the PBR output node.
    pub fn output_node(&self) -> Option<&ShaderNode> {
        self.nodes
            .iter()
            .find(|n| matches!(n.kind, ShaderNodeKind::PbrOutput))
    }

    /// Compute a content hash for cache keying.
    pub fn content_hash(&self) -> u64 {
        // Simple FNV-1a over the debug representation.
        let repr = format!("{:?}{:?}", self.nodes, self.connections);
        let mut h: u64 = 0xcbf29ce484222325;
        for b in repr.bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h
    }
}

// ── Compiler ───────────────────────────────────────────────────────

/// Compiles a [`ShaderGraph`] into a WGSL fragment shader string.
pub struct ShaderGraphCompiler;

impl ShaderGraphCompiler {
    /// Compile the given graph to WGSL. Returns `Err` if the graph is
    /// malformed (e.g. missing PBR output, cycles).
    pub fn compile(graph: &ShaderGraph) -> Result<String, String> {
        let output = graph
            .output_node()
            .ok_or_else(|| "No PbrOutput node found".to_string())?;

        let mut wgsl = String::with_capacity(4096);

        // Header
        writeln!(wgsl, "// Auto-generated by ShaderGraphCompiler").unwrap();
        writeln!(wgsl, "// Graph: {}", graph.name).unwrap();
        writeln!(wgsl).unwrap();

        // Collect texture bindings.
        let mut texture_slots: Vec<u32> = Vec::new();
        for node in &graph.nodes {
            if let ShaderNodeKind::TextureSample { binding_slot } = &node.kind {
                if !texture_slots.contains(binding_slot) {
                    texture_slots.push(*binding_slot);
                }
            }
        }
        texture_slots.sort();

        for (i, slot) in texture_slots.iter().enumerate() {
            writeln!(
                wgsl,
                "@group(2) @binding({}) var tex_{}: texture_2d<f32>;",
                i * 2,
                slot
            )
            .unwrap();
            writeln!(
                wgsl,
                "@group(2) @binding({}) var samp_{}: sampler;",
                i * 2 + 1,
                slot
            )
            .unwrap();
        }
        writeln!(wgsl).unwrap();

        // Emit struct + entry.
        writeln!(wgsl, "struct MaterialInput {{").unwrap();
        writeln!(wgsl, "    @location(0) uv: vec2<f32>,").unwrap();
        writeln!(wgsl, "    @location(1) world_pos: vec3<f32>,").unwrap();
        writeln!(wgsl, "    @location(2) world_normal: vec3<f32>,").unwrap();
        writeln!(wgsl, "    @location(3) view_dir: vec3<f32>,").unwrap();
        writeln!(wgsl, "}};").unwrap();
        writeln!(wgsl).unwrap();
        writeln!(wgsl, "struct PbrOutput {{").unwrap();
        writeln!(wgsl, "    base_color: vec4<f32>,").unwrap();
        writeln!(wgsl, "    normal: vec3<f32>,").unwrap();
        writeln!(wgsl, "    roughness: f32,").unwrap();
        writeln!(wgsl, "    metallic: f32,").unwrap();
        writeln!(wgsl, "    emissive: vec3<f32>,").unwrap();
        writeln!(wgsl, "}};").unwrap();
        writeln!(wgsl).unwrap();

        // Inline helper functions.
        writeln!(wgsl, "fn fresnel_schlick(cos_theta: f32, exp: f32) -> f32 {{").unwrap();
        writeln!(wgsl, "    return pow(1.0 - cos_theta, exp);").unwrap();
        writeln!(wgsl, "}}").unwrap();
        writeln!(wgsl).unwrap();

        // Main function — evaluate nodes in topological order.
        writeln!(wgsl, "fn evaluate_material(input: MaterialInput) -> PbrOutput {{").unwrap();

        let order = Self::topological_order(graph)?;

        // Variable names: node_<id>_<output_slot>
        let mut emitted: HashMap<NodeId, bool> = HashMap::new();

        for node_id in &order {
            let node = graph.node(*node_id).unwrap();
            Self::emit_node(&mut wgsl, graph, node, &emitted)?;
            emitted.insert(*node_id, true);
        }

        // Wire PBR output.
        let out_id = output.id;
        let bc = Self::input_var(graph, out_id, 0, "vec4<f32>(1.0, 1.0, 1.0, 1.0)");
        let nm = Self::input_var(graph, out_id, 1, "input.world_normal");
        let ro = Self::input_var(graph, out_id, 2, "0.5");
        let me = Self::input_var(graph, out_id, 3, "0.0");
        let em = Self::input_var(graph, out_id, 4, "vec3<f32>(0.0)");
        let _al = Self::input_var(graph, out_id, 5, "1.0");

        writeln!(wgsl, "    var out: PbrOutput;").unwrap();
        writeln!(wgsl, "    out.base_color = {};", bc).unwrap();
        writeln!(wgsl, "    out.normal     = {};", nm).unwrap();
        writeln!(wgsl, "    out.roughness  = {};", ro).unwrap();
        writeln!(wgsl, "    out.metallic   = {};", me).unwrap();
        writeln!(wgsl, "    out.emissive   = {};", em).unwrap();
        writeln!(wgsl, "    return out;").unwrap();
        writeln!(wgsl, "}}").unwrap();
        writeln!(wgsl).unwrap();

        // Emit the @fragment entry point that calls evaluate_material.
        writeln!(wgsl, "@fragment").unwrap();
        writeln!(wgsl, "fn fs_main(input: MaterialInput) -> @location(0) vec4<f32> {{").unwrap();
        writeln!(wgsl, "    let mat = evaluate_material(input);").unwrap();
        writeln!(wgsl, "    return mat.base_color;").unwrap();
        writeln!(wgsl, "}}").unwrap();

        Ok(wgsl)
    }

    /// Get the WGSL variable name for a node's input slot, or a default literal.
    fn input_var(graph: &ShaderGraph, node_id: NodeId, slot: SlotIndex, default: &str) -> String {
        if let Some(conn) = graph.find_input(node_id, slot) {
            format!("node_{}_{}", conn.from_node, conn.from_slot)
        } else {
            default.to_string()
        }
    }

    /// Emit WGSL for a single node.
    fn emit_node(
        wgsl: &mut String,
        graph: &ShaderGraph,
        node: &ShaderNode,
        _emitted: &HashMap<NodeId, bool>,
    ) -> Result<(), String> {
        let id = node.id;
        let out = |slot: u32| -> String { format!("node_{}_{}", id, slot) };

        match &node.kind {
            // Inputs
            ShaderNodeKind::TexCoord { set } => {
                if *set == 0 {
                    writeln!(wgsl, "    let {} = input.uv;", out(0)).unwrap();
                } else {
                    writeln!(wgsl, "    let {} = input.uv;", out(0)).unwrap();
                }
            }
            ShaderNodeKind::WorldPosition => {
                writeln!(wgsl, "    let {} = input.world_pos;", out(0)).unwrap();
            }
            ShaderNodeKind::WorldNormal => {
                writeln!(wgsl, "    let {} = input.world_normal;", out(0)).unwrap();
            }
            ShaderNodeKind::ViewDirection => {
                writeln!(wgsl, "    let {} = input.view_dir;", out(0)).unwrap();
            }
            ShaderNodeKind::Time => {
                writeln!(wgsl, "    let {} = 0.0;", out(0)).unwrap();
            }

            // Constants
            ShaderNodeKind::ConstFloat(v) => {
                writeln!(wgsl, "    let {} = {:.6};", out(0), v).unwrap();
            }
            ShaderNodeKind::ConstVec2(v) => {
                writeln!(wgsl, "    let {} = vec2<f32>({:.6}, {:.6});", out(0), v[0], v[1])
                    .unwrap();
            }
            ShaderNodeKind::ConstVec3(v) => {
                writeln!(
                    wgsl,
                    "    let {} = vec3<f32>({:.6}, {:.6}, {:.6});",
                    out(0), v[0], v[1], v[2]
                )
                .unwrap();
            }
            ShaderNodeKind::ConstVec4(v) | ShaderNodeKind::ConstColor(v) => {
                writeln!(
                    wgsl,
                    "    let {} = vec4<f32>({:.6}, {:.6}, {:.6}, {:.6});",
                    out(0), v[0], v[1], v[2], v[3]
                )
                .unwrap();
            }

            // Texture sample
            ShaderNodeKind::TextureSample { binding_slot } => {
                let uv = Self::input_var(graph, id, 0, "input.uv");
                writeln!(
                    wgsl,
                    "    let {} = textureSample(tex_{}, samp_{}, {});",
                    out(0), binding_slot, binding_slot, uv
                )
                .unwrap();
            }

            // Binary math
            ShaderNodeKind::Add => {
                let a = Self::input_var(graph, id, 0, "0.0");
                let b = Self::input_var(graph, id, 1, "0.0");
                writeln!(wgsl, "    let {} = {} + {};", out(0), a, b).unwrap();
            }
            ShaderNodeKind::Subtract => {
                let a = Self::input_var(graph, id, 0, "0.0");
                let b = Self::input_var(graph, id, 1, "0.0");
                writeln!(wgsl, "    let {} = {} - {};", out(0), a, b).unwrap();
            }
            ShaderNodeKind::Multiply => {
                let a = Self::input_var(graph, id, 0, "1.0");
                let b = Self::input_var(graph, id, 1, "1.0");
                writeln!(wgsl, "    let {} = {} * {};", out(0), a, b).unwrap();
            }
            ShaderNodeKind::Divide => {
                let a = Self::input_var(graph, id, 0, "1.0");
                let b = Self::input_var(graph, id, 1, "1.0");
                writeln!(wgsl, "    let {} = {} / {};", out(0), a, b).unwrap();
            }
            ShaderNodeKind::Power => {
                let a = Self::input_var(graph, id, 0, "1.0");
                let b = Self::input_var(graph, id, 1, "1.0");
                writeln!(wgsl, "    let {} = pow({}, {});", out(0), a, b).unwrap();
            }
            ShaderNodeKind::Dot => {
                let a = Self::input_var(graph, id, 0, "vec3<f32>(0.0)");
                let b = Self::input_var(graph, id, 1, "vec3<f32>(0.0)");
                writeln!(wgsl, "    let {} = dot({}, {});", out(0), a, b).unwrap();
            }
            ShaderNodeKind::Cross => {
                let a = Self::input_var(graph, id, 0, "vec3<f32>(0.0)");
                let b = Self::input_var(graph, id, 1, "vec3<f32>(0.0)");
                writeln!(wgsl, "    let {} = cross({}, {});", out(0), a, b).unwrap();
            }

            // Unary math
            ShaderNodeKind::Sqrt => {
                let a = Self::input_var(graph, id, 0, "0.0");
                writeln!(wgsl, "    let {} = sqrt({});", out(0), a).unwrap();
            }
            ShaderNodeKind::Abs => {
                let a = Self::input_var(graph, id, 0, "0.0");
                writeln!(wgsl, "    let {} = abs({});", out(0), a).unwrap();
            }
            ShaderNodeKind::Normalize => {
                let a = Self::input_var(graph, id, 0, "vec3<f32>(0.0, 1.0, 0.0)");
                writeln!(wgsl, "    let {} = normalize({});", out(0), a).unwrap();
            }
            ShaderNodeKind::Length => {
                let a = Self::input_var(graph, id, 0, "vec3<f32>(0.0)");
                writeln!(wgsl, "    let {} = length({});", out(0), a).unwrap();
            }
            ShaderNodeKind::Saturate => {
                let a = Self::input_var(graph, id, 0, "0.0");
                writeln!(wgsl, "    let {} = clamp({}, 0.0, 1.0);", out(0), a).unwrap();
            }
            ShaderNodeKind::Negate => {
                let a = Self::input_var(graph, id, 0, "0.0");
                writeln!(wgsl, "    let {} = -({});", out(0), a).unwrap();
            }
            ShaderNodeKind::OneMinus => {
                let a = Self::input_var(graph, id, 0, "0.0");
                writeln!(wgsl, "    let {} = 1.0 - ({});", out(0), a).unwrap();
            }

            // Clamp / lerp / smoothstep
            ShaderNodeKind::Clamp => {
                let v = Self::input_var(graph, id, 0, "0.0");
                let lo = Self::input_var(graph, id, 1, "0.0");
                let hi = Self::input_var(graph, id, 2, "1.0");
                writeln!(wgsl, "    let {} = clamp({}, {}, {});", out(0), v, lo, hi).unwrap();
            }
            ShaderNodeKind::Lerp => {
                let a = Self::input_var(graph, id, 0, "0.0");
                let b = Self::input_var(graph, id, 1, "1.0");
                let t = Self::input_var(graph, id, 2, "0.5");
                writeln!(wgsl, "    let {} = mix({}, {}, {});", out(0), a, b, t).unwrap();
            }
            ShaderNodeKind::Smoothstep => {
                let e0 = Self::input_var(graph, id, 0, "0.0");
                let e1 = Self::input_var(graph, id, 1, "1.0");
                let x = Self::input_var(graph, id, 2, "0.5");
                writeln!(wgsl, "    let {} = smoothstep({}, {}, {});", out(0), e0, e1, x)
                    .unwrap();
            }

            // Fresnel
            ShaderNodeKind::Fresnel => {
                let n = Self::input_var(graph, id, 0, "input.world_normal");
                let v = Self::input_var(graph, id, 1, "input.view_dir");
                let exp = Self::input_var(graph, id, 2, "5.0");
                writeln!(
                    wgsl,
                    "    let {} = fresnel_schlick(max(dot({}, {}), 0.0), {});",
                    out(0), n, v, exp
                )
                .unwrap();
            }

            // If
            ShaderNodeKind::If => {
                let cond = Self::input_var(graph, id, 0, "0.0");
                let thresh = Self::input_var(graph, id, 1, "0.5");
                let a = Self::input_var(graph, id, 2, "1.0");
                let b = Self::input_var(graph, id, 3, "0.0");
                writeln!(
                    wgsl,
                    "    let {} = select({}, {}, {} > {});",
                    out(0), b, a, cond, thresh
                )
                .unwrap();
            }

            // Split / Combine
            ShaderNodeKind::SplitVec4 => {
                let v = Self::input_var(graph, id, 0, "vec4<f32>(0.0)");
                writeln!(wgsl, "    let {}_v = {};", out(0), v).unwrap();
                writeln!(wgsl, "    let {} = {}_v.x;", out(0), out(0)).unwrap();
                writeln!(wgsl, "    let {} = {}_v.y;", format!("node_{}_{}", id, 1), out(0)).unwrap();
                writeln!(wgsl, "    let {} = {}_v.z;", format!("node_{}_{}", id, 2), out(0)).unwrap();
                writeln!(wgsl, "    let {} = {}_v.w;", format!("node_{}_{}", id, 3), out(0)).unwrap();
            }
            ShaderNodeKind::CombineVec4 => {
                let x = Self::input_var(graph, id, 0, "0.0");
                let y = Self::input_var(graph, id, 1, "0.0");
                let z = Self::input_var(graph, id, 2, "0.0");
                let w = Self::input_var(graph, id, 3, "1.0");
                writeln!(
                    wgsl,
                    "    let {} = vec4<f32>({}, {}, {}, {});",
                    out(0), x, y, z, w
                )
                .unwrap();
            }

            // PBR output handled in the caller
            ShaderNodeKind::PbrOutput => {}
        }

        Ok(())
    }

    /// Kahn's algorithm for topological sort.
    fn topological_order(graph: &ShaderGraph) -> Result<Vec<NodeId>, String> {
        let ids: Vec<NodeId> = graph.nodes.iter().map(|n| n.id).collect();
        let mut in_degree: HashMap<NodeId, u32> = ids.iter().map(|id| (*id, 0)).collect();

        for conn in &graph.connections {
            *in_degree.entry(conn.to_node).or_insert(0) += 1;
        }

        let mut queue: Vec<NodeId> = ids
            .iter()
            .filter(|id| in_degree[*id] == 0)
            .copied()
            .collect();

        let mut order = Vec::with_capacity(ids.len());

        while let Some(node_id) = queue.pop() {
            order.push(node_id);
            for conn in &graph.connections {
                if conn.from_node == node_id {
                    let deg = in_degree.get_mut(&conn.to_node).unwrap();
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push(conn.to_node);
                    }
                }
            }
        }

        if order.len() != ids.len() {
            return Err("Cycle detected in shader graph".to_string());
        }

        Ok(order)
    }
}

// ── Cache ──────────────────────────────────────────────────────────

/// Caches compiled WGSL by graph content hash.
pub struct ShaderGraphCache {
    entries: HashMap<u64, String>,
}

impl ShaderGraphCache {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Get or compile the WGSL for a graph.
    pub fn get_or_compile(&mut self, graph: &ShaderGraph) -> Result<String, String> {
        let hash = graph.content_hash();
        if let Some(cached) = self.entries.get(&hash) {
            return Ok(cached.clone());
        }
        let wgsl = ShaderGraphCompiler::compile(graph)?;
        self.entries.insert(hash, wgsl.clone());
        Ok(wgsl)
    }

    /// Invalidate all cached entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

impl Default for ShaderGraphCache {
    fn default() -> Self {
        Self::new()
    }
}
