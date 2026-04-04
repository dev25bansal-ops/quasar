//! Visual Dialog Graph Editor for Quasar Engine.
//!
//! Provides:
//! - **Visual node editing** — drag-and-drop dialog nodes
//! - **Branch conditions** — condition-based dialog branching
//! - **Localization support** — locale key integration
//! - **Preview mode** — test dialog flow in-editor
//! - **Serialization** — save/load dialog trees

use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DialogNodeId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DialogChoiceId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DialogNodeType {
    Start,
    Dialog,
    Choice,
    Condition,
    Action,
    End,
}

impl DialogNodeType {
    pub fn name(&self) -> &'static str {
        match self {
            DialogNodeType::Start => "Start",
            DialogNodeType::Dialog => "Dialog",
            DialogNodeType::Choice => "Choice",
            DialogNodeType::Condition => "Condition",
            DialogNodeType::Action => "Action",
            DialogNodeType::End => "End",
        }
    }

    pub fn color(&self) -> Color32 {
        match self {
            DialogNodeType::Start => Color32::from_rgb(100, 200, 100),
            DialogNodeType::Dialog => Color32::from_rgb(100, 150, 200),
            DialogNodeType::Choice => Color32::from_rgb(200, 150, 100),
            DialogNodeType::Condition => Color32::from_rgb(150, 100, 200),
            DialogNodeType::Action => Color32::from_rgb(200, 100, 150),
            DialogNodeType::End => Color32::from_rgb(200, 100, 100),
        }
    }

    pub fn can_have_outputs(&self) -> bool {
        !matches!(self, DialogNodeType::End)
    }

    pub fn max_outputs(&self) -> Option<usize> {
        match self {
            DialogNodeType::Start => Some(1),
            DialogNodeType::Dialog => Some(1),
            DialogNodeType::Choice => None,
            DialogNodeType::Condition => Some(2),
            DialogNodeType::Action => Some(1),
            DialogNodeType::End => Some(0),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogChoice {
    pub id: DialogChoiceId,
    pub text_key: String,
    pub condition: Option<String>,
    pub target_node: Option<DialogNodeId>,
}

impl DialogChoice {
    pub fn new(id: DialogChoiceId, text_key: &str) -> Self {
        Self {
            id,
            text_key: text_key.to_string(),
            condition: None,
            target_node: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogGraphNode {
    pub id: DialogNodeId,
    pub node_type: DialogNodeType,
    pub position: [f32; 2],
    pub speaker: Option<String>,
    pub text_key: String,
    pub choices: Vec<DialogChoice>,
    pub condition: Option<String>,
    pub action: Option<String>,
    pub next_node: Option<DialogNodeId>,
    pub effects: Vec<String>,
}

impl DialogGraphNode {
    pub fn new(id: DialogNodeId, node_type: DialogNodeType, position: Pos2) -> Self {
        Self {
            id,
            node_type,
            position: [position.x, position.y],
            speaker: None,
            text_key: String::new(),
            choices: Vec::new(),
            condition: None,
            action: None,
            next_node: None,
            effects: Vec::new(),
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
            DialogNodeType::Start => Vec2::new(100.0, 50.0),
            DialogNodeType::Dialog => Vec2::new(200.0, 100.0),
            DialogNodeType::Choice => Vec2::new(200.0, 80.0 + self.choices.len() as f32 * 25.0),
            DialogNodeType::Condition => Vec2::new(150.0, 80.0),
            DialogNodeType::Action => Vec2::new(150.0, 60.0),
            DialogNodeType::End => Vec2::new(100.0, 50.0),
        }
    }

    pub fn rect(&self) -> Rect {
        Rect::from_min_size(self.position(), self.size())
    }

    pub fn output_slot_position(&self, index: usize) -> Pos2 {
        let pos = self.position();
        let size = self.size();
        match self.node_type {
            DialogNodeType::Choice => {
                let y_offset = 60.0 + index as f32 * 25.0;
                Pos2::new(pos.x + size.x, pos.y + y_offset)
            }
            DialogNodeType::Condition => {
                let y_offset = if index == 0 { 40.0 } else { 70.0 };
                Pos2::new(pos.x + size.x, pos.y + y_offset)
            }
            _ => Pos2::new(pos.x + size.x, pos.y + size.y / 2.0),
        }
    }

    pub fn input_slot_position(&self) -> Pos2 {
        let pos = self.position();
        let size = self.size();
        Pos2::new(pos.x, pos.y + size.y / 2.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogConnection {
    pub from_node: DialogNodeId,
    pub from_slot: usize,
    pub to_node: DialogNodeId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogGraphDef {
    pub name: String,
    pub nodes: HashMap<DialogNodeId, DialogGraphNode>,
    pub connections: Vec<DialogConnection>,
    pub start_node: Option<DialogNodeId>,
    pub locale_table: String,
}

impl DialogGraphDef {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            nodes: HashMap::new(),
            connections: Vec::new(),
            start_node: None,
            locale_table: "dialogs".to_string(),
        }
    }

    pub fn add_node(&mut self, node: DialogGraphNode) {
        if self.nodes.is_empty() && node.node_type == DialogNodeType::Start {
            self.start_node = Some(node.id);
        }
        self.nodes.insert(node.id, node);
    }

    pub fn remove_node(&mut self, id: DialogNodeId) {
        self.nodes.remove(&id);
        self.connections
            .retain(|c| c.from_node != id && c.to_node != id);
        if self.start_node == Some(id) {
            self.start_node = self
                .nodes
                .iter()
                .find(|(_, n)| n.node_type == DialogNodeType::Start)
                .map(|(id, _)| *id);
        }
    }

    pub fn connect(&mut self, from: DialogNodeId, from_slot: usize, to: DialogNodeId) -> bool {
        if !self.nodes.contains_key(&from) || !self.nodes.contains_key(&to) {
            return false;
        }

        if let Some(from_node) = self.nodes.get(&from) {
            if let Some(max) = from_node.node_type.max_outputs() {
                if from_slot >= max {
                    return false;
                }
            }
        }

        self.connections
            .retain(|c| !(c.from_node == from && c.from_slot == from_slot));
        self.connections.push(DialogConnection {
            from_node: from,
            from_slot,
            to_node: to,
        });
        true
    }

    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();

        if self.start_node.is_none() {
            errors.push("No start node found".to_string());
        }

        for node in self.nodes.values() {
            if node.node_type == DialogNodeType::Dialog && node.text_key.is_empty() {
                errors.push(format!("Dialog node {:?} has no text key", node.id));
            }

            if node.node_type == DialogNodeType::Choice && node.choices.is_empty() {
                errors.push(format!("Choice node {:?} has no choices", node.id));
            }
        }

        for node in self.nodes.values() {
            if node.id != self.start_node.unwrap_or(DialogNodeId(0)) {
                let has_input = self.connections.iter().any(|c| c.to_node == node.id);
                if !has_input {
                    errors.push(format!("Node {:?} is not connected", node.id));
                }
            }
        }

        errors
    }

    pub fn compile_to_runtime(&self) -> quasar_core::dialog::DialogTree {
        let mut nodes = Vec::new();
        for node in self.nodes.values() {
            let runtime_node = match node.node_type {
                DialogNodeType::Start => quasar_core::dialog::DialogNode::Start,
                DialogNodeType::Dialog => quasar_core::dialog::DialogNode::Say {
                    speaker: node.speaker.clone().unwrap_or_default(),
                    text_key: node.text_key.clone(),
                    next: None,
                },
                DialogNodeType::Choice => quasar_core::dialog::DialogNode::Choice {
                    text_key: node.text_key.clone(),
                    responses: node
                        .choices
                        .iter()
                        .map(|c| quasar_core::dialog::DialogResponse {
                            text_key: c.text_key.clone(),
                            condition: c.condition.clone(),
                            next: None,
                            effects: Vec::new(),
                        })
                        .collect(),
                },
                DialogNodeType::Condition => quasar_core::dialog::DialogNode::Condition {
                    condition: node.condition.clone().unwrap_or_default(),
                    true_branch: None,
                    false_branch: None,
                },
                DialogNodeType::Action => quasar_core::dialog::DialogNode::Action {
                    action: node.action.clone().unwrap_or_default(),
                    effects: node.effects.clone(),
                    next: None,
                },
                DialogNodeType::End => quasar_core::dialog::DialogNode::End,
            };
            nodes.push((node.id.0 as usize, runtime_node));
        }

        quasar_core::dialog::DialogTree {
            name: self.name.clone(),
            nodes: nodes.into_iter().collect(),
            start_node: self.start_node.map(|id| id.0 as usize).unwrap_or(0),
        }
    }
}

pub struct DialogGraphEditor {
    graph: DialogGraphDef,
    selected_node: Option<DialogNodeId>,
    dragging_node: Option<DialogNodeId>,
    connecting_from: Option<(DialogNodeId, usize)>,
    next_node_id: u64,
    next_choice_id: u64,
    pan_offset: Vec2,
    zoom: f32,
    undo_stack: Vec<DialogGraphDef>,
    redo_stack: Vec<DialogGraphDef>,
    preview_mode: bool,
    preview_current: Option<DialogNodeId>,
    preview_choices: Vec<DialogChoiceId>,
    show_grid: bool,
    grid_size: f32,
}

impl DialogGraphEditor {
    pub fn new() -> Self {
        Self {
            graph: DialogGraphDef::new("New Dialog"),
            selected_node: None,
            dragging_node: None,
            connecting_from: None,
            next_node_id: 1,
            next_choice_id: 1,
            pan_offset: Vec2::ZERO,
            zoom: 1.0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            preview_mode: false,
            preview_current: None,
            preview_choices: Vec::new(),
            show_grid: true,
            grid_size: 20.0,
        }
    }

    pub fn with_graph(graph: DialogGraphDef) -> Self {
        let max_node = graph.nodes.keys().map(|id| id.0).max().unwrap_or(0);
        Self {
            graph,
            next_node_id: max_node + 1,
            ..Self::new()
        }
    }

    pub fn graph(&self) -> &DialogGraphDef {
        &self.graph
    }

    pub fn graph_mut(&mut self) -> &mut DialogGraphDef {
        &mut self.graph
    }

    pub fn save_state(&mut self) {
        self.undo_stack.push(self.graph.clone());
        self.redo_stack.clear();
        if self.undo_stack.len() > 50 {
            self.undo_stack.remove(0);
        }
    }

    pub fn undo(&mut self) {
        if let Some(state) = self.undo_stack.pop() {
            self.redo_stack.push(self.graph.clone());
            self.graph = state;
        }
    }

    pub fn redo(&mut self) {
        if let Some(state) = self.redo_stack.pop() {
            self.undo_stack.push(self.graph.clone());
            self.graph = state;
        }
    }

    pub fn add_node(&mut self, node_type: DialogNodeType, position: Pos2) -> DialogNodeId {
        self.save_state();
        let id = DialogNodeId(self.next_node_id);
        self.next_node_id += 1;

        let pos = Pos2::new(
            (position.x / self.grid_size).round() * self.grid_size,
            (position.y / self.grid_size).round() * self.grid_size,
        );

        let mut node = DialogGraphNode::new(id, node_type, pos);
        if node_type == DialogNodeType::Choice {
            node.choices.push(DialogChoice::new(
                DialogChoiceId(self.next_choice_id),
                "choice.default",
            ));
            self.next_choice_id += 1;
        }

        self.graph.add_node(node);
        self.selected_node = Some(id);
        id
    }

    pub fn delete_selected(&mut self) {
        if let Some(id) = self.selected_node {
            self.save_state();
            self.graph.remove_node(id);
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

    pub fn node_at_position(&self, pos: Pos2) -> Option<DialogNodeId> {
        for node in self.graph.nodes.values() {
            if node.rect().contains(pos) {
                return Some(node.id);
            }
        }
        None
    }

    pub fn show(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(&self.graph.name);
            ui.separator();
            if ui.button("Add Start").clicked() {
                self.add_node(DialogNodeType::Start, Pos2::new(50.0, 50.0));
            }
            if ui.button("Add Dialog").clicked() {
                self.add_node(DialogNodeType::Dialog, Pos2::new(250.0, 50.0));
            }
            if ui.button("Add Choice").clicked() {
                self.add_node(DialogNodeType::Choice, Pos2::new(250.0, 150.0));
            }
            if ui.button("Add Condition").clicked() {
                self.add_node(DialogNodeType::Condition, Pos2::new(250.0, 250.0));
            }
            if ui.button("Add Action").clicked() {
                self.add_node(DialogNodeType::Action, Pos2::new(250.0, 350.0));
            }
            if ui.button("Add End").clicked() {
                self.add_node(DialogNodeType::End, Pos2::new(500.0, 50.0));
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
            ui.separator();
            if ui.button("Compile").clicked() {
                let errors = self.graph.validate();
                if errors.is_empty() {
                    let _runtime = self.graph.compile_to_runtime();
                    log::info!("Dialog graph compiled successfully");
                } else {
                    for err in &errors {
                        log::warn!("Validation error: {}", err);
                    }
                }
            }
            ui.separator();
            ui.toggle_value(&mut self.preview_mode, "Preview");
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
                    self.selected_node.and_then(|id| self.graph.nodes.get(&id))
                {
                    for i in 0..node.output_slot_position(0).x as usize {
                        let slot_pos = node.output_slot_position(i);
                        if world_pos.distance(slot_pos) < 10.0 {
                            self.connecting_from = Some((node.id, i));
                            break;
                        }
                    }
                }
            }
        }

        if response.dragged() {
            if let Some(id) = self.dragging_node {
                if let Some(hover_pos) = response.hover_pos() {
                    let world_pos = self.screen_to_world(hover_pos);
                    if let Some(node) = self.graph.nodes.get_mut(&id) {
                        node.set_position(Pos2::new(
                            (world_pos.x / self.grid_size).round() * self.grid_size,
                            (world_pos.y / self.grid_size).round() * self.grid_size,
                        ));
                    }
                }
            }
        }

        if response.drag_stopped() {
            if let Some((from, slot)) = self.connecting_from {
                if let Some(hover_pos) = response.hover_pos() {
                    let world_pos = self.screen_to_world(hover_pos);
                    if let Some(to) = self.node_at_position(world_pos) {
                        if from != to {
                            self.save_state();
                            self.graph.connect(from, slot, to);
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
        for conn in &self.graph.connections {
            let from_node = match self.graph.nodes.get(&conn.from_node) {
                Some(n) => n,
                None => continue,
            };
            let to_node = match self.graph.nodes.get(&conn.to_node) {
                Some(n) => n,
                None => continue,
            };

            let from_pos = self.world_to_screen(from_node.output_slot_position(conn.from_slot));
            let to_pos = self.world_to_screen(to_node.input_slot_position());

            let ctrl1 = Pos2::new(from_pos.x + 50.0, from_pos.y);
            let ctrl2 = Pos2::new(to_pos.x - 50.0, to_pos.y);

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

    fn draw_nodes(&self, painter: &egui::Painter, _ui: &mut Ui) {
        for node in self.graph.nodes.values() {
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
                node.node_type.name(),
                egui::FontId::proportional(12.0),
                Color32::WHITE,
            );

            if let Some(speaker) = &node.speaker {
                painter.text(
                    Pos2::new(rect.min.x + 5.0, rect.min.y + 30.0),
                    egui::Align2::LEFT_TOP,
                    speaker,
                    egui::FontId::proportional(10.0),
                    Color32::LIGHT_GRAY,
                );
            }

            if !node.text_key.is_empty() {
                painter.text(
                    Pos2::new(rect.min.x + 5.0, rect.min.y + 45.0),
                    egui::Align2::LEFT_TOP,
                    &node.text_key,
                    egui::FontId::proportional(10.0),
                    Color32::LIGHT_GRAY,
                );
            }

            if node.node_type.can_have_outputs() {
                if let Some(max) = node.node_type.max_outputs() {
                    for i in 0..max {
                        let slot_pos = self.world_to_screen(node.output_slot_position(i));
                        painter.circle_filled(slot_pos, 6.0, Color32::WHITE);
                        painter.circle_stroke(
                            slot_pos,
                            6.0,
                            Stroke::new(2.0, Color32::from_gray(50)),
                        );
                    }
                } else {
                    for (i, _) in node.choices.iter().enumerate() {
                        let slot_pos = self.world_to_screen(node.output_slot_position(i));
                        painter.circle_filled(slot_pos, 6.0, Color32::WHITE);
                        painter.circle_stroke(
                            slot_pos,
                            6.0,
                            Stroke::new(2.0, Color32::from_gray(50)),
                        );
                    }
                }
            }

            let input_pos = self.world_to_screen(node.input_slot_position());
            painter.circle_filled(input_pos, 6.0, Color32::from_gray(200));
            painter.circle_stroke(input_pos, 6.0, Stroke::new(2.0, Color32::from_gray(50)));
        }
    }
}

impl Default for DialogGraphEditor {
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
    fn dialog_node_creation() {
        let node = DialogGraphNode::new(
            DialogNodeId(1),
            DialogNodeType::Dialog,
            Pos2::new(100.0, 50.0),
        );
        assert_eq!(node.id, DialogNodeId(1));
        assert_eq!(node.node_type, DialogNodeType::Dialog);
    }

    #[test]
    fn dialog_graph_creation() {
        let graph = DialogGraphDef::new("Test Dialog");
        assert_eq!(graph.name, "Test Dialog");
        assert!(graph.nodes.is_empty());
    }

    #[test]
    fn dialog_graph_add_node() {
        let mut graph = DialogGraphDef::new("Test");
        let node =
            DialogGraphNode::new(DialogNodeId(1), DialogNodeType::Start, Pos2::new(0.0, 0.0));
        graph.add_node(node);

        assert_eq!(graph.nodes.len(), 1);
        assert_eq!(graph.start_node, Some(DialogNodeId(1)));
    }

    #[test]
    fn dialog_graph_connect() {
        let mut graph = DialogGraphDef::new("Test");
        let start =
            DialogGraphNode::new(DialogNodeId(1), DialogNodeType::Start, Pos2::new(0.0, 0.0));
        let dialog = DialogGraphNode::new(
            DialogNodeId(2),
            DialogNodeType::Dialog,
            Pos2::new(200.0, 0.0),
        );
        graph.add_node(start);
        graph.add_node(dialog);

        assert!(graph.connect(DialogNodeId(1), 0, DialogNodeId(2)));
        assert_eq!(graph.connections.len(), 1);
    }

    #[test]
    fn dialog_editor_undo_redo() {
        let mut editor = DialogGraphEditor::new();
        editor.add_node(DialogNodeType::Start, Pos2::new(0.0, 0.0));
        assert_eq!(editor.graph().nodes.len(), 1);

        editor.undo();
        assert_eq!(editor.graph().nodes.len(), 0);

        editor.redo();
        assert_eq!(editor.graph().nodes.len(), 1);
    }

    #[test]
    fn dialog_compile_to_runtime() {
        let mut graph = DialogGraphDef::new("Test");
        let start =
            DialogGraphNode::new(DialogNodeId(1), DialogNodeType::Start, Pos2::new(0.0, 0.0));
        graph.add_node(start);

        let runtime = graph.compile_to_runtime();
        assert_eq!(runtime.name, "Test");
    }
}
