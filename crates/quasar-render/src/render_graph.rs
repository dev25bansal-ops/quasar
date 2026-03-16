//! Render graph — declarative pass composition.
//!
//! Defines rendering passes and their dependencies, allowing new effects
//! to be added without rewriting the main loop.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

/// Errors that can occur during render graph compilation/validation.
#[derive(Debug)]
pub enum RenderGraphError {
    /// The graph contains a dependency cycle involving these passes.
    CycleDetected(Vec<PassId>),
    /// A pass reads from an attachment that no earlier pass writes.
    MissingWriter {
        reader: PassId,
        attachment: AttachmentId,
    },
    /// A pass reads an attachment written by another pass without an explicit
    /// dependency (direct or transitive).
    MissingDependency {
        reader: PassId,
        writer: PassId,
        attachment: AttachmentId,
    },
}

impl fmt::Display for RenderGraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CycleDetected(ids) => {
                write!(f, "render graph cycle involving passes: {:?}", ids)
            }
            Self::MissingWriter { reader, attachment } => {
                write!(
                    f,
                    "pass {:?} reads attachment {:?} but no pass writes it",
                    reader, attachment
                )
            }
            Self::MissingDependency {
                reader,
                writer,
                attachment,
            } => {
                write!(
                    f,
                    "pass {:?} reads attachment {:?} written by {:?} but has no dependency on it",
                    reader, attachment, writer
                )
            }
        }
    }
}

impl std::error::Error for RenderGraphError {}

/// Unique identifier for a render pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PassId(pub u64);

/// A render pass that can be executed.
pub trait RenderPass: Send + Sync {
    /// Unique name for this pass type.
    fn name(&self) -> &str;

    /// Execute the pass, recording commands to the encoder.
    fn execute(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        context: &RenderContext,
    );
}

/// Context passed to each render pass.
pub struct RenderContext {
    /// Current screen size.
    pub screen_size: (u32, u32),
    /// HDR color target (if HDR is enabled).
    pub hdr_texture: Option<wgpu::TextureView>,
    /// Depth buffer.
    pub depth_view: wgpu::TextureView,
    /// Camera bind group.
    pub camera_bind_group: wgpu::BindGroup,
    /// Light bind group.
    pub light_bind_group: wgpu::BindGroup,
    /// Additional named resources.
    pub resources: HashMap<String, Arc<dyn std::any::Any + Send + Sync>>,
}

/// A node in the render graph.
pub struct PassNode {
    /// The pass implementation.
    pub pass: Box<dyn RenderPass>,
    /// Passes that must complete before this pass runs.
    pub dependencies: Vec<PassId>,
    /// Input attachments this pass reads from.
    pub inputs: Vec<AttachmentId>,
    /// Output attachments this pass writes to.
    pub outputs: Vec<AttachmentId>,
}

/// Identifier for an attachment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AttachmentId(pub u64);

/// An attachment that can be written to by passes.
pub struct Attachment {
    pub name: String,
    pub format: wgpu::TextureFormat,
    pub size: (u32, u32),
    pub texture: Option<wgpu::Texture>,
    pub view: Option<wgpu::TextureView>,
}

/// The render graph that manages pass execution.
pub struct RenderGraph {
    nodes: HashMap<PassId, PassNode>,
    attachments: HashMap<AttachmentId, Attachment>,
    pass_order: Vec<PassId>,
}

impl RenderGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            attachments: HashMap::new(),
            pass_order: Vec::new(),
        }
    }

    /// Add a render pass to the graph.
    pub fn add_pass(&mut self, id: PassId, pass: Box<dyn RenderPass>) -> &mut Self {
        self.nodes.insert(
            id,
            PassNode {
                pass,
                dependencies: Vec::new(),
                inputs: Vec::new(),
                outputs: Vec::new(),
            },
        );
        self.pass_order.push(id);
        self
    }

    /// Add a dependency between passes.
    pub fn add_dependency(&mut self, pass: PassId, depends_on: PassId) -> &mut Self {
        if let Some(node) = self.nodes.get_mut(&pass) {
            node.dependencies.push(depends_on);
        }
        self
    }

    /// Add an output attachment to a pass.
    pub fn add_output(&mut self, pass: PassId, attachment: AttachmentId) -> &mut Self {
        if let Some(node) = self.nodes.get_mut(&pass) {
            node.outputs.push(attachment);
        }
        self
    }

    /// Add an input attachment to a pass (declares a read dependency).
    pub fn add_input(&mut self, pass: PassId, attachment: AttachmentId) -> &mut Self {
        if let Some(node) = self.nodes.get_mut(&pass) {
            node.inputs.push(attachment);
        }
        self
    }

    /// Add an attachment to the graph.
    pub fn add_attachment(&mut self, id: AttachmentId, attachment: Attachment) -> &mut Self {
        self.attachments.insert(id, attachment);
        self
    }

    /// Create GPU resources for all attachments, aliasing textures of
    /// non-overlapping passes that share the same format and size.
    pub fn create_resources(&mut self, device: &wgpu::Device) -> Result<(), RenderGraphError> {
        let order = self.compile()?;
        let lifetimes = self.compute_attachment_lifetimes(&order);

        // Group attachments by (format, size) for aliasing candidates.
        struct Pool {
            texture: wgpu::Texture,
            last_used: usize, // topological index when freed
        }
        let mut pools: HashMap<(wgpu::TextureFormat, u32, u32), Vec<Pool>> = HashMap::new();

        // Sort attachments by first-use order so earlier ones claim pools first.
        let mut att_ids: Vec<AttachmentId> = self.attachments.keys().copied().collect();
        att_ids.sort_by_key(|id| lifetimes.get(id).map(|r| r.0).unwrap_or(usize::MAX));

        for att_id in att_ids {
            let Some(attachment) = self.attachments.get_mut(&att_id) else { continue };
            let key = (attachment.format, attachment.size.0, attachment.size.1);
            let (first, last) = match lifetimes.get(&att_id) {
                Some(&(f, l)) => (f, l),
                None => {
                    // Not referenced by any pass — allocate standalone.
                    let (texture, view) = Self::alloc_texture(device, attachment);
                    attachment.texture = Some(texture);
                    attachment.view = Some(view);
                    continue;
                }
            };

            // Try to reuse a pool entry whose last_used < first (non-overlapping).
            let pool = pools.entry(key).or_default();
            let reuse_idx = pool.iter().position(|p| p.last_used < first);
            if let Some(idx) = reuse_idx {
                pool[idx].last_used = last;
                let view = pool[idx].texture.create_view(&wgpu::TextureViewDescriptor::default());
                attachment.texture = None; // aliased — owned by pool
                attachment.view = Some(view);
            } else {
                let (texture, _view) = Self::alloc_texture(device, attachment);
                attachment.view = Some(texture.create_view(&wgpu::TextureViewDescriptor::default()));
                pool.push(Pool { texture, last_used: last });
            }
        }
        Ok(())
    }

    fn alloc_texture(device: &wgpu::Device, attachment: &Attachment) -> (wgpu::Texture, wgpu::TextureView) {
        let size = wgpu::Extent3d {
            width: attachment.size.0,
            height: attachment.size.1,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&attachment.name),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: attachment.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    /// Compute the first and last topological index at which each attachment is used.
    fn compute_attachment_lifetimes(&self, order: &[PassId]) -> HashMap<AttachmentId, (usize, usize)> {
        let pass_index: HashMap<PassId, usize> = order.iter().enumerate().map(|(i, &id)| (id, i)).collect();
        let mut lifetimes: HashMap<AttachmentId, (usize, usize)> = HashMap::new();
        for (&pid, node) in &self.nodes {
            let idx = match pass_index.get(&pid) {
                Some(&i) => i,
                None => continue,
            };
            for &att in node.outputs.iter().chain(node.inputs.iter()) {
                let entry = lifetimes.entry(att).or_insert((idx, idx));
                entry.0 = entry.0.min(idx);
                entry.1 = entry.1.max(idx);
            }
        }
        lifetimes
    }

    /// Execute all passes in topological order.
    pub fn execute(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        context: &RenderContext,
    ) -> Result<wgpu::CommandBuffer, RenderGraphError> {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Graph Encoder"),
        });

        let order = self.compile()?;
        for pass_id in &order {
            if let Some(node) = self.nodes.get(pass_id) {
                node.pass.execute(device, queue, &mut encoder, context);
            }
        }

        Ok(encoder.finish())
    }

    /// Compile the graph: topological sort + validation.
    /// Returns the sorted pass order or the first error found.
    pub fn compile(&self) -> Result<Vec<PassId>, RenderGraphError> {
        let sorted = self.topological_sort()?;
        self.validate_resources(&sorted)?;
        Ok(sorted)
    }

    /// Validate that every input attachment has a writer that precedes the reader
    /// in the topological order, and that an explicit dependency path exists.
    fn validate_resources(&self, order: &[PassId]) -> Result<(), RenderGraphError> {
        let pass_index: HashMap<PassId, usize> =
            order.iter().enumerate().map(|(i, &id)| (id, i)).collect();

        // Build writer map: attachment -> list of (PassId, topo_index) that write it.
        let mut writers: HashMap<AttachmentId, Vec<(PassId, usize)>> = HashMap::new();
        for (&pid, node) in &self.nodes {
            if let Some(&idx) = pass_index.get(&pid) {
                for &att in &node.outputs {
                    writers.entry(att).or_default().push((pid, idx));
                }
            }
        }

        // Precompute transitive dependencies for each pass (via BFS).
        let transitive = self.transitive_dependencies();

        for (&reader_id, node) in &self.nodes {
            let reader_idx = match pass_index.get(&reader_id) {
                Some(&i) => i,
                None => continue,
            };
            for &att in &node.inputs {
                let att_writers = match writers.get(&att) {
                    Some(w) => w,
                    None => {
                        return Err(RenderGraphError::MissingWriter {
                            reader: reader_id,
                            attachment: att,
                        });
                    }
                };
                // Find the latest writer that precedes this reader in topo order.
                let preceding_writer = att_writers
                    .iter()
                    .filter(|(_, idx)| *idx < reader_idx)
                    .max_by_key(|(_, idx)| *idx);
                match preceding_writer {
                    None => {
                        return Err(RenderGraphError::MissingWriter {
                            reader: reader_id,
                            attachment: att,
                        });
                    }
                    Some(&(writer_id, _)) => {
                        // Check transitive dependency.
                        let deps = transitive.get(&reader_id);
                        let has_dep = deps.is_some_and(|d| d.contains(&writer_id));
                        if !has_dep {
                            return Err(RenderGraphError::MissingDependency {
                                reader: reader_id,
                                writer: writer_id,
                                attachment: att,
                            });
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Compute the set of all transitive dependencies for each pass.
    fn transitive_dependencies(&self) -> HashMap<PassId, std::collections::HashSet<PassId>> {
        let mut result: HashMap<PassId, std::collections::HashSet<PassId>> = HashMap::new();
        for &id in &self.pass_order {
            let mut visited = std::collections::HashSet::new();
            let mut stack: Vec<PassId> = Vec::new();
            if let Some(node) = self.nodes.get(&id) {
                stack.extend_from_slice(&node.dependencies);
            }
            while let Some(dep) = stack.pop() {
                if visited.insert(dep) {
                    if let Some(node) = self.nodes.get(&dep) {
                        stack.extend_from_slice(&node.dependencies);
                    }
                }
            }
            result.insert(id, visited);
        }
        result
    }

    /// Compute a topological ordering of passes respecting dependencies.
    ///
    /// Returns an error if the graph contains a cycle.
    pub fn topological_sort(&self) -> Result<Vec<PassId>, RenderGraphError> {
        // Kahn's algorithm
        let mut in_degree: HashMap<PassId, usize> = HashMap::new();
        for &id in &self.pass_order {
            in_degree.entry(id).or_insert(0);
        }
        // Build adjacency: dep -> [nodes that depend on it]
        let mut adj: HashMap<PassId, Vec<PassId>> = HashMap::new();
        for (&id, node) in &self.nodes {
            for &dep in &node.dependencies {
                adj.entry(dep).or_default().push(id);
                *in_degree.entry(id).or_insert(0) += 1;
            }
        }

        let mut queue: std::collections::VecDeque<PassId> = self
            .pass_order
            .iter()
            .copied()
            .filter(|id| *in_degree.get(id).unwrap_or(&0) == 0)
            .collect();

        let mut sorted = Vec::with_capacity(self.pass_order.len());
        while let Some(id) = queue.pop_front() {
            sorted.push(id);
            if let Some(dependents) = adj.get(&id) {
                for &dep in dependents {
                    if let Some(deg) = in_degree.get_mut(&dep) {
                        *deg = deg.saturating_sub(1);
                        if *deg == 0 {
                            queue.push_back(dep);
                        }
                    }
                }
            }
        }

        // If there's a cycle, not all nodes were visited.
        if sorted.len() < self.pass_order.len() {
            let cycle_members: Vec<PassId> = self
                .pass_order
                .iter()
                .copied()
                .filter(|id| !sorted.contains(id))
                .collect();
            return Err(RenderGraphError::CycleDetected(cycle_members));
        }

        Ok(sorted)
    }

    /// Reorder the pass execution list. Passes not in `new_order` are
    /// appended at the end in their original order.
    pub fn set_pass_order(&mut self, new_order: Vec<PassId>) {
        let mut remaining: Vec<PassId> = self
            .pass_order
            .iter()
            .filter(|id| !new_order.contains(id))
            .copied()
            .collect();
        self.pass_order = new_order;
        self.pass_order.append(&mut remaining);
    }

    /// Get a pass by ID.
    pub fn get_pass(&self, id: PassId) -> Option<&PassNode> {
        self.nodes.get(&id)
    }

    /// Get an attachment by ID.
    pub fn get_attachment(&self, id: AttachmentId) -> Option<&Attachment> {
        self.attachments.get(&id)
    }
}

impl Default for RenderGraph {
    fn default() -> Self {
        Self::new()
    }
}

// ── Resource state tracking and barrier insertion ────────────────────

/// The usage state of an attachment between passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceState {
    /// Not yet used this frame.
    Undefined,
    /// Written as a render target (color or depth attachment).
    RenderTarget,
    /// Read as a sampled texture in a shader.
    ShaderRead,
    /// Read/written as a storage texture or buffer.
    Storage,
    /// Ready for presentation to the surface.
    Present,
}

/// A resource state transition between two passes.
#[derive(Debug, Clone)]
pub struct ResourceTransition {
    pub attachment: AttachmentId,
    pub from: ResourceState,
    pub to: ResourceState,
    /// The pass that requires this transition *before* it runs.
    pub before_pass: PassId,
}

impl RenderGraph {
    /// Compute the resource transitions needed between passes in the given
    /// topological order.  This information can be used to insert pipeline
    /// barriers on explicit-sync APIs (Vulkan/D3D12) or to validate usage
    /// on wgpu (which does implicit barriers but benefits from the metadata).
    pub fn compute_transitions(&self, order: &[PassId]) -> Vec<ResourceTransition> {
        let mut current_state: HashMap<AttachmentId, ResourceState> = HashMap::new();
        let mut transitions = Vec::new();

        for &pass_id in order {
            let node = match self.nodes.get(&pass_id) {
                Some(n) => n,
                None => continue,
            };

            // Inputs need ShaderRead.
            for &att in &node.inputs {
                let prev = current_state.get(&att).copied().unwrap_or(ResourceState::Undefined);
                if prev != ResourceState::ShaderRead {
                    transitions.push(ResourceTransition {
                        attachment: att,
                        from: prev,
                        to: ResourceState::ShaderRead,
                        before_pass: pass_id,
                    });
                    current_state.insert(att, ResourceState::ShaderRead);
                }
            }

            // Outputs need RenderTarget.
            for &att in &node.outputs {
                let prev = current_state.get(&att).copied().unwrap_or(ResourceState::Undefined);
                if prev != ResourceState::RenderTarget {
                    transitions.push(ResourceTransition {
                        attachment: att,
                        from: prev,
                        to: ResourceState::RenderTarget,
                        before_pass: pass_id,
                    });
                    current_state.insert(att, ResourceState::RenderTarget);
                }
            }
        }

        transitions
    }

    /// Return the final resource states after all passes have been executed
    /// in the given order.  Useful for determining which attachments need a
    /// transition to `Present` before swapchain submission.
    pub fn final_states(&self, order: &[PassId]) -> HashMap<AttachmentId, ResourceState> {
        let mut state: HashMap<AttachmentId, ResourceState> = HashMap::new();
        for &pid in order {
            if let Some(node) = self.nodes.get(&pid) {
                for &att in &node.inputs {
                    state.insert(att, ResourceState::ShaderRead);
                }
                for &att in &node.outputs {
                    state.insert(att, ResourceState::RenderTarget);
                }
            }
        }
        state
    }
}

/// Common pass IDs.
pub mod pass_ids {
    use super::PassId;

    pub const SHADOW: PassId = PassId(0);
    pub const OPAQUE: PassId = PassId(1);
    pub const TRANSPARENT: PassId = PassId(2);
    pub const POST_PROCESS: PassId = PassId(3);
    pub const TONEMAP: PassId = PassId(4);
    pub const UI: PassId = PassId(5);
}

/// Common attachment IDs.
pub mod attachment_ids {
    use super::AttachmentId;

    pub const HDR_COLOR: AttachmentId = AttachmentId(0);
    pub const DEPTH: AttachmentId = AttachmentId(1);
    pub const SHADOW_MAP: AttachmentId = AttachmentId(2);
}

// ── Async Compute Queue Support ──────────────────────────────────

/// Which GPU queue a pass should be submitted to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PassQueue {
    /// Main graphics queue (vertex/fragment/compute).
    Graphics,
    /// Async compute queue (compute-only, runs in parallel with graphics).
    AsyncCompute,
    /// Transfer/copy queue (DMA operations).
    Transfer,
}

/// Extended pass node with queue affinity and barrier info.
pub struct PassNodeExt {
    pub pass: Box<dyn RenderPass>,
    pub dependencies: Vec<PassId>,
    pub inputs: Vec<AttachmentId>,
    pub outputs: Vec<AttachmentId>,
    /// Which queue this pass should run on.
    pub queue: PassQueue,
    /// Whether this pass requires a cross-queue sync before execution.
    pub cross_queue_wait: bool,
}

/// A barrier inserted between passes to synchronize resource state.
#[derive(Debug, Clone)]
pub struct TextureBarrier {
    pub attachment: AttachmentId,
    pub src_state: ResourceState,
    pub dst_state: ResourceState,
    /// Insert this barrier before this pass.
    pub before_pass: PassId,
    /// If true, this barrier also synchronizes across queue families.
    pub cross_queue: bool,
}

impl RenderGraph {
    /// Build a full barrier plan from the compiled pass order.
    ///
    /// Returns a list of barriers that must be inserted before each pass.
    /// Cross-queue barriers are flagged when consecutive passes use different queues.
    pub fn build_barrier_plan(
        &self,
        order: &[PassId],
        queue_map: &HashMap<PassId, PassQueue>,
    ) -> Vec<TextureBarrier> {
        let mut current_state: HashMap<AttachmentId, ResourceState> = HashMap::new();
        let mut last_queue: HashMap<AttachmentId, PassQueue> = HashMap::new();
        let mut barriers = Vec::new();

        for &pass_id in order {
            let node = match self.nodes.get(&pass_id) {
                Some(n) => n,
                None => continue,
            };
            let pass_queue = queue_map.get(&pass_id).copied().unwrap_or(PassQueue::Graphics);

            // Inputs → ShaderRead
            for &att in &node.inputs {
                let prev = current_state.get(&att).copied().unwrap_or(ResourceState::Undefined);
                let prev_queue = last_queue.get(&att).copied().unwrap_or(PassQueue::Graphics);
                let cross = prev_queue != pass_queue;

                if prev != ResourceState::ShaderRead || cross {
                    barriers.push(TextureBarrier {
                        attachment: att,
                        src_state: prev,
                        dst_state: ResourceState::ShaderRead,
                        before_pass: pass_id,
                        cross_queue: cross,
                    });
                    current_state.insert(att, ResourceState::ShaderRead);
                    last_queue.insert(att, pass_queue);
                }
            }

            // Outputs → RenderTarget
            for &att in &node.outputs {
                let prev = current_state.get(&att).copied().unwrap_or(ResourceState::Undefined);
                let prev_queue = last_queue.get(&att).copied().unwrap_or(PassQueue::Graphics);
                let cross = prev_queue != pass_queue;

                if prev != ResourceState::RenderTarget || cross {
                    barriers.push(TextureBarrier {
                        attachment: att,
                        src_state: prev,
                        dst_state: ResourceState::RenderTarget,
                        before_pass: pass_id,
                        cross_queue: cross,
                    });
                    current_state.insert(att, ResourceState::RenderTarget);
                    last_queue.insert(att, pass_queue);
                }
            }
        }

        barriers
    }

    /// Split the compiled pass order into per-queue command buffer groups.
    ///
    /// Returns a list of (PassQueue, Vec<PassId>) submit groups in order.
    pub fn split_by_queue(
        &self,
        order: &[PassId],
        queue_map: &HashMap<PassId, PassQueue>,
    ) -> Vec<(PassQueue, Vec<PassId>)> {
        let mut groups: Vec<(PassQueue, Vec<PassId>)> = Vec::new();

        for &pass_id in order {
            let queue = queue_map.get(&pass_id).copied().unwrap_or(PassQueue::Graphics);
            if let Some(last) = groups.last_mut() {
                if last.0 == queue {
                    last.1.push(pass_id);
                    continue;
                }
            }
            groups.push((queue, vec![pass_id]));
        }

        groups
    }

    /// Infer automatic dependencies from attachment read-after-write patterns.
    ///
    /// For every pass P that reads attachment A, find the latest pass W that
    /// writes A and add W as a dependency of P (if not already present).
    pub fn infer_dependencies(&mut self) {
        // Build writer map: attachment → list of passes that write it.
        let mut writers: HashMap<AttachmentId, Vec<PassId>> = HashMap::new();
        for (&pid, node) in &self.nodes {
            for &att in &node.outputs {
                writers.entry(att).or_default().push(pid);
            }
        }

        // For each pass that reads, add deps on writers.
        let pass_ids: Vec<PassId> = self.nodes.keys().copied().collect();
        for pid in pass_ids {
            let inputs = self.nodes.get(&pid).map(|n| n.inputs.clone()).unwrap_or_default();
            for att in inputs {
                if let Some(att_writers) = writers.get(&att) {
                    for &writer in att_writers {
                        if writer != pid {
                            if let Some(node) = self.nodes.get_mut(&pid) {
                                if !node.dependencies.contains(&writer) {
                                    node.dependencies.push(writer);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_graph() {
        let graph = RenderGraph::new();
        assert!(graph.nodes.is_empty());
    }

    #[test]
    fn add_attachment() {
        let mut graph = RenderGraph::new();
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
        assert!(graph.get_attachment(attachment_ids::HDR_COLOR).is_some());
    }

    // Dummy pass for testing.
    struct DummyPass(&'static str);
    impl RenderPass for DummyPass {
        fn name(&self) -> &str { self.0 }
        fn execute(
            &self, _: &wgpu::Device, _: &wgpu::Queue,
            _: &mut wgpu::CommandEncoder, _: &RenderContext,
        ) {}
    }

    #[test]
    fn topological_sort_respects_dependencies() {
        let mut graph = RenderGraph::new();
        let a = PassId(0);
        let b = PassId(1);
        let c = PassId(2);
        graph.add_pass(a, Box::new(DummyPass("a")));
        graph.add_pass(b, Box::new(DummyPass("b")));
        graph.add_pass(c, Box::new(DummyPass("c")));
        graph.add_dependency(c, a);
        graph.add_dependency(c, b);

        let order = graph.topological_sort().unwrap();
        let pos = |id: PassId| order.iter().position(|&x| x == id).unwrap();
        assert!(pos(a) < pos(c));
        assert!(pos(b) < pos(c));
    }

    #[test]
    fn cycle_detected() {
        let mut graph = RenderGraph::new();
        let a = PassId(0);
        let b = PassId(1);
        graph.add_pass(a, Box::new(DummyPass("a")));
        graph.add_pass(b, Box::new(DummyPass("b")));
        graph.add_dependency(a, b);
        graph.add_dependency(b, a);

        let result = graph.topological_sort();
        assert!(matches!(result, Err(RenderGraphError::CycleDetected(_))));
    }

    #[test]
    fn validate_missing_writer() {
        let mut graph = RenderGraph::new();
        let a = PassId(0);
        let att = AttachmentId(99);
        graph.add_pass(a, Box::new(DummyPass("a")));
        graph.add_input(a, att);

        let result = graph.compile();
        assert!(matches!(
            result,
            Err(RenderGraphError::MissingWriter { .. })
        ));
    }

    #[test]
    fn validate_missing_dependency() {
        let mut graph = RenderGraph::new();
        let a = PassId(0);
        let b = PassId(1);
        let att = AttachmentId(10);
        graph.add_pass(a, Box::new(DummyPass("writer")));
        graph.add_pass(b, Box::new(DummyPass("reader")));
        graph.add_output(a, att);
        graph.add_input(b, att);
        // b reads att written by a, but no explicit dependency.

        let result = graph.compile();
        assert!(matches!(
            result,
            Err(RenderGraphError::MissingDependency { .. })
        ));
    }

    #[test]
    fn validate_ok_with_dependency() {
        let mut graph = RenderGraph::new();
        let a = PassId(0);
        let b = PassId(1);
        let att = AttachmentId(10);
        graph.add_pass(a, Box::new(DummyPass("writer")));
        graph.add_pass(b, Box::new(DummyPass("reader")));
        graph.add_output(a, att);
        graph.add_input(b, att);
        graph.add_dependency(b, a);

        assert!(graph.compile().is_ok());
    }

    #[test]
    fn validate_transitive_dependency_ok() {
        let mut graph = RenderGraph::new();
        let a = PassId(0);
        let b = PassId(1);
        let c = PassId(2);
        let att = AttachmentId(10);
        graph.add_pass(a, Box::new(DummyPass("writer")));
        graph.add_pass(b, Box::new(DummyPass("middle")));
        graph.add_pass(c, Box::new(DummyPass("reader")));
        graph.add_output(a, att);
        graph.add_input(c, att);
        // c depends on b, b depends on a → transitive dep on a.
        graph.add_dependency(b, a);
        graph.add_dependency(c, b);

        assert!(graph.compile().is_ok());
    }
}
