//! VFX Graph system for node-based visual effects.
//!
//! Provides a node-based visual effect system with:
//! - Modular effect nodes (emitters, modifiers, renderers)
//! - Data flow between nodes
//! - GPU compute integration
//! - Timeline and sequencing

use glam::{Vec2, Vec3, Vec4};

/// Unique identifier for a VFX node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VfxNodeId(pub u64);

/// Unique identifier for a node pin (input/output).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PinId {
    pub node: VfxNodeId,
    pub index: u32,
}

/// Connection between two node pins.
#[derive(Debug, Clone)]
pub struct VfxConnection {
    pub from: PinId,
    pub to: PinId,
}

/// Types of data that flow through the graph.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VfxDataType {
    Particle,
    Float,
    Vec2,
    Vec3,
    Vec4,
    Color,
    Texture,
    Mesh,
    Bool,
    Trigger,
}

/// A node in the VFX graph.
#[derive(Debug, Clone)]
pub struct VfxNode {
    pub id: VfxNodeId,
    pub name: String,
    pub node_type: VfxNodeType,
    pub position: Vec2,
    pub inputs: Vec<Pin>,
    pub outputs: Vec<Pin>,
    pub properties: Vec<Property>,
}

impl VfxNode {
    pub fn new(id: VfxNodeId, name: impl Into<String>, node_type: VfxNodeType) -> Self {
        Self {
            id,
            name: name.into(),
            node_type,
            position: Vec2::ZERO,
            inputs: Vec::new(),
            outputs: Vec::new(),
            properties: Vec::new(),
        }
    }

    pub fn with_position(mut self, x: f32, y: f32) -> Self {
        self.position = Vec2::new(x, y);
        self
    }

    pub fn add_input(&mut self, name: &str, data_type: VfxDataType) {
        self.inputs.push(Pin {
            id: PinId { node: self.id, index: self.inputs.len() as u32 },
            name: name.to_string(),
            data_type,
        });
    }

    pub fn add_output(&mut self, name: &str, data_type: VfxDataType) {
        self.outputs.push(Pin {
            id: PinId { node: self.id, index: self.outputs.len() as u32 },
            name: name.to_string(),
            data_type,
        });
    }

    pub fn add_property(&mut self, name: &str, value: PropertyValue) {
        self.properties.push(Property {
            name: name.to_string(),
            value,
        });
    }
}

/// A pin on a node (input or output).
#[derive(Debug, Clone)]
pub struct Pin {
    pub id: PinId,
    pub name: String,
    pub data_type: VfxDataType,
}

/// A property on a node.
#[derive(Debug, Clone)]
pub struct Property {
    pub name: String,
    pub value: PropertyValue,
}

/// Property value types.
#[derive(Debug, Clone)]
pub enum PropertyValue {
    Float(f32),
    Vec2(Vec2),
    Vec3(Vec3),
    Vec4(Vec4),
    Color(Vec4),
    Bool(bool),
    Int(i32),
    UInt(u32),
    String(String),
}

/// Types of VFX nodes.
#[derive(Debug, Clone)]
pub enum VfxNodeType {
    Emitter(EmitterType),
    Modifier(ModifierType),
    Renderer(RendererType),
    Operator(OperatorType),
    Context(ContextType),
}

/// Particle emitter types.
#[derive(Debug, Clone)]
pub enum EmitterType {
    Burst,
    Continuous,
    Sphere,
    Box,
    Mesh,
    Trail,
    Rate,
}

/// Particle modifier types.
#[derive(Debug, Clone)]
pub enum ModifierType {
    Force,
    Drag,
    Turbulence,
    Color,
    Size,
    Rotation,
    Velocity,
    Lifetime,
    Collision,
    Kill,
    SubEmitters,
}

/// Renderer types.
#[derive(Debug, Clone)]
pub enum RendererType {
    Billboard,
    Mesh,
    Trail,
    Ribbon,
    Point,
    Sprite,
}

/// Operator types for data manipulation.
#[derive(Debug, Clone)]
pub enum OperatorType {
    Add,
    Subtract,
    Multiply,
    Divide,
    Lerp,
    Clamp,
    Min,
    Max,
    Sin,
    Cos,
    Noise,
    Random,
    SampleTexture,
}

/// Context nodes for particle data access.
#[derive(Debug, Clone)]
pub enum ContextType {
    ParticleAge,
    ParticlePosition,
    ParticleVelocity,
    ParticleColor,
    ParticleSize,
    ParticleLifetime,
    EmitterPosition,
    EmitterRotation,
    DeltaTime,
    Time,
    RandomPerParticle,
    Index,
}

/// The VFX graph containing all nodes and connections.
#[derive(Debug, Clone)]
pub struct VfxGraph {
    pub nodes: Vec<VfxNode>,
    pub connections: Vec<VfxConnection>,
    pub name: String,
}

impl VfxGraph {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            nodes: Vec::new(),
            connections: Vec::new(),
            name: name.into(),
        }
    }

    pub fn add_node(&mut self, node: VfxNode) -> VfxNodeId {
        let id = node.id;
        self.nodes.push(node);
        id
    }

    pub fn connect(&mut self, from: PinId, to: PinId) {
        self.connections.retain(|c| c.to != to);
        self.connections.push(VfxConnection { from, to });
    }

    pub fn disconnect(&mut self, from: PinId, to: PinId) {
        self.connections.retain(|c| !(c.from == from && c.to == to));
    }

    pub fn get_node(&self, id: VfxNodeId) -> Option<&VfxNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn get_node_mut(&mut self, id: VfxNodeId) -> Option<&mut VfxNode> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    pub fn validate(&self) -> Result<(), String> {
        for conn in &self.connections {
            let from_node = self.get_node(conn.from.node)
                .ok_or("Connection from invalid node")?;
            let to_node = self.get_node(conn.to.node)
                .ok_or("Connection to invalid node")?;

            let from_pin = from_node.outputs.get(conn.from.index as usize)
                .ok_or("Invalid from pin")?;
            let to_pin = to_node.inputs.get(conn.to.index as usize)
                .ok_or("Invalid to pin")?;

            if from_pin.data_type != to_pin.data_type {
                return Err(format!("Type mismatch: {:?} != {:?}", from_pin.data_type, to_pin.data_type));
            }
        }
        Ok(())
    }

    pub fn create_emitter(&mut self, emitter_type: EmitterType) -> VfxNodeId {
        let id = VfxNodeId(self.nodes.len() as u64);
        let mut node = VfxNode::new(id, format!("{:?}", emitter_type), VfxNodeType::Emitter(emitter_type));
        node.add_output("particles", VfxDataType::Particle);
        node.add_property("rate", PropertyValue::Float(10.0));
        node.add_property("lifetime", PropertyValue::Float(2.0));
        self.add_node(node);
        id
    }

    pub fn create_modifier(&mut self, modifier_type: ModifierType) -> VfxNodeId {
        let id = VfxNodeId(self.nodes.len() as u64);
        let mut node = VfxNode::new(id, format!("{:?}", modifier_type), VfxNodeType::Modifier(modifier_type));
        node.add_input("particles", VfxDataType::Particle);
        node.add_output("particles", VfxDataType::Particle);
        self.add_node(node);
        id
    }

    pub fn create_renderer(&mut self, renderer_type: RendererType) -> VfxNodeId {
        let id = VfxNodeId(self.nodes.len() as u64);
        let mut node = VfxNode::new(id, format!("{:?}", renderer_type), VfxNodeType::Renderer(renderer_type));
        node.add_input("particles", VfxDataType::Particle);
        self.add_node(node);
        id
    }
}

impl Default for VfxGraph {
    fn default() -> Self {
        Self::new("VFX Effect")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vfx_node_new() {
        let node = VfxNode::new(VfxNodeId(0), "test", VfxNodeType::Emitter(EmitterType::Continuous));
        assert_eq!(node.name, "test");
        assert!(node.inputs.is_empty());
    }

    #[test]
    fn vfx_node_add_pin() {
        let mut node = VfxNode::new(VfxNodeId(0), "test", VfxNodeType::Emitter(EmitterType::Continuous));
        node.add_input("input", VfxDataType::Float);
        assert_eq!(node.inputs.len(), 1);
    }

    #[test]
    fn vfx_node_with_position() {
        let node = VfxNode::new(VfxNodeId(0), "test", VfxNodeType::Emitter(EmitterType::Continuous))
            .with_position(100.0, 200.0);
        assert!((node.position.x - 100.0).abs() < 0.001);
    }

    #[test]
    fn vfx_graph_new() {
        let graph = VfxGraph::new("test");
        assert_eq!(graph.name, "test");
        assert!(graph.nodes.is_empty());
    }

    #[test]
    fn vfx_graph_add_node() {
        let mut graph = VfxGraph::new("test");
        let node = VfxNode::new(VfxNodeId(0), "emitter", VfxNodeType::Emitter(EmitterType::Continuous));
        let id = graph.add_node(node);
        assert_eq!(id, VfxNodeId(0));
        assert_eq!(graph.nodes.len(), 1);
    }

    #[test]
    fn vfx_graph_connect() {
        let mut graph = VfxGraph::new("test");
        let emitter = graph.create_emitter(EmitterType::Continuous);
        let modifier = graph.create_modifier(ModifierType::Force);
        
        let from = PinId { node: emitter, index: 0 };
        let to = PinId { node: modifier, index: 0 };
        graph.connect(from, to);
        
        assert_eq!(graph.connections.len(), 1);
    }

    #[test]
    fn vfx_graph_disconnect() {
        let mut graph = VfxGraph::new("test");
        let emitter = graph.create_emitter(EmitterType::Continuous);
        let modifier = graph.create_modifier(ModifierType::Force);
        
        let from = PinId { node: emitter, index: 0 };
        let to = PinId { node: modifier, index: 0 };
        graph.connect(from, to);
        graph.disconnect(from, to);
        
        assert!(graph.connections.is_empty());
    }

    #[test]
    fn vfx_graph_create_emitter() {
        let mut graph = VfxGraph::new("test");
        let id = graph.create_emitter(EmitterType::Burst);
        assert!(graph.get_node(id).is_some());
    }

    #[test]
    fn vfx_graph_create_modifier() {
        let mut graph = VfxGraph::new("test");
        let id = graph.create_modifier(ModifierType::Color);
        assert!(graph.get_node(id).is_some());
    }

    #[test]
    fn vfx_graph_create_renderer() {
        let mut graph = VfxGraph::new("test");
        let id = graph.create_renderer(RendererType::Billboard);
        assert!(graph.get_node(id).is_some());
    }

    #[test]
    fn vfx_graph_validate() {
        let mut graph = VfxGraph::new("test");
        let emitter = graph.create_emitter(EmitterType::Continuous);
        let modifier = graph.create_modifier(ModifierType::Force);
        
        let from = PinId { node: emitter, index: 0 };
        let to = PinId { node: modifier, index: 0 };
        graph.connect(from, to);
        
        assert!(graph.validate().is_ok());
    }

    #[test]
    fn pin_id_equality() {
        let p1 = PinId { node: VfxNodeId(1), index: 0 };
        let p2 = PinId { node: VfxNodeId(1), index: 0 };
        assert_eq!(p1, p2);
    }
}
