//! Render graph — declarative pass composition.
//!
//! Defines rendering passes and their dependencies, allowing new effects
//! to be added without rewriting the main loop.

use std::collections::HashMap;
use std::sync::Arc;

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

    /// Add an attachment to the graph.
    pub fn add_attachment(&mut self, id: AttachmentId, attachment: Attachment) -> &mut Self {
        self.attachments.insert(id, attachment);
        self
    }

    /// Create GPU resources for all attachments.
    pub fn create_resources(&mut self, device: &wgpu::Device) {
        for attachment in self.attachments.values_mut() {
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
            attachment.texture = Some(texture);
            attachment.view = Some(view);
        }
    }

    /// Execute all passes in topological order.
    pub fn execute(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        context: &RenderContext,
    ) -> wgpu::CommandBuffer {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Graph Encoder"),
        });

        let order = self.topological_sort();
        for pass_id in &order {
            if let Some(node) = self.nodes.get(pass_id) {
                node.pass.execute(device, queue, &mut encoder, context);
            }
        }

        encoder.finish()
    }

    /// Compute a topological ordering of passes respecting dependencies.
    ///
    /// Falls back to the declared order for passes that have no
    /// dependency relationships.
    pub fn topological_sort(&self) -> Vec<PassId> {
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

        // If there's a cycle or we missed nodes, append remaining in declared order
        if sorted.len() < self.pass_order.len() {
            for id in &self.pass_order {
                if !sorted.contains(id) {
                    sorted.push(*id);
                }
            }
        }

        sorted
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

/// Common pass IDs.
pub mod pass_ids {
    use super::PassId;

    pub const SHADOW: PassId = PassId(0);
    pub const OPAQUE: PassId = PassId(1);
    pub const TRANSPARENT: PassId = PassId(2);
    pub const POST_PROCESS: PassId = PassId(3);
    pub const UI: PassId = PassId(4);
}

/// Common attachment IDs.
pub mod attachment_ids {
    use super::AttachmentId;

    pub const HDR_COLOR: AttachmentId = AttachmentId(0);
    pub const DEPTH: AttachmentId = AttachmentId(1);
    pub const SHADOW_MAP: AttachmentId = AttachmentId(2);
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
}
