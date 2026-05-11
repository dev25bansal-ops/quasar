//! Visual Behavior Tree Graph Editor for Quasar Engine.
//!
//! Provides:
//! - **Node types** — composites, decorators, leaves
//! - **Drag-and-drop** — reposition nodes freely
//! - **Connection management** — create/delete connections
//! - **Viewport pan/zoom** — navigate large trees
//! - **Selection** — click to select, delete selected
//! - **Validation** — detect invalid tree structures

#![allow(deprecated)]

use egui::{epaint::Shape, Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Unique identifier for a node in the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GraphNodeId(pub u64);

/// Unique identifier for a connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GraphConnectionId(pub u64);

/// Types of nodes available in the behavior tree editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BtEditorNodeType {
    // Composite nodes
    Selector,
    Sequence,
    Parallel,
    RandomSelector,
    RandomSequence,
    // Decorator nodes
    Inverter,
    Repeater,
    Succeeder,
    Failer,
    Timeout,
    Cooldown,
    Retry,
    AlwaysRunning,
    // Leaf nodes
    Action,
    Condition,
    Wait,
    SetBlackboard,
    Log,
    // Special
    Comment,
}

impl BtEditorNodeType {
    /// Human-readable name.
    pub fn name(&self) -> &'static str {
        match self {
            BtEditorNodeType::Selector => "Selector",
            BtEditorNodeType::Sequence => "Sequence",
            BtEditorNodeType::Parallel => "Parallel",
            BtEditorNodeType::RandomSelector => "Random Selector",
            BtEditorNodeType::RandomSequence => "Random Sequence",
            BtEditorNodeType::Inverter => "Inverter",
            BtEditorNodeType::Repeater => "Repeater",
            BtEditorNodeType::Succeeder => "Succeeder",
            BtEditorNodeType::Failer => "Failer",
            BtEditorNodeType::Timeout => "Timeout",
            BtEditorNodeType::Cooldown => "Cooldown",
            BtEditorNodeType::Retry => "Retry",
            BtEditorNodeType::AlwaysRunning => "Always Running",
            BtEditorNodeType::Action => "Action",
            BtEditorNodeType::Condition => "Condition",
            BtEditorNodeType::Wait => "Wait",
            BtEditorNodeType::SetBlackboard => "Set Blackboard",
            BtEditorNodeType::Log => "Log",
            BtEditorNodeType::Comment => "Comment",
        }
    }

    /// Icon for the node type.
    pub fn icon(&self) -> &'static str {
        match self {
            BtEditorNodeType::Selector => "\u{25C9}",
            BtEditorNodeType::Sequence => "\u{25A0}",
            BtEditorNodeType::Parallel => "\u{25A6}",
            BtEditorNodeType::RandomSelector => "\u{1F3B2}",
            BtEditorNodeType::RandomSequence => "\u{1F3B2}",
            BtEditorNodeType::Inverter => "\u{00AC}",
            BtEditorNodeType::Repeater => "\u{21BB}",
            BtEditorNodeType::Succeeder => "\u{2713}",
            BtEditorNodeType::Failer => "\u{2717}",
            BtEditorNodeType::Timeout => "\u{23F1}",
            BtEditorNodeType::Cooldown => "\u{23F3}",
            BtEditorNodeType::Retry => "\u{21BA}",
            BtEditorNodeType::AlwaysRunning => "\u{25B6}",
            BtEditorNodeType::Action => "\u{2699}",
            BtEditorNodeType::Condition => "\u{2753}",
            BtEditorNodeType::Wait => "\u{23F8}",
            BtEditorNodeType::SetBlackboard => "\u{1F4CB}",
            BtEditorNodeType::Log => "\u{1F4DD}",
            BtEditorNodeType::Comment => "\u{1F4AC}",
        }
    }

    /// Background color for the node type.
    pub fn color(&self) -> Color32 {
        match self {
            // Composites - warm colors
            BtEditorNodeType::Selector => Color32::from_rgb(255, 160, 60),
            BtEditorNodeType::Sequence => Color32::from_rgb(60, 180, 120),
            BtEditorNodeType::Parallel => Color32::from_rgb(160, 100, 220),
            BtEditorNodeType::RandomSelector => Color32::from_rgb(255, 200, 100),
            BtEditorNodeType::RandomSequence => Color32::from_rgb(100, 220, 160),
            // Decorators - cool colors
            BtEditorNodeType::Inverter => Color32::from_rgb(180, 120, 220),
            BtEditorNodeType::Repeater => Color32::from_rgb(120, 160, 220),
            BtEditorNodeType::Succeeder => Color32::from_rgb(80, 200, 80),
            BtEditorNodeType::Failer => Color32::from_rgb(220, 80, 80),
            BtEditorNodeType::Timeout => Color32::from_rgb(220, 200, 60),
            BtEditorNodeType::Cooldown => Color32::from_rgb(180, 180, 200),
            BtEditorNodeType::Retry => Color32::from_rgb(140, 180, 220),
            BtEditorNodeType::AlwaysRunning => Color32::from_rgb(100, 100, 100),
            // Leaves - neutral colors
            BtEditorNodeType::Action => Color32::from_rgb(70, 130, 200),
            BtEditorNodeType::Condition => Color32::from_rgb(200, 100, 70),
            BtEditorNodeType::Wait => Color32::from_rgb(160, 160, 160),
            BtEditorNodeType::SetBlackboard => Color32::from_rgb(100, 200, 220),
            BtEditorNodeType::Log => Color32::from_rgb(140, 140, 180),
            // Special
            BtEditorNodeType::Comment => Color32::from_rgb(200, 200, 100),
        }
    }

    /// Whether this node type can have children.
    pub fn can_have_children(&self) -> bool {
        match self {
            BtEditorNodeType::Selector
            | BtEditorNodeType::Sequence
            | BtEditorNodeType::Parallel
            | BtEditorNodeType::RandomSelector
            | BtEditorNodeType::RandomSequence
            | BtEditorNodeType::Inverter
            | BtEditorNodeType::Repeater
            | BtEditorNodeType::Succeeder
            | BtEditorNodeType::Failer
            | BtEditorNodeType::Timeout
            | BtEditorNodeType::Cooldown
            | BtEditorNodeType::Retry
            | BtEditorNodeType::AlwaysRunning => true,
            BtEditorNodeType::Action
            | BtEditorNodeType::Condition
            | BtEditorNodeType::Wait
            | BtEditorNodeType::SetBlackboard
            | BtEditorNodeType::Log
            | BtEditorNodeType::Comment => false,
        }
    }

    /// Maximum children (None = unlimited).
    pub fn max_children(&self) -> Option<usize> {
        match self {
            BtEditorNodeType::Inverter
            | BtEditorNodeType::Succeeder
            | BtEditorNodeType::Failer
            | BtEditorNodeType::Timeout
            | BtEditorNodeType::Cooldown
            | BtEditorNodeType::Retry
            | BtEditorNodeType::AlwaysRunning => Some(1),
            BtEditorNodeType::Selector
            | BtEditorNodeType::Sequence
            | BtEditorNodeType::Parallel
            | BtEditorNodeType::RandomSelector
            | BtEditorNodeType::RandomSequence => None,
            _ => Some(0),
        }
    }

    /// Whether this is a composite node.
    pub fn is_composite(&self) -> bool {
        matches!(
            self,
            BtEditorNodeType::Selector
                | BtEditorNodeType::Sequence
                | BtEditorNodeType::Parallel
                | BtEditorNodeType::RandomSelector
                | BtEditorNodeType::RandomSequence
        )
    }

    /// Whether this is a decorator node.
    pub fn is_decorator(&self) -> bool {
        matches!(
            self,
            BtEditorNodeType::Inverter
                | BtEditorNodeType::Repeater
                | BtEditorNodeType::Succeeder
                | BtEditorNodeType::Failer
                | BtEditorNodeType::Timeout
                | BtEditorNodeType::Cooldown
                | BtEditorNodeType::Retry
                | BtEditorNodeType::AlwaysRunning
        )
    }

    /// Whether this is a leaf node.
    pub fn is_leaf(&self) -> bool {
        matches!(
            self,
            BtEditorNodeType::Action
                | BtEditorNodeType::Condition
                | BtEditorNodeType::Wait
                | BtEditorNodeType::SetBlackboard
                | BtEditorNodeType::Log
        )
    }

    /// All node types as a static slice.
    pub fn all_types() -> &'static [Self] {
        &[
            BtEditorNodeType::Selector,
            BtEditorNodeType::Sequence,
            BtEditorNodeType::Parallel,
            BtEditorNodeType::RandomSelector,
            BtEditorNodeType::RandomSequence,
            BtEditorNodeType::Inverter,
            BtEditorNodeType::Repeater,
            BtEditorNodeType::Succeeder,
            BtEditorNodeType::Failer,
            BtEditorNodeType::Timeout,
            BtEditorNodeType::Cooldown,
            BtEditorNodeType::Retry,
            BtEditorNodeType::AlwaysRunning,
            BtEditorNodeType::Action,
            BtEditorNodeType::Condition,
            BtEditorNodeType::Wait,
            BtEditorNodeType::SetBlackboard,
            BtEditorNodeType::Log,
            BtEditorNodeType::Comment,
        ]
    }
}

/// A node in the behavior tree graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BtEditorNode {
    pub id: GraphNodeId,
    pub node_type: BtEditorNodeType,
    pub name: String,
    pub position: [f32; 2],
    pub properties: HashMap<String, String>,
    /// Simulation status (set during simulation).
    pub sim_status: Option<String>,
}

impl BtEditorNode {
    pub fn new(id: GraphNodeId, node_type: BtEditorNodeType, name: &str, position: Pos2) -> Self {
        let mut properties = HashMap::new();
        // Set default properties based on node type
        match node_type {
            BtEditorNodeType::Repeater => {
                properties.insert("count".to_string(), "-1".to_string()); // -1 = infinite
            }
            BtEditorNodeType::Retry => {
                properties.insert("max_retries".to_string(), "3".to_string());
            }
            BtEditorNodeType::Timeout => {
                properties.insert("timeout".to_string(), "5.0".to_string());
            }
            BtEditorNodeType::Cooldown => {
                properties.insert("cooldown".to_string(), "2.0".to_string());
            }
            BtEditorNodeType::Wait => {
                properties.insert("duration".to_string(), "1.0".to_string());
            }
            BtEditorNodeType::Action => {
                properties.insert("action_name".to_string(), name.to_string());
            }
            BtEditorNodeType::Condition => {
                properties.insert("key".to_string(), "".to_string());
                properties.insert("expected".to_string(), "true".to_string());
            }
            BtEditorNodeType::SetBlackboard => {
                properties.insert("key".to_string(), "".to_string());
                properties.insert("value".to_string(), "".to_string());
            }
            BtEditorNodeType::Log => {
                properties.insert("message".to_string(), "".to_string());
            }
            BtEditorNodeType::Parallel => {
                properties.insert("policy".to_string(), "RequireAll".to_string());
            }
            _ => {}
        }

        Self {
            id,
            node_type,
            name: name.to_string(),
            position: [position.x, position.y],
            properties,
            sim_status: None,
        }
    }

    /// Node rectangle in world space.
    pub fn rect(&self) -> Rect {
        let pos = self.position();
        let size = self.size();
        Rect::from_min_size(pos, size)
    }

    /// Node size.
    pub fn size(&self) -> Vec2 {
        let name_len = self.name.len() as f32;
        let width = (name_len * 7.0 + 60.0).max(120.0).min(250.0);
        Vec2::new(width, 50.0)
    }

    /// Input slot position (top center).
    pub fn input_slot_pos(&self) -> Pos2 {
        let pos = self.position();
        let size = self.size();
        Pos2::new(pos.x + size.x / 2.0, pos.y)
    }

    /// Output slot position (bottom center).
    pub fn output_slot_pos(&self) -> Pos2 {
        let pos = self.position();
        let size = self.size();
        Pos2::new(pos.x + size.x / 2.0, pos.y + size.y)
    }

    pub fn position(&self) -> Pos2 {
        Pos2::new(self.position[0], self.position[1])
    }

    pub fn set_position(&mut self, pos: Pos2) {
        self.position = [pos.x, pos.y];
    }
}

/// A connection between two nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BtEditorConnection {
    pub id: GraphConnectionId,
    pub from: GraphNodeId,
    pub to: GraphNodeId,
    /// Order among siblings (for ordering children).
    pub order: u32,
}

/// The complete state of a behavior tree graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BtGraphState {
    pub name: String,
    pub nodes: HashMap<GraphNodeId, BtEditorNode>,
    pub connections: HashMap<GraphConnectionId, BtEditorConnection>,
    pub root_node: Option<GraphNodeId>,
    pub next_node_id: u64,
    pub next_connection_id: u64,
    /// Currently selected node.
    pub selected_node: Option<GraphNodeId>,
    /// Node being dragged.
    pub dragging_node: Option<GraphNodeId>,
    /// Connection being created (source node).
    pub connecting_from: Option<GraphNodeId>,
    /// Viewport pan offset.
    pub pan_offset: [f32; 2],
    /// Viewport zoom level.
    pub zoom: f32,
    /// Show grid background.
    pub show_grid: bool,
    /// Snap nodes to grid.
    pub snap_to_grid: bool,
    /// Grid cell size.
    pub grid_size: f32,
}

impl BtGraphState {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            nodes: HashMap::new(),
            connections: HashMap::new(),
            root_node: None,
            next_node_id: 1,
            next_connection_id: 1,
            selected_node: None,
            dragging_node: None,
            connecting_from: None,
            pan_offset: [0.0, 0.0],
            zoom: 1.0,
            show_grid: true,
            snap_to_grid: true,
            grid_size: 20.0,
        }
    }

    /// Create a new graph state from an existing graph (copy).
    pub fn from_existing(other: &BtGraphState) -> Self {
        Self {
            name: other.name.clone(),
            nodes: other.nodes.clone(),
            connections: other.connections.clone(),
            root_node: other.root_node,
            next_node_id: other.next_node_id,
            next_connection_id: other.next_connection_id,
            selected_node: None,
            dragging_node: None,
            connecting_from: None,
            pan_offset: other.pan_offset,
            zoom: other.zoom,
            show_grid: other.show_grid,
            snap_to_grid: other.snap_to_grid,
            grid_size: other.grid_size,
        }
    }

    /// Add a node and return its ID.
    pub fn add_node(
        &mut self,
        node_type: BtEditorNodeType,
        name: &str,
        position: Pos2,
    ) -> GraphNodeId {
        let id = GraphNodeId(self.next_node_id);
        self.next_node_id += 1;

        let pos = if self.snap_to_grid {
            Pos2::new(
                (position.x / self.grid_size).round() * self.grid_size,
                (position.y / self.grid_size).round() * self.grid_size,
            )
        } else {
            position
        };

        let node = BtEditorNode::new(id, node_type, name, pos);

        // First node becomes root
        if self.root_node.is_none() {
            self.root_node = Some(id);
        }

        self.nodes.insert(id, node);
        id
    }

    /// Remove a node and all its connections.
    pub fn remove_node(&mut self, id: GraphNodeId) {
        self.nodes.remove(&id);
        self.connections.retain(|_, c| c.from != id && c.to != id);
        if self.root_node == Some(id) {
            self.root_node = self.nodes.keys().next().copied();
        }
        if self.selected_node == Some(id) {
            self.selected_node = None;
        }
    }

    /// Add a connection between two nodes.
    pub fn add_connection(
        &mut self,
        from: GraphNodeId,
        to: GraphNodeId,
    ) -> Option<GraphConnectionId> {
        if !self.nodes.contains_key(&from) || !self.nodes.contains_key(&to) {
            return None;
        }
        if from == to {
            return None;
        }
        // Prevent cycles: check if `to` is already an ancestor of `from`
        if self.is_ancestor(to, from) {
            return None;
        }
        // Check if target already has a parent
        if self.connections.values().any(|c| c.to == to) {
            return None;
        }
        // Check max children
        if let Some(from_node) = self.nodes.get(&from) {
            if let Some(max) = from_node.node_type.max_children() {
                let child_count = self.connections.values().filter(|c| c.from == from).count();
                if child_count >= max {
                    return None;
                }
            }
        }

        let id = GraphConnectionId(self.next_connection_id);
        self.next_connection_id += 1;

        let order = self.connections.values().filter(|c| c.from == from).count() as u32;

        self.connections.insert(
            id,
            BtEditorConnection {
                id,
                from,
                to,
                order,
            },
        );
        Some(id)
    }

    /// Remove a connection.
    pub fn remove_connection(&mut self, id: GraphConnectionId) {
        self.connections.remove(&id);
    }

    /// Check if `potential_ancestor` is an ancestor of `node`.
    fn is_ancestor(&self, potential_ancestor: GraphNodeId, node: GraphNodeId) -> bool {
        // Find parent of `node`
        let parent = self
            .connections
            .values()
            .find(|c| c.to == node)
            .map(|c| c.from);

        match parent {
            Some(p) if p == potential_ancestor => true,
            Some(p) => self.is_ancestor(potential_ancestor, p),
            None => false,
        }
    }

    /// Get children of a node (ordered).
    pub fn children_of(&self, node_id: GraphNodeId) -> Vec<&BtEditorNode> {
        let mut children: Vec<_> = self
            .connections
            .values()
            .filter(|c| c.from == node_id)
            .filter_map(|c| self.nodes.get(&c.to))
            .collect();
        children.sort_by_key(|n| {
            self.connections
                .values()
                .find(|c| c.to == n.id)
                .map(|c| c.order)
                .unwrap_or(0)
        });
        children
    }

    /// Get parent of a node.
    pub fn parent_of(&self, node_id: GraphNodeId) -> Option<&BtEditorNode> {
        self.connections
            .values()
            .find(|c| c.to == node_id)
            .and_then(|c| self.nodes.get(&c.from))
    }

    /// Find node at a given world position.
    pub fn node_at_position(&self, world_pos: Pos2) -> Option<GraphNodeId> {
        for node in self.nodes.values() {
            if node.rect().contains(world_pos) {
                return Some(node.id);
            }
        }
        None
    }

    /// Find output slot at a given world position.
    pub fn output_slot_at_position(&self, world_pos: Pos2) -> Option<GraphNodeId> {
        for node in self.nodes.values() {
            if node.node_type.can_have_children() {
                let slot_pos = node.output_slot_pos();
                let dist = slot_pos.distance(world_pos);
                if dist < 10.0 {
                    return Some(node.id);
                }
            }
        }
        None
    }

    /// Convert screen position to world position.
    pub fn screen_to_world(&self, screen: Pos2) -> Pos2 {
        Pos2::new(
            (screen.x - self.pan_offset[0]) / self.zoom,
            (screen.y - self.pan_offset[1]) / self.zoom,
        )
    }

    /// Convert world position to screen position.
    pub fn world_to_screen(&self, world: Pos2) -> Pos2 {
        Pos2::new(
            world.x * self.zoom + self.pan_offset[0],
            world.y * self.zoom + self.pan_offset[1],
        )
    }

    /// Get the center of the current viewport in world space.
    pub fn viewport_center(&self) -> Pos2 {
        Pos2::new(
            -self.pan_offset[0] / self.zoom,
            -self.pan_offset[1] / self.zoom,
        )
    }

    /// Update a node's name.
    pub fn update_node_name(&mut self, id: GraphNodeId, name: &str) {
        if let Some(node) = self.nodes.get_mut(&id) {
            node.name = name.to_string();
        }
    }

    /// Update a node's property.
    pub fn update_node_property(&mut self, id: GraphNodeId, key: String, value: String) {
        if let Some(node) = self.nodes.get_mut(&id) {
            node.properties.insert(key, value);
        }
    }

    /// Get mutable reference to node properties.
    pub fn node_properties_mut(&mut self, id: GraphNodeId) -> &mut HashMap<String, String> {
        &mut self.nodes.get_mut(&id).unwrap().properties
    }

    /// Validate the tree structure and return any errors.
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();

        // Check for multiple roots (nodes without parents that aren't the root)
        let mut parentless_nodes = Vec::new();
        for node_id in self.nodes.keys() {
            if !self.connections.values().any(|c| c.to == *node_id) {
                parentless_nodes.push(*node_id);
            }
        }
        if parentless_nodes.len() > 1 {
            errors.push(format!(
                "Multiple root nodes detected ({})",
                parentless_nodes.len()
            ));
        }

        // Check node-specific constraints
        for node in self.nodes.values() {
            if let Some(max) = node.node_type.max_children() {
                let child_count = self
                    .connections
                    .values()
                    .filter(|c| c.from == node.id)
                    .count();
                if child_count > max {
                    errors.push(format!(
                        "Node '{}' exceeds max children ({} > {})",
                        node.name, child_count, max
                    ));
                }
            }
            if node.node_type.is_leaf() {
                let child_count = self
                    .connections
                    .values()
                    .filter(|c| c.from == node.id)
                    .count();
                if child_count > 0 {
                    errors.push(format!(
                        "Leaf node '{}' has {} children",
                        node.name, child_count
                    ));
                }
            }
        }

        // Check for cycles
        if let Some(root) = self.root_node {
            if self.has_cycle(root) {
                errors.push("Cycle detected in behavior tree".to_string());
            }
        }

        errors
    }

    /// Check for cycles starting from the given node.
    fn has_cycle(&self, start: GraphNodeId) -> bool {
        let mut visited = HashSet::new();
        let mut stack = vec![start];

        while let Some(node) = stack.pop() {
            if visited.contains(&node) {
                return true;
            }
            visited.insert(node);
            for child in self.children_of(node) {
                stack.push(child.id);
            }
        }
        false
    }

    /// Handle interaction with the graph editor.
    pub fn handle_interaction(
        &mut self,
        ui: &Ui,
        response: &egui::Response,
        available_size: Vec2,
    ) -> Self {
        let mut new_state = self.clone();

        // Handle zoom with ctrl+scroll
        if response.hovered() && ui.input(|i| i.zoom_delta() != 1.0) {
            let zoom_delta = ui.input(|i| i.zoom_delta());
            if let Some(mouse_pos) = response.hover_pos() {
                let world_before = new_state.screen_to_world(mouse_pos);
                new_state.zoom = (new_state.zoom * zoom_delta).clamp(0.2, 3.0);
                let screen_after = new_state.world_to_screen(world_before);
                new_state.pan_offset[0] += mouse_pos.x - screen_after.x;
                new_state.pan_offset[1] += mouse_pos.y - screen_after.y;
            }
        }

        // Pan with middle mouse or right drag
        if response.dragged_by(egui::PointerButton::Middle)
            || (response.dragged() && ui.input(|i| i.modifiers.shift))
        {
            if let Some(delta) = response.interact_pointer_pos().map(|p| {
                let prev = ui.input(|i| i.pointer.interact_pos()).unwrap_or(p);
                p - prev
            }) {
                new_state.pan_offset[0] += delta.x;
                new_state.pan_offset[1] += delta.y;
            }
        }

        // Handle node dragging
        if response.drag_started() {
            if let Some(mouse_pos) = response.interact_pointer_pos() {
                let world_pos = new_state.screen_to_world(mouse_pos);
                // Check if we clicked on a node
                if let Some(node_id) = new_state.node_at_position(world_pos) {
                    new_state.selected_node = Some(node_id);
                    new_state.dragging_node = Some(node_id);
                } else {
                    // Check if we clicked on an output slot (start connection)
                    if let Some(node_id) = new_state.output_slot_at_position(world_pos) {
                        new_state.connecting_from = Some(node_id);
                    } else {
                        new_state.selected_node = None;
                    }
                }
            }
        }

        if response.dragged() && new_state.dragging_node.is_some() {
            if let Some(mouse_pos) = response.interact_pointer_pos() {
                let world_pos = new_state.screen_to_world(mouse_pos);
                if let Some(node) = new_state.nodes.get_mut(&new_state.dragging_node.unwrap()) {
                    let mut new_pos = world_pos - node.size() / 2.0;
                    if new_state.snap_to_grid {
                        new_pos = Pos2::new(
                            (new_pos.x / new_state.grid_size).round() * new_state.grid_size,
                            (new_pos.y / new_state.grid_size).round() * new_state.grid_size,
                        );
                    }
                    node.set_position(new_pos);
                }
            }
        }

        if response.drag_stopped() {
            if let Some(from) = new_state.connecting_from.take() {
                if let Some(mouse_pos) = response.interact_pointer_pos() {
                    let world_pos = new_state.screen_to_world(mouse_pos);
                    // Find the nearest input slot
                    let mut best_node: Option<GraphNodeId> = None;
                    let mut best_dist = f32::MAX;
                    for node in new_state.nodes.values() {
                        let slot_pos = node.input_slot_pos();
                        let dist = slot_pos.distance(world_pos);
                        if dist < best_dist && dist < 30.0 && node.id != from {
                            best_dist = dist;
                            best_node = Some(node.id);
                        }
                    }
                    if let Some(to) = best_node {
                        new_state.add_connection(from, to);
                    }
                }
            }
            new_state.dragging_node = None;
        }

        // Delete selected node with Delete key
        if ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)) {
            if let Some(selected) = new_state.selected_node {
                new_state.remove_node(selected);
            }
        }

        // Copy with Ctrl+C
        if ui.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::C)) {
            // Copy handled by parent editor
        }

        new_state
    }

    /// Draw the graph.
    pub fn draw_graph(&self, ui: &mut Ui, rect: &Rect) {
        let painter = ui.painter_at(*rect);

        // Draw grid
        if self.show_grid {
            self.draw_grid(&painter, rect);
        }

        // Draw connections
        self.draw_connections(&painter);

        // Draw nodes
        self.draw_nodes(&painter);

        // Draw connection being created
        if let Some(from_id) = self.connecting_from {
            if let Some(from_node) = self.nodes.get(&from_id) {
                if let Some(mouse_pos) = ui.ctx().input(|i| i.pointer.interact_pos()) {
                    let from_screen = self.world_to_screen(from_node.output_slot_pos());
                    let to_screen = mouse_pos;
                    self.draw_bezier_connection(&painter, from_screen, to_screen, Color32::WHITE);
                }
            }
        }
    }

    fn draw_grid(&self, painter: &egui::Painter, rect: &Rect) {
        let grid_color = Color32::from_gray(25);
        let spacing = self.grid_size * self.zoom;

        if spacing < 5.0 {
            return; // Too small to draw
        }

        let start_x = rect.min.x + (self.pan_offset[0] % spacing);
        let start_y = rect.min.y + (self.pan_offset[1] % spacing);

        let mut x = start_x;
        while x <= rect.max.x {
            painter.line_segment(
                [Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)],
                Stroke::new(0.5, grid_color),
            );
            x += spacing;
        }

        let mut y = start_y;
        while y <= rect.max.y {
            painter.line_segment(
                [Pos2::new(rect.min.x, y), Pos2::new(rect.max.x, y)],
                Stroke::new(0.5, grid_color),
            );
            y += spacing;
        }
    }

    fn draw_connections(&self, painter: &egui::Painter) {
        for conn in self.connections.values() {
            let from_node = match self.nodes.get(&conn.from) {
                Some(n) => n,
                None => continue,
            };
            let to_node = match self.nodes.get(&conn.to) {
                Some(n) => n,
                None => continue,
            };

            let from_pos = self.world_to_screen(from_node.output_slot_pos());
            let to_pos = self.world_to_screen(to_node.input_slot_pos());

            // Color based on simulation status
            let color = if let Some(ref status) = to_node.sim_status {
                match status.as_str() {
                    "Running" => Color32::YELLOW,
                    "Success" => Color32::GREEN,
                    "Failure" => Color32::RED,
                    _ => Color32::from_rgb(150, 150, 180),
                }
            } else {
                Color32::from_rgb(150, 150, 180)
            };

            let stroke = Stroke::new(2.5, color);
            self.draw_bezier_connection_stroke(painter, from_pos, to_pos, stroke);
        }
    }

    fn draw_bezier_connection(
        &self,
        painter: &egui::Painter,
        from: Pos2,
        to: Pos2,
        color: Color32,
    ) {
        let stroke = Stroke::new(2.0, color);
        self.draw_bezier_connection_stroke(painter, from, to, stroke);
    }

    fn draw_bezier_connection_stroke(
        &self,
        painter: &egui::Painter,
        from: Pos2,
        to: Pos2,
        stroke: Stroke,
    ) {
        let vertical_dist = (to.y - from.y).abs();
        let ctrl_offset = vertical_dist.max(30.0) * 0.4;
        let ctrl1 = Pos2::new(from.x, from.y + ctrl_offset);
        let ctrl2 = Pos2::new(to.x, to.y - ctrl_offset);

        let num_segments = 30;
        for i in 0..num_segments {
            let t0 = i as f32 / num_segments as f32;
            let t1 = (i + 1) as f32 / num_segments as f32;
            let p0 = cubic_bezier(from, ctrl1, ctrl2, to, t0);
            let p1 = cubic_bezier(from, ctrl1, ctrl2, to, t1);
            painter.line_segment([p0, p1], stroke);
        }

        // Arrow head at the end
        let arrow_size = 6.0;
        let p_end = cubic_bezier(from, ctrl1, ctrl2, to, 1.0);
        let p_before = cubic_bezier(from, ctrl1, ctrl2, to, 0.95);
        let angle = (p_end.y - p_before.y).atan2(p_end.x - p_before.x);

        let tip1 = Pos2::new(
            p_end.x - arrow_size * (angle.cos() - 0.5 * angle.sin()),
            p_end.y - arrow_size * (angle.sin() + 0.5 * angle.cos()),
        );
        let tip2 = Pos2::new(
            p_end.x - arrow_size * (angle.cos() + 0.5 * angle.sin()),
            p_end.y - arrow_size * (angle.sin() - 0.5 * angle.cos()),
        );

        painter.line_segment([tip1, p_end], stroke);
        painter.line_segment([tip2, p_end], stroke);
    }

    fn draw_nodes(&self, painter: &egui::Painter) {
        for node in self.nodes.values() {
            let screen_pos = self.world_to_screen(node.position());
            let screen_size = node.size() * self.zoom;
            let rect = Rect::from_min_size(screen_pos, screen_size);

            // Background
            let bg_color = node.node_type.color();
            let is_selected = self.selected_node == Some(node.id);
            let border_color = if is_selected {
                Color32::WHITE
            } else {
                Color32::from_gray(80)
            };

            // Shadow
            let shadow_rect = rect.translate(Vec2::new(3.0, 3.0));
            painter.rect_filled(shadow_rect, 6.0, Color32::from_black_alpha(60));

            // Main rect
            painter.rect_filled(rect, 6.0, bg_color);
            painter.rect_stroke(
                rect,
                6.0,
                Stroke::new(if is_selected { 2.5 } else { 1.5 }, border_color),
                egui::epaint::StrokeKind::Outside,
            );

            // Node type icon
            let icon_pos = Pos2::new(rect.min.x + 14.0, rect.center().y);
            painter.text(
                icon_pos,
                egui::Align2::CENTER_CENTER,
                node.node_type.icon(),
                egui::FontId::new(12.0, egui::FontFamily::Monospace),
                Color32::WHITE,
            );

            // Node name
            let name_pos = Pos2::new(rect.center().x + 8.0, rect.center().y);
            painter.text(
                name_pos,
                egui::Align2::CENTER_CENTER,
                &node.name,
                egui::FontId::new(11.0, egui::FontFamily::Proportional),
                Color32::WHITE,
            );

            // Simulation status indicator
            if let Some(ref status) = node.sim_status {
                let status_color = match status.as_str() {
                    "Running" => Color32::YELLOW,
                    "Success" => Color32::GREEN,
                    "Failure" => Color32::RED,
                    _ => Color32::LIGHT_GRAY,
                };
                // Status dot in top-right corner
                let dot_pos = Pos2::new(rect.max.x - 8.0, rect.min.y + 8.0);
                painter.circle_filled(dot_pos, 4.0, status_color);
            }

            // Input slot (top)
            let input_pos = self.world_to_screen(node.input_slot_pos());
            let has_parent = self.connections.values().any(|c| c.to == node.id);
            painter.circle_filled(
                input_pos,
                6.0,
                if has_parent {
                    Color32::from_rgb(180, 180, 200)
                } else {
                    Color32::from_gray(100)
                },
            );
            painter.circle_stroke(input_pos, 6.0, Stroke::new(1.5, Color32::from_gray(60)));

            // Output slot (bottom) - only if node can have children
            if node.node_type.can_have_children() {
                let output_pos = self.world_to_screen(node.output_slot_pos());
                let child_count = self
                    .connections
                    .values()
                    .filter(|c| c.from == node.id)
                    .count();
                painter.circle_filled(
                    output_pos,
                    6.0,
                    if child_count > 0 {
                        Color32::from_rgb(100, 200, 100)
                    } else {
                        Color32::from_gray(150)
                    },
                );
                painter.circle_stroke(output_pos, 6.0, Stroke::new(1.5, Color32::from_gray(60)));
            }
        }
    }

    /// Convert this graph state into the runtime BehaviorTree format.
    pub fn to_behavior_tree(&self) -> Option<quasar_ai::behavior_tree::BtNode> {
        let root_id = self.root_node?;
        self.build_node(&root_id)
    }

    fn build_node(&self, node_id: &GraphNodeId) -> Option<quasar_ai::behavior_tree::BtNode> {
        use quasar_ai::behavior_tree::BtNode as RuntimeNode;

        let node = self.nodes.get(node_id)?;
        let children: Vec<_> = self
            .children_of(*node_id)
            .iter()
            .filter_map(|child| self.build_node(&child.id))
            .collect();

        let result = match node.node_type {
            BtEditorNodeType::Selector => RuntimeNode::Sequence { children },
            BtEditorNodeType::Sequence => RuntimeNode::Sequence { children },
            BtEditorNodeType::Parallel => {
                let policy =
                    if node.properties.get("policy").map(|s| s.as_str()) == Some("RequireOne") {
                        quasar_ai::behavior_tree::ParallelPolicy::RequireOne
                    } else {
                        quasar_ai::behavior_tree::ParallelPolicy::RequireAll
                    };
                RuntimeNode::Parallel { children, policy }
            }
            BtEditorNodeType::RandomSelector => RuntimeNode::Selector { children },
            BtEditorNodeType::RandomSequence => RuntimeNode::Sequence { children },
            BtEditorNodeType::Inverter => {
                if let Some(child) = children.into_iter().next() {
                    RuntimeNode::Inverter {
                        child: Box::new(child),
                    }
                } else {
                    RuntimeNode::Succeed
                }
            }
            BtEditorNodeType::Repeater => {
                if let Some(child) = children.into_iter().next() {
                    let count = node
                        .properties
                        .get("count")
                        .and_then(|s| s.parse::<u32>().ok());
                    RuntimeNode::Repeater {
                        child: Box::new(child),
                        count,
                    }
                } else {
                    RuntimeNode::Succeed
                }
            }
            BtEditorNodeType::Succeeder => {
                if let Some(child) = children.into_iter().next() {
                    RuntimeNode::Repeater {
                        child: Box::new(child),
                        count: None,
                    }
                } else {
                    RuntimeNode::Succeed
                }
            }
            BtEditorNodeType::Failer => RuntimeNode::Fail,
            BtEditorNodeType::Timeout => {
                if let Some(child) = children.into_iter().next() {
                    let duration = node
                        .properties
                        .get("timeout")
                        .and_then(|s| s.parse::<f32>().ok())
                        .unwrap_or(5.0);
                    RuntimeNode::Timeout {
                        child: Box::new(child),
                        duration_secs: duration,
                    }
                } else {
                    RuntimeNode::Succeed
                }
            }
            BtEditorNodeType::Cooldown => {
                if let Some(child) = children.into_iter().next() {
                    RuntimeNode::Repeater {
                        child: Box::new(child),
                        count: Some(1),
                    }
                } else {
                    RuntimeNode::Succeed
                }
            }
            BtEditorNodeType::Retry => {
                if let Some(child) = children.into_iter().next() {
                    let max_tries = node
                        .properties
                        .get("max_retries")
                        .and_then(|s| s.parse::<u32>().ok())
                        .unwrap_or(3);
                    RuntimeNode::Retry {
                        child: Box::new(child),
                        max_tries,
                    }
                } else {
                    RuntimeNode::Succeed
                }
            }
            BtEditorNodeType::AlwaysRunning => RuntimeNode::Running,
            BtEditorNodeType::Action => {
                let action_name = node
                    .properties
                    .get("action_name")
                    .cloned()
                    .unwrap_or_else(|| node.name.clone());
                RuntimeNode::Action { name: action_name }
            }
            BtEditorNodeType::Condition => {
                let key = node.properties.get("key").cloned().unwrap_or_default();
                let expected_str = node
                    .properties
                    .get("expected")
                    .cloned()
                    .unwrap_or_else(|| "true".to_string());
                let expected = if expected_str == "true" {
                    quasar_ai::BlackboardValue::Bool(true)
                } else if expected_str == "false" {
                    quasar_ai::BlackboardValue::Bool(false)
                } else if let Ok(i) = expected_str.parse::<i64>() {
                    quasar_ai::BlackboardValue::Int(i)
                } else if let Ok(f) = expected_str.parse::<f32>() {
                    quasar_ai::BlackboardValue::Float(f)
                } else {
                    quasar_ai::BlackboardValue::String(expected_str)
                };
                RuntimeNode::Condition { key, expected }
            }
            BtEditorNodeType::Wait => {
                let duration = node
                    .properties
                    .get("duration")
                    .and_then(|s| s.parse::<f32>().ok())
                    .unwrap_or(1.0);
                RuntimeNode::Wait {
                    duration_secs: duration,
                }
            }
            BtEditorNodeType::SetBlackboard => RuntimeNode::Action {
                name: format!(
                    "SetBB({})",
                    node.properties.get("key").cloned().unwrap_or_default()
                ),
            },
            BtEditorNodeType::Log => RuntimeNode::Action {
                name: format!(
                    "Log({})",
                    node.properties.get("message").cloned().unwrap_or_default()
                ),
            },
            BtEditorNodeType::Comment => RuntimeNode::Succeed,
        };

        Some(result)
    }
}

impl Default for BtGraphState {
    fn default() -> Self {
        Self::new("Untitled")
    }
}

/// Cubic bezier interpolation.
fn cubic_bezier(p0: Pos2, p1: Pos2, p2: Pos2, p3: Pos2, t: f32) -> Pos2 {
    let mt = 1.0 - t;
    let mt2 = mt * mt;
    let mt3 = mt2 * mt;
    let t2 = t * t;
    let t3 = t2 * t;

    Pos2::new(
        mt3 * p0.x + 3.0 * mt2 * t * p1.x + 3.0 * mt * t2 * p2.x + t3 * p3.x,
        mt3 * p0.y + 3.0 * mt2 * t * p1.y + 3.0 * mt * t2 * p2.y + t3 * p3.y,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_creation() {
        let mut graph = BtGraphState::new("Test");
        let id = graph.add_node(BtEditorNodeType::Selector, "Root", Pos2::new(100.0, 50.0));
        assert_eq!(graph.nodes.len(), 1);
        assert_eq!(graph.root_node, Some(id));
    }

    #[test]
    fn test_connection_creation() {
        let mut graph = BtGraphState::new("Test");
        let root = graph.add_node(BtEditorNodeType::Selector, "Root", Pos2::new(100.0, 50.0));
        let child = graph.add_node(BtEditorNodeType::Action, "Act", Pos2::new(100.0, 150.0));
        let conn = graph.add_connection(root, child);
        assert!(conn.is_some());
        assert_eq!(graph.connections.len(), 1);
    }

    #[test]
    fn test_connection_prevents_cycle() {
        let mut graph = BtGraphState::new("Test");
        let a = graph.add_node(BtEditorNodeType::Selector, "A", Pos2::new(100.0, 50.0));
        let b = graph.add_node(BtEditorNodeType::Sequence, "B", Pos2::new(50.0, 150.0));
        let c = graph.add_node(BtEditorNodeType::Action, "C", Pos2::new(150.0, 150.0));

        graph.add_connection(a, b);
        graph.add_connection(b, c);

        // Try to create a cycle: c -> a
        let cycle_result = graph.add_connection(c, a);
        assert!(cycle_result.is_none());
    }

    #[test]
    fn test_connection_prevents_multiple_parents() {
        let mut graph = BtGraphState::new("Test");
        let a = graph.add_node(BtEditorNodeType::Selector, "A", Pos2::new(100.0, 50.0));
        let b = graph.add_node(BtEditorNodeType::Sequence, "B", Pos2::new(50.0, 150.0));
        let c = graph.add_node(BtEditorNodeType::Action, "C", Pos2::new(150.0, 150.0));

        graph.add_connection(a, c);
        let result = graph.add_connection(b, c); // c already has a parent
        assert!(result.is_none());
    }

    #[test]
    fn test_decorator_max_children() {
        let mut graph = BtGraphState::new("Test");
        let inv = graph.add_node(BtEditorNodeType::Inverter, "Not", Pos2::new(100.0, 50.0));
        let a = graph.add_node(BtEditorNodeType::Action, "A", Pos2::new(50.0, 150.0));
        let b = graph.add_node(BtEditorNodeType::Action, "B", Pos2::new(150.0, 150.0));

        assert!(graph.add_connection(inv, a).is_some());
        assert!(graph.add_connection(inv, b).is_none()); // Inverter can only have 1 child
    }

    #[test]
    fn test_validate_multiple_roots() {
        let mut graph = BtGraphState::new("Test");
        graph.add_node(BtEditorNodeType::Selector, "Root1", Pos2::new(100.0, 50.0));
        graph.add_node(BtEditorNodeType::Sequence, "Root2", Pos2::new(300.0, 50.0));

        let errors = graph.validate();
        assert!(!errors.is_empty());
    }

    #[test]
    fn test_children_order() {
        let mut graph = BtGraphState::new("Test");
        let root = graph.add_node(BtEditorNodeType::Selector, "Root", Pos2::new(100.0, 50.0));
        let a = graph.add_node(BtEditorNodeType::Action, "A", Pos2::new(50.0, 150.0));
        let b = graph.add_node(BtEditorNodeType::Action, "B", Pos2::new(100.0, 150.0));
        let c = graph.add_node(BtEditorNodeType::Action, "C", Pos2::new(150.0, 150.0));

        graph.add_connection(root, a);
        graph.add_connection(root, b);
        graph.add_connection(root, c);

        let children = graph.children_of(root);
        assert_eq!(children.len(), 3);
        assert_eq!(children[0].name, "A");
        assert_eq!(children[1].name, "B");
        assert_eq!(children[2].name, "C");
    }

    #[test]
    fn test_screen_to_world() {
        let mut graph = BtGraphState::new("Test");
        graph.pan_offset = [100.0, 50.0];
        graph.zoom = 2.0;

        let screen = Pos2::new(300.0, 150.0);
        let world = graph.screen_to_world(screen);

        assert!((world.x - 100.0).abs() < 0.01);
        assert!((world.y - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_node_type_categories() {
        assert!(BtEditorNodeType::Selector.is_composite());
        assert!(BtEditorNodeType::Inverter.is_decorator());
        assert!(BtEditorNodeType::Action.is_leaf());
        assert!(!BtEditorNodeType::Action.is_composite());
    }

    #[test]
    fn bt_editor_node_creation() {
        let node = BtEditorNode::new(
            GraphNodeId(1),
            BtEditorNodeType::Action,
            "MyAction",
            Pos2::new(100.0, 50.0),
        );
        assert_eq!(node.id, GraphNodeId(1));
        assert_eq!(node.name, "MyAction");
        assert!(!node.properties.is_empty());
    }
}
