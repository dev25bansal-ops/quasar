//! Shader graph â€” node-based visual material authoring.
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

// â”€â”€ Validation / diagnostics â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// A single diagnostic message from graph validation or compilation.
#[derive(Debug, Clone)]
pub struct ShaderGraphDiagnostic {
    /// The node that caused the issue (if applicable).
    pub node_id: Option<NodeId>,
    pub severity: DiagnosticSeverity,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
}

/// Result of a live-preview compilation attempt.
#[derive(Debug, Clone)]
pub struct CompileResult {
    /// The generated WGSL (empty if compilation failed).
    pub wgsl: String,
    /// All diagnostics collected during compilation.
    pub diagnostics: Vec<ShaderGraphDiagnostic>,
    /// Whether compilation succeeded (no errors).
    pub success: bool,
}

// â”€â”€ Node types â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Unique node identifier within a graph.
pub type NodeId = u32;
/// Slot index on a node (both input & output).
pub type SlotIndex = u32;

/// The kinds of node available in the shader graph.
#[derive(Debug, Clone, PartialEq)]
pub enum ShaderNodeKind {
    // â”€â”€â”€ Inputs â”€â”€â”€
    /// Mesh UV coordinate set (index).
    TexCoord {
        set: u32,
    },
    /// Camera-space position of the fragment.
    WorldPosition,
    /// World-space normal.
    WorldNormal,
    /// View direction (camera â†’ fragment).
    ViewDirection,
    /// Time (seconds) from engine clock.
    Time,

    // â”€â”€â”€ Constants â”€â”€â”€
    ConstFloat(f32),
    ConstVec2([f32; 2]),
    ConstVec3([f32; 3]),
    ConstVec4([f32; 4]),
    ConstColor([f32; 4]),

    // â”€â”€â”€ Texture â”€â”€â”€
    /// Sample a 2D texture. Input 0 = UV (vec2).
    TextureSample {
        binding_slot: u32,
    },

    // â”€â”€â”€ Math â”€â”€â”€
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

    // â”€â”€â”€ Interpolation â”€â”€â”€
    /// Lerp(a, b, t). Inputs: 0=a, 1=b, 2=t.
    Lerp,
    /// Smoothstep(edge0, edge1, x). Inputs: 0=edge0, 1=edge1, 2=x.
    Smoothstep,

    // â”€â”€â”€ Comparison / branch â”€â”€â”€
    /// If(condition > threshold) â†’ A else B.
    /// Inputs: 0=condition(f32), 1=threshold(f32), 2=A, 3=B.
    If,

    // â”€â”€â”€ Fresnel â”€â”€â”€
    /// Schlick Fresnel. Inputs: 0=normal, 1=view, 2=exponent(f32).
    Fresnel,

    // â”€â”€â”€ Split / combine â”€â”€â”€
    /// Split vec4 â†’ (x, y, z, w).
    SplitVec4,
    /// Combine (x, y, z, w) â†’ vec4.
    CombineVec4,

    // â”€â”€â”€ Material outputs â”€â”€â”€
    /// The final PBR output â€” inputs map to base_color, normal, roughness,
    /// metallic, emissive, alpha.
    PbrOutput,
}

// â”€â”€ Node â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

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

// â”€â”€ Connection â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// A directed edge from one node's output slot to another's input slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShaderConnection {
    pub from_node: NodeId,
    pub from_slot: SlotIndex,
    pub to_node: NodeId,
    pub to_slot: SlotIndex,
}

// â”€â”€ Graph â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// The shader graph â€” a DAG of [`ShaderNode`]s connected by [`ShaderConnection`]s.
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

    /// Connect `from_node:from_slot` â†’ `to_node:to_slot`.
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

// â”€â”€ Compiler â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

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
        let _ = writeln!(wgsl, "// Auto-generated by ShaderGraphCompiler");
        let _ = writeln!(wgsl, "// Graph: {}", graph.name);
        let _ = writeln!(wgsl);

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
            let _ = writeln!(
                wgsl,
                "@group(2) @binding({}) var tex_{}: texture_2d<f32>;",
                i * 2,
                slot
            );
            let _ = writeln!(
                wgsl,
                "@group(2) @binding({}) var samp_{}: sampler;",
                i * 2 + 1,
                slot
            );
        }
        let _ = writeln!(wgsl);

        // Emit struct + entry.
        let _ = writeln!(wgsl, "struct MaterialInput {{");
        let _ = writeln!(wgsl, "    @location(0) uv: vec2<f32>,");
        let _ = writeln!(wgsl, "    @location(1) world_pos: vec3<f32>,");
        let _ = writeln!(wgsl, "    @location(2) world_normal: vec3<f32>,");
        let _ = writeln!(wgsl, "    @location(3) view_dir: vec3<f32>,");
        let _ = writeln!(wgsl, "}};");
        let _ = writeln!(wgsl);
        let _ = writeln!(wgsl, "struct PbrOutput {{");
        let _ = writeln!(wgsl, "    base_color: vec4<f32>,");
        let _ = writeln!(wgsl, "    normal: vec3<f32>,");
        let _ = writeln!(wgsl, "    roughness: f32,");
        let _ = writeln!(wgsl, "    metallic: f32,");
        let _ = writeln!(wgsl, "    emissive: vec3<f32>,");
        let _ = writeln!(wgsl, "}};");
        let _ = writeln!(wgsl);

        // Inline helper functions.
        let _ = writeln!(
            wgsl,
            "fn fresnel_schlick(cos_theta: f32, exp: f32) -> f32 {{"
        );
        let _ = writeln!(wgsl, "    return pow(1.0 - cos_theta, exp);");
        let _ = writeln!(wgsl, "}}");
        let _ = writeln!(wgsl);

        // Main function â€” evaluate nodes in topological order.
        let _ = writeln!(
            wgsl,
            "fn evaluate_material(input: MaterialInput) -> PbrOutput {{"
        );

        let order = Self::topological_order(graph)?;

        // Variable names: node_<id>_<output_slot>
        let mut emitted: HashMap<NodeId, bool> = HashMap::new();

        for node_id in &order {
            let Some(node) = graph.node(*node_id) else {
                continue;
            };
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

        let _ = writeln!(wgsl, "    var out: PbrOutput;");
        let _ = writeln!(wgsl, "    out.base_color = {};", bc);
        let _ = writeln!(wgsl, "    out.normal     = {};", nm);
        let _ = writeln!(wgsl, "    out.roughness  = {};", ro);
        let _ = writeln!(wgsl, "    out.metallic   = {};", me);
        let _ = writeln!(wgsl, "    out.emissive   = {};", em);
        let _ = writeln!(wgsl, "    return out;");
        let _ = writeln!(wgsl, "}}");
        let _ = writeln!(wgsl);

        // Emit the @fragment entry point that calls evaluate_material.
        let _ = writeln!(wgsl, "@fragment");
        let _ = writeln!(
            wgsl,
            "fn fs_main(input: MaterialInput) -> @location(0) vec4<f32> {{"
        );
        let _ = writeln!(wgsl, "    let mat = evaluate_material(input);");
        let _ = writeln!(wgsl, "    return mat.base_color;");
        let _ = writeln!(wgsl, "}}");

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
            ShaderNodeKind::TexCoord { set: _ } => {
                let _ = writeln!(wgsl, " let {} = input.uv;", out(0));
            }
            ShaderNodeKind::WorldPosition => {
                let _ = writeln!(wgsl, "    let {} = input.world_pos;", out(0));
            }
            ShaderNodeKind::WorldNormal => {
                let _ = writeln!(wgsl, "    let {} = input.world_normal;", out(0));
            }
            ShaderNodeKind::ViewDirection => {
                let _ = writeln!(wgsl, "    let {} = input.view_dir;", out(0));
            }
            ShaderNodeKind::Time => {
                let _ = writeln!(wgsl, "    let {} = 0.0;", out(0));
            }

            // Constants
            ShaderNodeKind::ConstFloat(v) => {
                let _ = writeln!(wgsl, "    let {} = {:.6};", out(0), v);
            }
            ShaderNodeKind::ConstVec2(v) => {
                let _ = writeln!(
                    wgsl,
                    "    let {} = vec2<f32>({:.6}, {:.6});",
                    out(0),
                    v[0],
                    v[1]
                );
            }
            ShaderNodeKind::ConstVec3(v) => {
                let _ = writeln!(
                    wgsl,
                    "    let {} = vec3<f32>({:.6}, {:.6}, {:.6});",
                    out(0),
                    v[0],
                    v[1],
                    v[2]
                );
            }
            ShaderNodeKind::ConstVec4(v) | ShaderNodeKind::ConstColor(v) => {
                let _ = writeln!(
                    wgsl,
                    "    let {} = vec4<f32>({:.6}, {:.6}, {:.6}, {:.6});",
                    out(0),
                    v[0],
                    v[1],
                    v[2],
                    v[3]
                );
            }

            // Texture sample
            ShaderNodeKind::TextureSample { binding_slot } => {
                let uv = Self::input_var(graph, id, 0, "input.uv");
                let _ = writeln!(
                    wgsl,
                    "    let {} = textureSample(tex_{}, samp_{}, {});",
                    out(0),
                    binding_slot,
                    binding_slot,
                    uv
                );
            }

            // Binary math
            ShaderNodeKind::Add => {
                let a = Self::input_var(graph, id, 0, "0.0");
                let b = Self::input_var(graph, id, 1, "0.0");
                let _ = writeln!(wgsl, "    let {} = {} + {};", out(0), a, b);
            }
            ShaderNodeKind::Subtract => {
                let a = Self::input_var(graph, id, 0, "0.0");
                let b = Self::input_var(graph, id, 1, "0.0");
                let _ = writeln!(wgsl, "    let {} = {} - {};", out(0), a, b);
            }
            ShaderNodeKind::Multiply => {
                let a = Self::input_var(graph, id, 0, "1.0");
                let b = Self::input_var(graph, id, 1, "1.0");
                let _ = writeln!(wgsl, "    let {} = {} * {};", out(0), a, b);
            }
            ShaderNodeKind::Divide => {
                let a = Self::input_var(graph, id, 0, "1.0");
                let b = Self::input_var(graph, id, 1, "1.0");
                let _ = writeln!(wgsl, "    let {} = {} / {};", out(0), a, b);
            }
            ShaderNodeKind::Power => {
                let a = Self::input_var(graph, id, 0, "1.0");
                let b = Self::input_var(graph, id, 1, "1.0");
                let _ = writeln!(wgsl, "    let {} = pow({}, {});", out(0), a, b);
            }
            ShaderNodeKind::Dot => {
                let a = Self::input_var(graph, id, 0, "vec3<f32>(0.0)");
                let b = Self::input_var(graph, id, 1, "vec3<f32>(0.0)");
                let _ = writeln!(wgsl, "    let {} = dot({}, {});", out(0), a, b);
            }
            ShaderNodeKind::Cross => {
                let a = Self::input_var(graph, id, 0, "vec3<f32>(0.0)");
                let b = Self::input_var(graph, id, 1, "vec3<f32>(0.0)");
                let _ = writeln!(wgsl, "    let {} = cross({}, {});", out(0), a, b);
            }

            // Unary math
            ShaderNodeKind::Sqrt => {
                let a = Self::input_var(graph, id, 0, "0.0");
                let _ = writeln!(wgsl, "    let {} = sqrt({});", out(0), a);
            }
            ShaderNodeKind::Abs => {
                let a = Self::input_var(graph, id, 0, "0.0");
                let _ = writeln!(wgsl, "    let {} = abs({});", out(0), a);
            }
            ShaderNodeKind::Normalize => {
                let a = Self::input_var(graph, id, 0, "vec3<f32>(0.0, 1.0, 0.0)");
                let _ = writeln!(wgsl, "    let {} = normalize({});", out(0), a);
            }
            ShaderNodeKind::Length => {
                let a = Self::input_var(graph, id, 0, "vec3<f32>(0.0)");
                let _ = writeln!(wgsl, "    let {} = length({});", out(0), a);
            }
            ShaderNodeKind::Saturate => {
                let a = Self::input_var(graph, id, 0, "0.0");
                let _ = writeln!(wgsl, "    let {} = clamp({}, 0.0, 1.0);", out(0), a);
            }
            ShaderNodeKind::Negate => {
                let a = Self::input_var(graph, id, 0, "0.0");
                let _ = writeln!(wgsl, "    let {} = -({});", out(0), a);
            }
            ShaderNodeKind::OneMinus => {
                let a = Self::input_var(graph, id, 0, "0.0");
                let _ = writeln!(wgsl, "    let {} = 1.0 - ({});", out(0), a);
            }

            // Clamp / lerp / smoothstep
            ShaderNodeKind::Clamp => {
                let v = Self::input_var(graph, id, 0, "0.0");
                let lo = Self::input_var(graph, id, 1, "0.0");
                let hi = Self::input_var(graph, id, 2, "1.0");
                let _ = writeln!(wgsl, "    let {} = clamp({}, {}, {});", out(0), v, lo, hi);
            }
            ShaderNodeKind::Lerp => {
                let a = Self::input_var(graph, id, 0, "0.0");
                let b = Self::input_var(graph, id, 1, "1.0");
                let t = Self::input_var(graph, id, 2, "0.5");
                let _ = writeln!(wgsl, "    let {} = mix({}, {}, {});", out(0), a, b, t);
            }
            ShaderNodeKind::Smoothstep => {
                let e0 = Self::input_var(graph, id, 0, "0.0");
                let e1 = Self::input_var(graph, id, 1, "1.0");
                let x = Self::input_var(graph, id, 2, "0.5");
                let _ = writeln!(
                    wgsl,
                    "    let {} = smoothstep({}, {}, {});",
                    out(0),
                    e0,
                    e1,
                    x
                );
            }

            // Fresnel
            ShaderNodeKind::Fresnel => {
                let n = Self::input_var(graph, id, 0, "input.world_normal");
                let v = Self::input_var(graph, id, 1, "input.view_dir");
                let exp = Self::input_var(graph, id, 2, "5.0");
                let _ = writeln!(
                    wgsl,
                    "    let {} = fresnel_schlick(max(dot({}, {}), 0.0), {});",
                    out(0),
                    n,
                    v,
                    exp
                );
            }

            // If
            ShaderNodeKind::If => {
                let cond = Self::input_var(graph, id, 0, "0.0");
                let thresh = Self::input_var(graph, id, 1, "0.5");
                let a = Self::input_var(graph, id, 2, "1.0");
                let b = Self::input_var(graph, id, 3, "0.0");
                let _ = writeln!(
                    wgsl,
                    "    let {} = select({}, {}, {} > {});",
                    out(0),
                    b,
                    a,
                    cond,
                    thresh
                );
            }

            // Split / Combine
            ShaderNodeKind::SplitVec4 => {
                let v = Self::input_var(graph, id, 0, "vec4<f32>(0.0)");
                let _ = writeln!(wgsl, " let {} = {}_v.x;", out(0), v);
                let _ = writeln!(wgsl, " let node_{}_1 = {}_v.y;", id, out(0));
                let _ = writeln!(wgsl, " let node_{}_2 = {}_v.z;", id, out(0));
                let _ = writeln!(wgsl, " let node_{}_3 = {}_v.w;", id, out(0));
            }
            ShaderNodeKind::CombineVec4 => {
                let x = Self::input_var(graph, id, 0, "0.0");
                let y = Self::input_var(graph, id, 1, "0.0");
                let z = Self::input_var(graph, id, 2, "0.0");
                let w = Self::input_var(graph, id, 3, "1.0");
                let _ = writeln!(
                    wgsl,
                    "    let {} = vec4<f32>({}, {}, {}, {});",
                    out(0),
                    x,
                    y,
                    z,
                    w
                );
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
                    if let Some(deg) = in_degree.get_mut(&conn.to_node) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push(conn.to_node);
                        }
                    }
                }
            }
        }

        if order.len() != ids.len() {
            return Err("Cycle detected in shader graph".to_string());
        }

        Ok(order)
    }

    /// Validate the graph and return a list of diagnostics.
    ///
    /// Checks:
    /// - PBR output node exists.
    /// - No cycles.
    /// - No dangling connections (referencing missing nodes).
    /// - Warns about unconnected required inputs.
    pub fn validate(graph: &ShaderGraph) -> Vec<ShaderGraphDiagnostic> {
        let mut diags = Vec::new();

        // 1. Check PBR output exists.
        if graph.output_node().is_none() {
            diags.push(ShaderGraphDiagnostic {
                node_id: None,
                severity: DiagnosticSeverity::Error,
                message: "No PbrOutput node in the graph.".into(),
            });
        }

        // 2. Dangling connections.
        let ids: Vec<NodeId> = graph.nodes.iter().map(|n| n.id).collect();
        for conn in &graph.connections {
            if !ids.contains(&conn.from_node) {
                diags.push(ShaderGraphDiagnostic {
                    node_id: Some(conn.to_node),
                    severity: DiagnosticSeverity::Error,
                    message: format!(
                        "Connection references missing source node {}.",
                        conn.from_node
                    ),
                });
            }
            if !ids.contains(&conn.to_node) {
                diags.push(ShaderGraphDiagnostic {
                    node_id: Some(conn.from_node),
                    severity: DiagnosticSeverity::Error,
                    message: format!(
                        "Connection references missing target node {}.",
                        conn.to_node
                    ),
                });
            }
        }

        // 3. Slot bounds.
        for conn in &graph.connections {
            if let Some(src) = graph.node(conn.from_node) {
                if conn.from_slot >= src.output_count() {
                    diags.push(ShaderGraphDiagnostic {
                        node_id: Some(conn.from_node),
                        severity: DiagnosticSeverity::Error,
                        message: format!(
                            "Output slot {} exceeds node {:?} output count ({}).",
                            conn.from_slot,
                            src.kind,
                            src.output_count()
                        ),
                    });
                }
            }
            if let Some(dst) = graph.node(conn.to_node) {
                if conn.to_slot >= dst.input_count() {
                    diags.push(ShaderGraphDiagnostic {
                        node_id: Some(conn.to_node),
                        severity: DiagnosticSeverity::Error,
                        message: format!(
                            "Input slot {} exceeds node {:?} input count ({}).",
                            conn.to_slot,
                            dst.kind,
                            dst.input_count()
                        ),
                    });
                }
            }
        }

        // 4. Cycle detection via toposort.
        if let Err(msg) = Self::topological_order(graph) {
            diags.push(ShaderGraphDiagnostic {
                node_id: None,
                severity: DiagnosticSeverity::Error,
                message: msg,
            });
        }

        // 5. Warn about unconnected PBR inputs.
        if let Some(out) = graph.output_node() {
            let slot_names = [
                "base_color",
                "normal",
                "roughness",
                "metallic",
                "emissive",
                "alpha",
            ];
            for (i, name) in slot_names.iter().enumerate() {
                if graph.find_input(out.id, i as u32).is_none() {
                    diags.push(ShaderGraphDiagnostic {
                        node_id: Some(out.id),
                        severity: DiagnosticSeverity::Warning,
                        message: format!("PbrOutput.{name} is unconnected â€” using default."),
                    });
                }
            }
        }

        diags
    }

    /// Live-preview compile: validates, compiles, and returns a [`CompileResult`]
    /// with diagnostics instead of a bare `Result<String, String>`.
    ///
    /// Intended to be called frequently (e.g. on every graph edit) by the editor;
    /// the caller can display the diagnostics inline next to the affected nodes.
    pub fn compile_live(graph: &ShaderGraph) -> CompileResult {
        let mut diagnostics = Self::validate(graph);
        let has_errors = diagnostics
            .iter()
            .any(|d| d.severity == DiagnosticSeverity::Error);

        if has_errors {
            return CompileResult {
                wgsl: String::new(),
                diagnostics,
                success: false,
            };
        }

        match Self::compile(graph) {
            Ok(wgsl) => {
                diagnostics.push(ShaderGraphDiagnostic {
                    node_id: None,
                    severity: DiagnosticSeverity::Info,
                    message: format!("Compiled OK â€” {} bytes WGSL.", wgsl.len()),
                });
                CompileResult {
                    wgsl,
                    diagnostics,
                    success: true,
                }
            }
            Err(e) => {
                diagnostics.push(ShaderGraphDiagnostic {
                    node_id: None,
                    severity: DiagnosticSeverity::Error,
                    message: e,
                });
                CompileResult {
                    wgsl: String::new(),
                    diagnostics,
                    success: false,
                }
            }
        }
    }
}

// â”€â”€ Cache â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

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

// â”€â”€ Material Domains â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// The domain a material graph targets. Each domain changes which outputs
/// are available and how the compiled WGSL is integrated into the pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MaterialDomain {
    /// Standard PBR surface (mesh rendering). Outputs: base_color, normal,
    /// roughness, metallic, emissive, alpha.
    Surface,
    /// Post-process full-screen effect. Outputs: color (vec4).
    PostProcess,
    /// Deferred decal projection. Outputs: base_color, normal, roughness.
    Decal,
    /// UI / 2D element. Outputs: color, alpha.
    UI,
    /// Unlit surface â€” no PBR lighting. Outputs: color, alpha.
    Unlit,
}

impl MaterialDomain {
    /// Names of the output slots for this domain.
    pub fn output_slot_names(&self) -> &'static [&'static str] {
        match self {
            Self::Surface => &[
                "base_color",
                "normal",
                "roughness",
                "metallic",
                "emissive",
                "alpha",
            ],
            Self::PostProcess => &["color"],
            Self::Decal => &["base_color", "normal", "roughness"],
            Self::UI => &["color", "alpha"],
            Self::Unlit => &["color", "alpha"],
        }
    }

    /// Whether this domain participates in the deferred G-buffer pass.
    pub fn uses_gbuffer(&self) -> bool {
        matches!(self, Self::Surface | Self::Decal)
    }
}

/// A material graph is a [`ShaderGraph`] with an associated domain and
/// blend/render-state metadata.
#[derive(Debug, Clone)]
pub struct MaterialGraph {
    pub graph: ShaderGraph,
    pub domain: MaterialDomain,
    /// Alpha blending mode.
    pub blend_mode: BlendMode,
    /// Whether back-face culling is disabled.
    pub two_sided: bool,
    /// Render queue priority override (lower = drawn first).
    pub queue_priority: i32,
}

/// Blend mode for a material.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    Opaque,
    AlphaTest,
    AlphaBlend,
    Additive,
    Multiply,
}

impl MaterialGraph {
    /// Create a new material graph with the given name and domain.
    pub fn new(name: impl Into<String>, domain: MaterialDomain) -> Self {
        let name_str: String = name.into();
        Self {
            graph: ShaderGraph::new(&name_str),
            domain,
            blend_mode: BlendMode::Opaque,
            two_sided: false,
            queue_priority: 0,
        }
    }

    /// Compile the material graph to WGSL. The output preamble changes
    /// depending on the domain (e.g. `PostProcess` omits the PBR struct).
    pub fn compile(&self) -> Result<String, String> {
        match self.domain {
            MaterialDomain::Surface => ShaderGraphCompiler::compile(&self.graph),
            _ => {
                // For non-surface domains, delegate to the standard compiler
                // which already handles PbrOutput â€” real domain-specific codegen
                // can be added later.
                ShaderGraphCompiler::compile(&self.graph)
            }
        }
    }

    /// Validate the material graph, adding domain-specific checks on top of
    /// the standard graph validation.
    pub fn validate(&self) -> Vec<ShaderGraphDiagnostic> {
        let mut diags = ShaderGraphCompiler::validate(&self.graph);
        if self.domain != MaterialDomain::Surface && self.graph.output_node().is_none() {
            // For non-Surface domains, the PbrOutput requirement is relaxed.
            diags.retain(|d| !d.message.contains("PbrOutput"));
        }
        diags
    }
}
