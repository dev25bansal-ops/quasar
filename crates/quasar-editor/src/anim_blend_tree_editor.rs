//! Visual Animation Blend Tree Editor for Quasar Engine.
//!
//! Provides:
//! - **Visual node editing** — drag-and-drop blend nodes
//! - **Blend types** — 1D, 2D blending, additive, direct
//! - **Parameter binding** — link to animation parameters
//! - **Live preview** — see blend results in-editor
//! - **Serialization** — save/load blend trees

use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlendNodeId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlendNodeType {
    Clip,
    Blend1D,
    Blend2D,
    Additive,
    Layer,
    Mask,
    Output,
}

impl BlendNodeType {
    pub fn name(&self) -> &'static str {
        match self {
            BlendNodeType::Clip => "Animation Clip",
            BlendNodeType::Blend1D => "Blend 1D",
            BlendNodeType::Blend2D => "Blend 2D",
            BlendNodeType::Additive => "Additive",
            BlendNodeType::Layer => "Layer",
            BlendNodeType::Mask => "Bone Mask",
            BlendNodeType::Output => "Output",
        }
    }

    pub fn color(&self) -> Color32 {
        match self {
            BlendNodeType::Clip => Color32::from_rgb(100, 150, 200),
            BlendNodeType::Blend1D => Color32::from_rgb(100, 200, 150),
            BlendNodeType::Blend2D => Color32::from_rgb(150, 200, 100),
            BlendNodeType::Additive => Color32::from_rgb(200, 150, 100),
            BlendNodeType::Layer => Color32::from_rgb(150, 100, 200),
            BlendNodeType::Mask => Color32::from_rgb(200, 100, 150),
            BlendNodeType::Output => Color32::from_rgb(200, 100, 100),
        }
    }

    pub fn input_count(&self) -> usize {
        match self {
            BlendNodeType::Clip => 0,
            BlendNodeType::Blend1D => 2,
            BlendNodeType::Blend2D => 4,
            BlendNodeType::Additive => 2,
            BlendNodeType::Layer => 2,
            BlendNodeType::Mask => 1,
            BlendNodeType::Output => 1,
        }
    }

    pub fn output_count(&self) -> usize {
        match self {
            BlendNodeType::Output => 0,
            _ => 1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlendParameter {
    pub name: String,
    pub min_value: f32,
    pub max_value: f32,
    pub default_value: f32,
}

impl BlendParameter {
    pub fn new(name: &str, min: f32, max: f32, default: f32) -> Self {
        Self {
            name: name.to_string(),
            min_value: min,
            max_value: max,
            default_value,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlendTreeNode {
    pub id: BlendNodeId,
    pub node_type: BlendNodeType,
    pub position: [f32; 2],
    pub name: String,
    pub clip_name: Option<String>,
    pub parameters: Vec<String>,
    pub speed: f32,
    pub loop_animation: bool,
    pub blend_threshold: f32,
    pub bone_mask: Vec<String>,
}

impl BlendTreeNode {
    pub fn new(id: BlendNodeId, node_type: BlendNodeType, position: Pos2) -> Self {
        Self {
            id,
            node_type,
            position: [position.x, position.y],
            name: node_type.name().to_string(),
            clip_name: None,
            parameters: Vec::new(),
            speed: 1.0,
            loop_animation: true,
            blend_threshold: 0.1,
            bone_mask: Vec::new(),
        }
    }

    pub fn position(&self) -> Pos2 {
        Pos2::new(self.position[0], self.position[1])
    }

    pub fn set_position(&mut self, pos: Pos2) {
        self.position = [pos.x, pos.y];
    }

    pub fn size(&self) -> Vec2 {
        match self.node_type {
            BlendNodeType::Clip => Vec2::new(160.0, 80.0),
            BlendNodeType::Blend1D => Vec2::new(180.0, 100.0),
            BlendNodeType::Blend2D => Vec2::new(200.0, 120.0),
            BlendNodeType::Additive => Vec2::new(160.0, 80.0),
            BlendNodeType::Layer => Vec2::new(160.0, 80.0),
            BlendNodeType::Mask => Vec2::new(140.0, 60.0),
            BlendNodeType::Output => Vec2::new(120.0, 50.0),
        }
    }

    pub fn rect(&self) -> Rect {
        Rect::from_min_size(self.position(), self.size())
    }

    pub fn input_slot_position(&self, index: usize) -> Pos2 {
        let pos = self.position();
        let size = self.size();
        let input_count = self.node_type.input_count();

        if input_count == 0 {
            return pos;
        }

        let spacing = size.y / (input_count + 1) as f32;
        Pos2::new(pos.x, pos.y + spacing * (index + 1) as f32)
    }

    pub fn output_slot_position(&self) -> Pos2 {
        let pos = self.position();
        let size = self.size();
        Pos2::new(pos.x + size.x, pos.y + size.y / 2.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlendConnection {
    pub from_node: BlendNodeId,
    pub to_node: BlendNodeId,
    pub to_input: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlendTreeDef {
    pub name: String,
    pub nodes: HashMap<BlendNodeId, BlendTreeNode>,
    pub connections: Vec<BlendConnection>,
    pub parameters: Vec<BlendParameter>,
    pub output_node: Option<BlendNodeId>,
}

impl BlendTreeDef {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            nodes: HashMap::new(),
            connections: Vec::new(),
            parameters: vec![
                BlendParameter::new("speed", 0.0, 2.0, 1.0),
                BlendParameter::new("direction", -1.0, 1.0, 0.0),
            ],
            output_node: None,
        }
    }

    pub fn add_node(&mut self, node: BlendTreeNode) {
        if node.node_type == BlendNodeType::Output {
            self.output_node = Some(node.id);
        }
        self.nodes.insert(node.id, node);
    }

    pub fn remove_node(&mut self, id: BlendNodeId) {
        self.nodes.remove(&id);
        self.connections
            .retain(|c| c.from_node != id && c.to_node != id);
        if self.output_node == Some(id) {
            self.output_node = self
                .nodes
                .iter()
                .find(|(_, n)| n.node_type == BlendNodeType::Output)
                .map(|(id, _)| *id);
        }
    }

    pub fn connect(&mut self, from: BlendNodeId, to: BlendNodeId, to_input: usize) -> bool {
        if !self.nodes.contains_key(&from) || !self.nodes.contains_key(&to) {
            return false;
        }

        if let Some(to_node) = self.nodes.get(&to) {
            if to_input >= to_node.node_type.input_count() {
                return false;
            }
        }

        self.connections
            .retain(|c| !(c.to_node == to && c.to_input == to_input));
        self.connections.push(BlendConnection {
            from_node: from,
            to_node: to,
            to_input,
        });
        true
    }

    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();

        if self.output_node.is_none() {
            errors.push("No output node found".to_string());
        }

        for node in self.nodes.values() {
            if node.node_type == BlendNodeType::Clip && node.clip_name.is_none() {
                errors.push(format!(
                    "Clip node '{}' has no animation assigned",
                    node.name
                ));
            }

            if matches!(
                node.node_type,
                BlendNodeType::Blend1D | BlendNodeType::Blend2D
            ) && node.parameters.is_empty()
            {
                errors.push(format!(
                    "Blend node '{}' has no parameter binding",
                    node.name
                ));
            }
        }

        for node in self.nodes.values() {
            if node.node_type != BlendNodeType::Output {
                let has_output = self.connections.iter().any(|c| c.from_node == node.id);
                if !has_output && self.output_node != Some(node.id) {
                    errors.push(format!("Node '{}' output is not connected", node.name));
                }
            }
        }

        errors
    }

    pub fn evaluate(&self, param_values: &HashMap<String, f32>) -> Option<String> {
        let output_id = self.output_node?;

        fn find_clip(
            nodes: &HashMap<BlendNodeId, BlendTreeNode>,
            connections: &[BlendConnection],
            node_id: BlendNodeId,
            params: &HashMap<String, f32>,
        ) -> Option<String> {
            let node = nodes.get(&node_id)?;

            match node.node_type {
                BlendNodeType::Clip => node.clip_name.clone(),
                BlendNodeType::Blend1D => {
                    let param_value = params.get(node.parameters.first()?).copied().unwrap_or(0.5);
                    let inputs: Vec<_> = connections
                        .iter()
                        .filter(|c| c.to_node == node_id)
                        .sorted_by_key(|c| c.to_input)
                        .collect();

                    if inputs.len() >= 2 {
                        let blend = (param_value + 1.0) / 2.0;
                        let source = if blend < 0.5 { inputs[0] } else { inputs[1] };
                        find_clip(nodes, connections, source.from_node, params)
                    } else {
                        None
                    }
                }
                BlendNodeType::Output => {
                    let input = connections.iter().find(|c| c.to_node == node_id)?;
                    find_clip(nodes, connections, input.from_node, params)
                }
                _ => None,
            }
        }

        find_clip(&self.nodes, &self.connections, output_id, param_values)
    }
}

pub struct AnimBlendTreeEditor {
    tree: BlendTreeDef,
    selected_node: Option<BlendNodeId>,
    dragging_node: Option<BlendNodeId>,
    connecting_from: Option<BlendNodeId>,
    next_node_id: u64,
    pan_offset: Vec2,
    zoom: f32,
    undo_stack: Vec<BlendTreeDef>,
    redo_stack: Vec<BlendTreeDef>,
    show_grid: bool,
    grid_size: f32,
    preview_params: HashMap<String, f32>,
}

impl AnimBlendTreeEditor {
    pub fn new() -> Self {
        Self {
            tree: BlendTreeDef::new("New Blend Tree"),
            selected_node: None,
            dragging_node: None,
            connecting_from: None,
            next_node_id: 1,
            pan_offset: Vec2::ZERO,
            zoom: 1.0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            show_grid: true,
            grid_size: 20.0,
            preview_params: HashMap::new(),
        }
    }

    pub fn with_tree(tree: BlendTreeDef) -> Self {
        let max_node = tree.nodes.keys().map(|id| id.0).max().unwrap_or(0);
        let preview_params = tree
            .parameters
            .iter()
            .map(|p| (p.name.clone(), p.default_value))
            .collect();
        Self {
            tree,
            next_node_id: max_node + 1,
            preview_params,
            ..Self::new()
        }
    }

    pub fn tree(&self) -> &BlendTreeDef {
        &self.tree
    }

    pub fn tree_mut(&mut self) -> &mut BlendTreeDef {
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

    pub fn add_node(&mut self, node_type: BlendNodeType, position: Pos2) -> BlendNodeId {
        self.save_state();
        let id = BlendNodeId(self.next_node_id);
        self.next_node_id += 1;

        let pos = Pos2::new(
            (position.x / self.grid_size).round() * self.grid_size,
            (position.y / self.grid_size).round() * self.grid_size,
        );

        let node = BlendTreeNode::new(id, node_type, pos);
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

    pub fn node_at_position(&self, pos: Pos2) -> Option<BlendNodeId> {
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
            ui.separator();
            if ui.button("Add Clip").clicked() {
                self.add_node(BlendNodeType::Clip, Pos2::new(50.0, 50.0));
            }
            if ui.button("Add Blend 1D").clicked() {
                self.add_node(BlendNodeType::Blend1D, Pos2::new(250.0, 50.0));
            }
            if ui.button("Add Blend 2D").clicked() {
                self.add_node(BlendNodeType::Blend2D, Pos2::new(250.0, 150.0));
            }
            if ui.button("Add Additive").clicked() {
                self.add_node(BlendNodeType::Additive, Pos2::new(250.0, 250.0));
            }
            if ui.button("Add Layer").clicked() {
                self.add_node(BlendNodeType::Layer, Pos2::new(250.0, 350.0));
            }
            if ui.button("Add Mask").clicked() {
                self.add_node(BlendNodeType::Mask, Pos2::new(250.0, 450.0));
            }
            if ui.button("Add Output").clicked() {
                self.add_node(BlendNodeType::Output, Pos2::new(500.0, 100.0));
            }
            ui.separator();
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

        ui.horizontal(|ui| {
            ui.label("Preview Params:");
            for param in &self.tree.parameters {
                let value = self
                    .preview_params
                    .entry(param.name.clone())
                    .or_insert(param.default_value);
                ui.add(
                    egui::DragValue::new(value)
                        .speed(0.01)
                        .clamp_range(param.min_value..=param.max_value)
                        .suffix(&format!(" {}", param.name)),
                );
            }
            if let Some(clip) = self.tree.evaluate(&self.preview_params) {
                ui.label(format!("→ {}", clip));
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
                } else if let Some(node) =
                    self.selected_node.and_then(|id| self.tree.nodes.get(&id))
                {
                    let output_pos = node.output_slot_position();
                    if world_pos.distance(output_pos) < 15.0 && node.node_type.output_count() > 0 {
                        self.connecting_from = Some(node.id);
                    }
                }
            }
        }

        if response.dragged() {
            if let Some(id) = self.dragging_node {
                if let Some(hover_pos) = response.hover_pos() {
                    let world_pos = self.screen_to_world(hover_pos);
                    if let Some(node) = self.tree.nodes.get_mut(&id) {
                        node.set_position(Pos2::new(
                            (world_pos.x / self.grid_size).round() * self.grid_size,
                            (world_pos.y / self.grid_size).round() * self.grid_size,
                        ));
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
                            if let Some(to_node) = self.tree.nodes.get(&to) {
                                for i in 0..to_node.node_type.input_count() {
                                    let input_pos = to_node.input_slot_position(i);
                                    if world_pos.distance(input_pos) < 15.0 {
                                        self.save_state();
                                        self.tree.connect(from, to, i);
                                        break;
                                    }
                                }
                            }
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
        for conn in &self.tree.connections {
            let from_node = match self.tree.nodes.get(&conn.from_node) {
                Some(n) => n,
                None => continue,
            };
            let to_node = match self.tree.nodes.get(&conn.to_node) {
                Some(n) => n,
                None => continue,
            };

            let from_pos = self.world_to_screen(from_node.output_slot_position());
            let to_pos = self.world_to_screen(to_node.input_slot_position(conn.to_input));

            let ctrl1 = Pos2::new(from_pos.x + 50.0, from_pos.y);
            let ctrl2 = Pos2::new(to_pos.x - 50.0, to_pos.y);

            let color = Color32::from_rgb(150, 200, 150);
            let stroke = Stroke::new(3.0, color);

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

    fn draw_nodes(&self, painter: &egui::Painter, _ui: &mut Ui) {
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

            let text_pos = Pos2::new(rect.center().x, rect.min.y + 15.0);
            painter.text(
                text_pos,
                egui::Align2::CENTER_CENTER,
                &node.name,
                egui::FontId::proportional(11.0),
                Color32::WHITE,
            );

            if let Some(clip) = &node.clip_name {
                painter.text(
                    Pos2::new(rect.min.x + 5.0, rect.min.y + 30.0),
                    egui::Align2::LEFT_TOP,
                    clip,
                    egui::FontId::proportional(10.0),
                    Color32::LIGHT_GRAY,
                );
            }

            for i in 0..node.node_type.input_count() {
                let slot_pos = self.world_to_screen(node.input_slot_position(i));
                painter.circle_filled(slot_pos, 5.0, Color32::from_rgb(100, 200, 100));
                painter.circle_stroke(slot_pos, 5.0, Stroke::new(2.0, Color32::from_gray(50)));
            }

            if node.node_type.output_count() > 0 {
                let output_pos = self.world_to_screen(node.output_slot_position());
                painter.circle_filled(output_pos, 5.0, Color32::from_rgb(200, 100, 100));
                painter.circle_stroke(output_pos, 5.0, Stroke::new(2.0, Color32::from_gray(50)));
            }
        }
    }
}

impl Default for AnimBlendTreeEditor {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blend_node_creation() {
        let node = BlendTreeNode::new(BlendNodeId(1), BlendNodeType::Clip, Pos2::new(100.0, 50.0));
        assert_eq!(node.id, BlendNodeId(1));
        assert_eq!(node.node_type, BlendNodeType::Clip);
    }

    #[test]
    fn blend_tree_creation() {
        let tree = BlendTreeDef::new("Test Tree");
        assert_eq!(tree.name, "Test Tree");
        assert!(tree.nodes.is_empty());
    }

    #[test]
    fn blend_tree_add_node() {
        let mut tree = BlendTreeDef::new("Test");
        let node = BlendTreeNode::new(BlendNodeId(1), BlendNodeType::Output, Pos2::new(0.0, 0.0));
        tree.add_node(node);

        assert_eq!(tree.nodes.len(), 1);
        assert_eq!(tree.output_node, Some(BlendNodeId(1)));
    }

    #[test]
    fn blend_tree_connect() {
        let mut tree = BlendTreeDef::new("Test");
        let clip = BlendTreeNode::new(BlendNodeId(1), BlendNodeType::Clip, Pos2::new(0.0, 0.0));
        let output =
            BlendTreeNode::new(BlendNodeId(2), BlendNodeType::Output, Pos2::new(200.0, 0.0));
        tree.add_node(clip);
        tree.add_node(output);

        assert!(tree.connect(BlendNodeId(1), BlendNodeId(2), 0));
        assert_eq!(tree.connections.len(), 1);
    }

    #[test]
    fn blend_editor_undo_redo() {
        let mut editor = AnimBlendTreeEditor::new();
        editor.add_node(BlendNodeType::Clip, Pos2::new(0.0, 0.0));
        assert_eq!(editor.tree().nodes.len(), 1);

        editor.undo();
        assert_eq!(editor.tree().nodes.len(), 0);

        editor.redo();
        assert_eq!(editor.tree().nodes.len(), 1);
    }

    #[test]
    fn blend_parameter_creation() {
        let param = BlendParameter::new("speed", 0.0, 2.0, 1.0);
        assert_eq!(param.name, "speed");
        assert_eq!(param.min_value, 0.0);
        assert_eq!(param.max_value, 2.0);
        assert_eq!(param.default_value, 1.0);
    }
}
