//! Visual Behavior Tree Editor for Quasar Engine.
//!
//! Provides:
//! - **Visual node editing** — drag-and-drop node placement
//! - **Connection management** — create/delete node connections
//! - **Copy/paste** — duplicate subtrees
//! - **Undo/redo** — full edit history
//! - **Serialization** — save/load tree definitions

use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConnectionId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BtNodeType {
    Selector,
    Sequence,
    Parallel,
    Inverter,
    Repeater,
    Succeeder,
    Timeout,
    WaitFor,
    Action,
    Condition,
    Set,
    Increment,
    RandomSelector,
    RandomSequence,
    Cooldown,
    Guard,
    Constant,
    Noop,
}

impl BtNodeType {
    pub fn name(&self) -> &'static str {
        match self {
            BtNodeType::Selector => "Selector",
            BtNodeType::Sequence => "Sequence",
            BtNodeType::Parallel => "Parallel",
            BtNodeType::Inverter => "Inverter",
            BtNodeType::Repeater => "Repeater",
            BtNodeType::Succeeder => "Succeeder",
            BtNodeType::Timeout => "Timeout",
            BtNodeType::WaitFor => "Wait For",
            BtNodeType::Action => "Action",
            BtNodeType::Condition => "Condition",
            BtNodeType::Set => "Set",
            BtNodeType::Increment => "Increment",
            BtNodeType::RandomSelector => "Random Selector",
            BtNodeType::RandomSequence => "Random Sequence",
            BtNodeType::Cooldown => "Cooldown",
            BtNodeType::Guard => "Guard",
            BtNodeType::Constant => "Constant",
            BtNodeType::Noop => "Noop",
        }
    }

    pub fn color(&self) -> Color32 {
        match self {
            BtNodeType::Selector => Color32::from_rgb(100, 150, 200),
            BtNodeType::Sequence => Color32::from_rgb(100, 200, 150),
            BtNodeType::Parallel => Color32::from_rgb(150, 100, 200),
            BtNodeType::Inverter => Color32::from_rgb(200, 150, 100),
            BtNodeType::Repeater => Color32::from_rgb(200, 100, 150),
            BtNodeType::Succeeder => Color32::from_rgb(150, 200, 100),
            BtNodeType::Timeout => Color32::from_rgb(200, 200, 100),
            BtNodeType::WaitFor => Color32::from_rgb(100, 200, 200),
            BtNodeType::Action => Color32::from_rgb(200, 100, 100),
            BtNodeType::Condition => Color32::from_rgb(100, 100, 200),
            BtNodeType::Set => Color32::from_rgb(200, 150, 200),
            BtNodeType::Increment => Color32::from_rgb(150, 200, 200),
            BtNodeType::RandomSelector => Color32::from_rgb(180, 140, 160),
            BtNodeType::RandomSequence => Color32::from_rgb(160, 180, 140),
            BtNodeType::Cooldown => Color32::from_rgb(180, 160, 140),
            BtNodeType::Guard => Color32::from_rgb(140, 160, 180),
            BtNodeType::Constant => Color32::from_rgb(128, 128, 128),
            BtNodeType::Noop => Color32::from_rgb(80, 80, 80),
        }
    }

    pub fn can_have_children(&self) -> bool {
        matches!(
            self,
            BtNodeType::Selector
                | BtNodeType::Sequence
                | BtNodeType::Parallel
                | BtNodeType::Inverter
                | BtNodeType::Repeater
                | BtNodeType::Succeeder
                | BtNodeType::Timeout
                | BtNodeType::Cooldown
                | BtNodeType::Guard
                | BtNodeType::RandomSelector
                | BtNodeType::RandomSequence
        )
    }

    pub fn max_children(&self) -> Option<usize> {
        match self {
            BtNodeType::Inverter
            | BtNodeType::Repeater
            | BtNodeType::Succeeder
            | BtNodeType::Timeout
            | BtNodeType::Cooldown
            | BtNodeType::Guard => Some(1),
            _ => None,
        }
    }

    pub fn is_composite(&self) -> bool {
        matches!(
            self,
            BtNodeType::Selector
                | BtNodeType::Sequence
                | BtNodeType::Parallel
                | BtNodeType::RandomSelector
                | BtNodeType::RandomSequence
        )
    }

    pub fn is_decorator(&self) -> bool {
        matches!(
            self,
            BtNodeType::Inverter
                | BtNodeType::Repeater
                | BtNodeType::Succeeder
                | BtNodeType::Timeout
                | BtNodeType::Cooldown
                | BtNodeType::Guard
        )
    }

    pub fn is_leaf(&self) -> bool {
        matches!(
            self,
            BtNodeType::Action
                | BtNodeType::Condition
                | BtNodeType::Set
                | BtNodeType::Increment
                | BtNodeType::WaitFor
                | BtNodeType::Constant
                | BtNodeType::Noop
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BtNode {
    pub id: NodeId,
    pub node_type: BtNodeType,
    pub position: [f32; 2],
    pub name: String,
    pub properties: HashMap<String, String>,
}

impl BtNode {
    pub fn new(id: NodeId, node_type: BtNodeType, position: Pos2) -> Self {
        Self {
            id,
            node_type,
            position: [position.x, position.y],
            name: node_type.name().to_string(),
            properties: HashMap::new(),
        }
    }

    pub fn position(&self) -> Pos2 {
        Pos2::new(self.position[0], self.position[1])
    }

    pub fn set_position(&mut self, pos: Pos2) {
        self.position = [pos.x, pos.y];
    }

    pub fn rect(&self) -> Rect {
        let pos = self.position();
        let size = self.size();
        Rect::from_min_size(pos, size)
    }

    pub fn size(&self) -> Vec2 {
        Vec2::new(140.0, 60.0)
    }

    pub fn input_slot_position(&self) -> Pos2 {
        let pos = self.position();
        Pos2::new(pos.x + self.size().x / 2.0, pos.y)
    }

    pub fn output_slot_position(&self) -> Pos2 {
        let pos = self.position();
        Pos2::new(pos.x + self.size().x / 2.0, pos.y + self.size().y)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BtConnection {
    pub id: ConnectionId,
    pub from_node: NodeId,
    pub to_node: NodeId,
    pub order: u32,
}

impl BtConnection {
    pub fn new(id: ConnectionId, from: NodeId, to: NodeId, order: u32) -> Self {
        Self {
            id,
            from_node: from,
            to_node: to,
            order,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BtTreeDef {
    pub name: String,
    pub nodes: HashMap<NodeId, BtNode>,
    pub connections: HashMap<ConnectionId, BtConnection>,
    pub root_node: Option<NodeId>,
}

impl BtTreeDef {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            nodes: HashMap::new(),
            connections: HashMap::new(),
            root_node: None,
        }
    }

    pub fn add_node(&mut self, node: BtNode) {
        if self.nodes.is_empty() {
            self.root_node = Some(node.id);
        }
        self.nodes.insert(node.id, node);
    }

    pub fn remove_node(&mut self, id: NodeId) {
        self.nodes.remove(&id);
        self.connections
            .retain(|_, c| c.from_node != id && c.to_node != id);
        if self.root_node == Some(id) {
            self.root_node = self.nodes.keys().next().copied();
        }
    }

    pub fn connect(&mut self, id: ConnectionId, from: NodeId, to: NodeId) -> bool {
        if !self.nodes.contains_key(&from) || !self.nodes.contains_key(&to) {
            return false;
        }

        if self.connections.values().any(|c| c.to_node == to) {
            return false;
        }

        let from_node = &self.nodes[&from];
        if let Some(max) = from_node.node_type.max_children() {
            let child_count = self
                .connections
                .values()
                .filter(|c| c.from_node == from)
                .count();
            if child_count >= max {
                return false;
            }
        }

        let order = self
            .connections
            .values()
            .filter(|c| c.from_node == from)
            .count() as u32;
        let conn = BtConnection::new(id, from, to, order);
        self.connections.insert(conn.id, conn);
        true
    }

    pub fn disconnect(&mut self, id: ConnectionId) {
        self.connections.remove(&id);
    }

    pub fn children_of(&self, node_id: NodeId) -> Vec<&BtNode> {
        let mut children: Vec<_> = self
            .connections
            .values()
            .filter(|c| c.from_node == node_id)
            .map(|c| &self.nodes[&c.to_node])
            .collect();
        children.sort_by_key(|n| {
            self.connections
                .values()
                .find(|c| c.to_node == n.id)
                .map(|c| c.order)
                .unwrap_or(0)
        });
        children
    }

    pub fn parent_of(&self, node_id: NodeId) -> Option<&BtNode> {
        self.connections
            .values()
            .find(|c| c.to_node == node_id)
            .and_then(|c| self.nodes.get(&c.from_node))
    }

    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();

        for node in self.nodes.values() {
            if node.node_type.max_children() == Some(1) {
                let child_count = self
                    .connections
                    .values()
                    .filter(|c| c.from_node == node.id)
                    .count();
                if child_count > 1 {
                    errors.push(format!(
                        "Node '{}' ({:?}) can only have 1 child but has {}",
                        node.name, node.node_type, child_count
                    ));
                }
            }

            if node.node_type.is_leaf() {
                let child_count = self
                    .connections
                    .values()
                    .filter(|c| c.from_node == node.id)
                    .count();
                if child_count > 0 {
                    errors.push(format!(
                        "Node '{}' ({:?}) is a leaf but has children",
                        node.name, node.node_type
                    ));
                }
            }
        }

        for node in self.nodes.values() {
            if node.id != self.root_node.unwrap_or(NodeId(0))
                && !self.connections.values().any(|c| c.to_node == node.id) {
                    errors.push(format!("Node '{}' is not connected to the tree", node.name));
                }
        }

        errors
    }
}

pub struct BehaviorTreeEditor {
    tree: BtTreeDef,
    selected_node: Option<NodeId>,
    dragging_node: Option<NodeId>,
    connecting_from: Option<NodeId>,
    next_node_id: u64,
    next_conn_id: u64,
    pan_offset: Vec2,
    zoom: f32,
    undo_stack: Vec<BtTreeDef>,
    redo_stack: Vec<BtTreeDef>,
    clipboard: Option<Vec<BtNode>>,
    show_grid: bool,
    snap_to_grid: bool,
    grid_size: f32,
}

impl BehaviorTreeEditor {
    pub fn new() -> Self {
        Self {
            tree: BtTreeDef::new("New Behavior Tree"),
            selected_node: None,
            dragging_node: None,
            connecting_from: None,
            next_node_id: 1,
            next_conn_id: 1,
            pan_offset: Vec2::ZERO,
            zoom: 1.0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            clipboard: None,
            show_grid: true,
            snap_to_grid: true,
            grid_size: 20.0,
        }
    }

    pub fn with_tree(tree: BtTreeDef) -> Self {
        let max_node = tree.nodes.keys().map(|id| id.0).max().unwrap_or(0);
        let max_conn = tree.connections.keys().map(|id| id.0).max().unwrap_or(0);
        Self {
            tree,
            next_node_id: max_node + 1,
            next_conn_id: max_conn + 1,
            ..Self::new()
        }
    }

    pub fn tree(&self) -> &BtTreeDef {
        &self.tree
    }

    pub fn tree_mut(&mut self) -> &mut BtTreeDef {
        &mut self.tree
    }

    pub fn save_state(&mut self) {
        self.undo_stack.push(self.tree.clone());
        self.redo_stack.clear();
        if self.undo_stack.len() > 50 {
            self.undo_stack.remove(0);
        }
    }

    pub fn undo(&mut self) {
        if let Some(state) = self.undo_stack.pop() {
            self.redo_stack.push(self.tree.clone());
            self.tree = state;
        }
    }

    pub fn redo(&mut self) {
        if let Some(state) = self.redo_stack.pop() {
            self.undo_stack.push(self.tree.clone());
            self.tree = state;
        }
    }

    pub fn add_node(&mut self, node_type: BtNodeType, position: Pos2) -> NodeId {
        self.save_state();
        let id = NodeId(self.next_node_id);
        self.next_node_id += 1;

        let pos = if self.snap_to_grid {
            Pos2::new(
                (position.x / self.grid_size).round() * self.grid_size,
                (position.y / self.grid_size).round() * self.grid_size,
            )
        } else {
            position
        };

        let node = BtNode::new(id, node_type, pos);
        self.tree.add_node(node);
        self.selected_node = Some(id);
        id
    }

    pub fn delete_selected(&mut self) {
        if let Some(id) = self.selected_node {
            self.save_state();
            self.tree.remove_node(id);
            self.selected_node = None;
        }
    }

    pub fn copy_selected(&mut self) {
        if let Some(id) = self.selected_node {
            if let Some(node) = self.tree.nodes.get(&id).cloned() {
                self.clipboard = Some(vec![node]);
            }
        }
    }

    pub fn paste(&mut self, offset: Vec2) {
        if let Some(nodes) = self.clipboard.clone() {
            self.save_state();
            for node in &nodes {
                let new_id = NodeId(self.next_node_id);
                self.next_node_id += 1;
                let pos = Pos2::new(node.position[0] + offset.x, node.position[1] + offset.y);
                let new_node = BtNode {
                    id: new_id,
                    position: [pos.x, pos.y],
                    ..node.clone()
                };
                self.tree.add_node(new_node);
                self.selected_node = Some(new_id);
            }
        }
    }

    pub fn screen_to_world(&self, screen: Pos2) -> Pos2 {
        Pos2::new(
            (screen.x - self.pan_offset.x) / self.zoom,
            (screen.y - self.pan_offset.y) / self.zoom,
        )
    }

    pub fn world_to_screen(&self, world: Pos2) -> Pos2 {
        Pos2::new(
            world.x * self.zoom + self.pan_offset.x,
            world.y * self.zoom + self.pan_offset.y,
        )
    }

    pub fn node_at_position(&self, pos: Pos2) -> Option<NodeId> {
        for node in self.tree.nodes.values() {
            if node.rect().contains(pos) {
                return Some(node.id);
            }
        }
        None
    }

    pub fn show(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(&self.tree.name);
            if ui.button("Add Root").clicked() {
                self.add_node(BtNodeType::Selector, Pos2::new(200.0, 50.0));
            }
            if ui.button("Delete").clicked() {
                self.delete_selected();
            }
            if ui.button("Undo").clicked() {
                self.undo();
            }
            if ui.button("Redo").clicked() {
                self.redo();
            }
        });

        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());

        if self.show_grid {
            self.draw_grid(&painter);
        }

        self.draw_connections(&painter);

        self.draw_nodes(&painter, ui);

        if response.drag_started() {
            if let Some(hover_pos) = response.hover_pos() {
                let world_pos = self.screen_to_world(hover_pos);
                if let Some(id) = self.node_at_position(world_pos) {
                    self.dragging_node = Some(id);
                } else {
                    self.connecting_from = self.selected_node;
                }
            }
        }

        if response.dragged() {
            if let Some(id) = self.dragging_node {
                if let Some(hover_pos) = response.hover_pos() {
                    let world_pos = self.screen_to_world(hover_pos);
                    if let Some(node) = self.tree.nodes.get_mut(&id) {
                        let new_pos = world_pos - node.size() / 2.0;
                        node.set_position(if self.snap_to_grid {
                            Pos2::new(
                                (new_pos.x / self.grid_size).round() * self.grid_size,
                                (new_pos.y / self.grid_size).round() * self.grid_size,
                            )
                        } else {
                            new_pos
                        });
                    }
                }
            }
        }

        if response.drag_stopped() {
            if let Some(from) = self.connecting_from {
                if let Some(hover_pos) = response.hover_pos() {
                    let world_pos = self.screen_to_world(hover_pos);
                    if let Some(to) = self.node_at_position(world_pos) {
                        if from != to {
                            self.save_state();
                            let conn_id = ConnectionId(self.next_conn_id);
                            self.next_conn_id += 1;
                            self.tree.connect(conn_id, from, to);
                        }
                    }
                }
            }
            self.dragging_node = None;
            self.connecting_from = None;
        }

        if response.clicked() {
            if let Some(hover_pos) = response.hover_pos() {
                let world_pos = self.screen_to_world(hover_pos);
                self.selected_node = self.node_at_position(world_pos);
            }
        }
    }

    fn draw_grid(&self, painter: &egui::Painter) {
        let grid_color = Color32::from_gray(30);
        let spacing = self.grid_size * self.zoom;

        let visible_rect = painter.clip_rect();

        let start_x = (visible_rect.min.x / spacing).floor() * spacing;
        let start_y = (visible_rect.min.y / spacing).floor() * spacing;

        let mut x = start_x;
        while x <= visible_rect.max.x {
            painter.line_segment(
                [
                    Pos2::new(x, visible_rect.min.y),
                    Pos2::new(x, visible_rect.max.y),
                ],
                Stroke::new(1.0, grid_color),
            );
            x += spacing;
        }

        let mut y = start_y;
        while y <= visible_rect.max.y {
            painter.line_segment(
                [
                    Pos2::new(visible_rect.min.x, y),
                    Pos2::new(visible_rect.max.x, y),
                ],
                Stroke::new(1.0, grid_color),
            );
            y += spacing;
        }
    }

    fn draw_connections(&self, painter: &egui::Painter) {
        for conn in self.tree.connections.values() {
            let from_node = match self.tree.nodes.get(&conn.from_node) {
                Some(n) => n,
                None => continue,
            };
            let to_node = match self.tree.nodes.get(&conn.to_node) {
                Some(n) => n,
                None => continue,
            };

            let from_pos = self.world_to_screen(from_node.output_slot_position());
            let to_pos = self.world_to_screen(to_node.input_slot_position());

            let ctrl1 = Pos2::new(from_pos.x, from_pos.y + 50.0);
            let ctrl2 = Pos2::new(to_pos.x, to_pos.y - 50.0);

            let color = Color32::from_rgb(150, 150, 200);
            let stroke = Stroke::new(2.0, color);

            let num_segments = 20;
            for i in 0..num_segments {
                let t0 = i as f32 / num_segments as f32;
                let t1 = (i + 1) as f32 / num_segments as f32;

                let p0 = cubic_bezier(from_pos, ctrl1, ctrl2, to_pos, t0);
                let p1 = cubic_bezier(from_pos, ctrl1, ctrl2, to_pos, t1);

                painter.line_segment([p0, p1], stroke);
            }
        }
    }

    fn draw_nodes(&mut self, painter: &egui::Painter, _ui: &mut Ui) {
        for node in self.tree.nodes.values() {
            let screen_pos = self.world_to_screen(node.position());
            let screen_size = node.size() * self.zoom;
            let rect = Rect::from_min_size(screen_pos, screen_size);

            let bg_color = node.node_type.color();
            let border_color = if self.selected_node == Some(node.id) {
                Color32::WHITE
            } else {
                Color32::from_gray(100)
            };

            painter.rect_filled(rect, 5.0, bg_color);
            painter.rect_stroke(
                rect,
                5.0,
                Stroke::new(2.0, border_color),
                egui::epaint::StrokeKind::Outside,
            );

            let text_pos = Pos2::new(rect.center().x, rect.center().y);
            painter.text(
                text_pos,
                egui::Align2::CENTER_CENTER,
                &node.name,
                egui::FontId::default(),
                Color32::WHITE,
            );

            if node.node_type.can_have_children() {
                let slot_pos = self.world_to_screen(node.output_slot_position());
                painter.circle_filled(slot_pos, 6.0, Color32::WHITE);
                painter.circle_stroke(slot_pos, 6.0, Stroke::new(2.0, Color32::from_gray(50)));
            }

            let input_pos = self.world_to_screen(node.input_slot_position());
            painter.circle_filled(input_pos, 6.0, Color32::from_gray(200));
            painter.circle_stroke(input_pos, 6.0, Stroke::new(2.0, Color32::from_gray(50)));
        }
    }
}

impl Default for BehaviorTreeEditor {
    fn default() -> Self {
        Self::new()
    }
}

fn cubic_bezier(p0: Pos2, p1: Pos2, p2: Pos2, p3: Pos2, t: f32) -> Pos2 {
    let t2 = t * t;
    let t3 = t2 * t;
    let mt = 1.0 - t;
    let mt2 = mt * mt;
    let mt3 = mt2 * mt;

    Pos2::new(
        mt3 * p0.x + 3.0 * mt2 * t * p1.x + 3.0 * mt * t2 * p2.x + t3 * p3.x,
        mt3 * p0.y + 3.0 * mt2 * t * p1.y + 3.0 * mt * t2 * p2.y + t3 * p3.y,
    )
}

pub fn create_default_tree() -> BtTreeDef {
    let mut tree = BtTreeDef::new("Patrol AI");

    let root = BtNode::new(NodeId(1), BtNodeType::Selector, Pos2::new(200.0, 50.0));
    let seq = BtNode::new(NodeId(2), BtNodeType::Sequence, Pos2::new(100.0, 150.0));
    let cond = BtNode::new(NodeId(3), BtNodeType::Condition, Pos2::new(50.0, 250.0));
    let action = BtNode::new(NodeId(4), BtNodeType::Action, Pos2::new(150.0, 250.0));
    let patrol = BtNode::new(NodeId(5), BtNodeType::Action, Pos2::new(300.0, 150.0));

    tree.add_node(root);
    tree.add_node(seq);
    tree.add_node(cond);
    tree.add_node(action);
    tree.add_node(patrol);

    tree.connect(ConnectionId(1), NodeId(1), NodeId(2));
    tree.connect(ConnectionId(2), NodeId(1), NodeId(5));
    tree.connect(ConnectionId(3), NodeId(2), NodeId(3));
    tree.connect(ConnectionId(4), NodeId(2), NodeId(4));

    tree
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bt_node_creation() {
        let node = BtNode::new(NodeId(1), BtNodeType::Selector, Pos2::new(100.0, 50.0));
        assert_eq!(node.id, NodeId(1));
        assert_eq!(node.node_type, BtNodeType::Selector);
    }

    #[test]
    fn bt_tree_def_creation() {
        let tree = BtTreeDef::new("Test Tree");
        assert_eq!(tree.name, "Test Tree");
        assert!(tree.nodes.is_empty());
    }

    #[test]
    fn bt_tree_add_node() {
        let mut tree = BtTreeDef::new("Test");
        let node = BtNode::new(NodeId(1), BtNodeType::Selector, Pos2::new(0.0, 0.0));
        tree.add_node(node);

        assert_eq!(tree.nodes.len(), 1);
        assert_eq!(tree.root_node, Some(NodeId(1)));
    }

    #[test]
    fn bt_tree_connect() {
        let mut tree = BtTreeDef::new("Test");
        let parent = BtNode::new(NodeId(1), BtNodeType::Selector, Pos2::new(0.0, 0.0));
        let child = BtNode::new(NodeId(2), BtNodeType::Action, Pos2::new(0.0, 100.0));
        tree.add_node(parent);
        tree.add_node(child);

        assert!(tree.connect(ConnectionId(1), NodeId(1), NodeId(2)));
        assert_eq!(tree.connections.len(), 1);
    }

    #[test]
    fn bt_tree_disconnect() {
        let mut tree = BtTreeDef::new("Test");
        tree.add_node(BtNode::new(
            NodeId(1),
            BtNodeType::Selector,
            Pos2::new(0.0, 0.0),
        ));
        tree.add_node(BtNode::new(
            NodeId(2),
            BtNodeType::Action,
            Pos2::new(0.0, 100.0),
        ));
        tree.connect(ConnectionId(1), NodeId(1), NodeId(2));

        tree.disconnect(ConnectionId(1));
        assert_eq!(tree.connections.len(), 0);
    }

    #[test]
    fn bt_tree_children_of() {
        let mut tree = BtTreeDef::new("Test");
        tree.add_node(BtNode::new(
            NodeId(1),
            BtNodeType::Selector,
            Pos2::new(0.0, 0.0),
        ));
        tree.add_node(BtNode::new(
            NodeId(2),
            BtNodeType::Action,
            Pos2::new(0.0, 100.0),
        ));
        tree.add_node(BtNode::new(
            NodeId(3),
            BtNodeType::Action,
            Pos2::new(100.0, 100.0),
        ));
        tree.connect(ConnectionId(1), NodeId(1), NodeId(2));
        tree.connect(ConnectionId(2), NodeId(1), NodeId(3));

        let children = tree.children_of(NodeId(1));
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn bt_editor_undo_redo() {
        let mut editor = BehaviorTreeEditor::new();
        editor.add_node(BtNodeType::Selector, Pos2::new(0.0, 0.0));
        assert_eq!(editor.tree().nodes.len(), 1);

        editor.undo();
        assert_eq!(editor.tree().nodes.len(), 0);

        editor.redo();
        assert_eq!(editor.tree().nodes.len(), 1);
    }

    #[test]
    fn create_default_tree_valid() {
        let tree = create_default_tree();
        let errors = tree.validate();
        assert!(
            errors.is_empty(),
            "Default tree should be valid: {:?}",
            errors
        );
    }
}
