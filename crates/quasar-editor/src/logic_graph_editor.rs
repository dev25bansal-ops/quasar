//! Logic graph editor panel — visual node graph UI for game logic.
//!
//! Similar to the shader graph editor but operates on [`LogicGraph`] and
//! compiles to Lua instead of WGSL.

use egui::{Color32, Pos2, Rect, Stroke, Vec2, StrokeKind};

use crate::logic_graph::{
    ConnectionKind, LogicGraph, LogicGraphCompiler, LogicNode, LogicNodeKind,
};

const NODE_WIDTH: f32 = 170.0;
const NODE_HEADER_H: f32 = 24.0;
const SLOT_HEIGHT: f32 = 18.0;
const SLOT_RADIUS: f32 = 5.0;
const GRID_SPACING: f32 = 20.0;

/// Persistent state for the logic graph editor panel.
pub struct LogicGraphEditorState {
    pub scroll: Vec2,
    pub zoom: f32,
    dragging_node: Option<usize>,
    linking_from: Option<(ConnectionKind, u32, u32)>,
    context_menu_pos: Option<Pos2>,
    /// The last compiled Lua output for preview.
    pub compiled_lua: String,
}

impl Default for LogicGraphEditorState {
    fn default() -> Self {
        Self {
            scroll: Vec2::ZERO,
            zoom: 1.0,
            dragging_node: None,
            linking_from: None,
            context_menu_pos: None,
            compiled_lua: String::new(),
        }
    }
}

impl LogicGraphEditorState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Render the logic graph editor as a window. Returns true if modified.
    pub fn panel(&mut self, ctx: &egui::Context, graph: &mut LogicGraph) -> bool {
        let mut changed = false;

        egui::Window::new("📜 Logic Graph")
            .default_size([850.0, 550.0])
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("▶ Compile to Lua").clicked() {
                        match LogicGraphCompiler::compile(graph) {
                            Ok(lua) => self.compiled_lua = lua,
                            Err(e) => self.compiled_lua = format!("-- ERROR: {}", e),
                        }
                    }
                    ui.label(format!("{} nodes, {} connections", graph.nodes.len(), graph.connections.len()));
                });
                ui.separator();

                // Split: left = canvas, right = preview
                let available = ui.available_size();
                let canvas_width = (available.x * 0.65).max(200.0);

                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(canvas_width);
                        changed = self.canvas(ui, graph);
                    });
                    ui.separator();
                    ui.vertical(|ui| {
                        ui.label("Compiled Lua:");
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut self.compiled_lua.as_str())
                                    .font(egui::TextStyle::Monospace)
                                    .desired_width(f32::INFINITY),
                            );
                        });
                    });
                });
            });

        changed
    }

    fn canvas(&mut self, ui: &mut egui::Ui, graph: &mut LogicGraph) -> bool {
        let mut changed = false;
        let (response, painter) =
            ui.allocate_painter(ui.available_size_before_wrap(), egui::Sense::click_and_drag());
        let canvas_rect = response.rect;
        let origin = canvas_rect.min.to_vec2() + self.scroll;

        // Background grid
        self.draw_grid(&painter, canvas_rect);

        // Draw connections
        for conn in &graph.connections {
            if let (Some(from_node), Some(to_node)) = (
                graph.nodes.iter().find(|n| n.id == conn.from_node),
                graph.nodes.iter().find(|n| n.id == conn.to_node),
            ) {
                let (from_pos, to_pos) = match conn.kind {
                    ConnectionKind::Exec => (
                        self.exec_output_pos(from_node, conn.from_slot, origin),
                        self.exec_input_pos(to_node, origin),
                    ),
                    ConnectionKind::Data => (
                        self.data_output_pos(from_node, conn.from_slot, origin),
                        self.data_input_pos(to_node, conn.to_slot, origin),
                    ),
                };
                let color = match conn.kind {
                    ConnectionKind::Exec => Color32::from_rgb(220, 220, 220),
                    ConnectionKind::Data => Color32::from_rgb(120, 200, 255),
                };
                self.draw_bezier(&painter, from_pos, to_pos, color);
            }
        }

        // Draw nodes
        for node in &graph.nodes {
            let node_rect = self.node_rect(node, origin);
            if !canvas_rect.intersects(node_rect) {
                continue;
            }

            painter.rect_filled(node_rect, 6.0, Color32::from_rgb(45, 45, 48));
            painter.rect(
                node_rect, 6.0, Color32::TRANSPARENT,
                Stroke::new(1.0, Color32::from_rgb(80, 80, 85)), StrokeKind::Outside,
            );

            let header_rect = Rect::from_min_size(
                node_rect.min,
                Vec2::new(NODE_WIDTH * self.zoom, NODE_HEADER_H * self.zoom),
            );
            painter.rect_filled(header_rect, 6.0, self.node_color(&node.kind));
            painter.text(
                header_rect.center(), egui::Align2::CENTER_CENTER, &node.label,
                egui::FontId::proportional(11.0 * self.zoom), Color32::WHITE,
            );

            // Exec input (white triangle)
            if node.exec_input_count() > 0 {
                let pos = self.exec_input_pos(node, origin);
                painter.circle_filled(pos, SLOT_RADIUS * self.zoom, Color32::WHITE);
            }
            // Exec outputs (white triangles)
            for i in 0..node.exec_output_count() {
                let pos = self.exec_output_pos(node, i, origin);
                painter.circle_filled(pos, SLOT_RADIUS * self.zoom, Color32::WHITE);
            }
            // Data inputs (green circles)
            for i in 0..node.data_input_count() {
                let pos = self.data_input_pos(node, i, origin);
                painter.circle_filled(pos, SLOT_RADIUS * self.zoom, Color32::from_rgb(100, 200, 100));
            }
            // Data outputs (blue circles)
            for i in 0..node.data_output_count() {
                let pos = self.data_output_pos(node, i, origin);
                painter.circle_filled(pos, SLOT_RADIUS * self.zoom, Color32::from_rgb(100, 150, 255));
            }
        }

        // Handle dragging
        if response.dragged_by(egui::PointerButton::Primary) {
            let delta = response.drag_delta();
            if let Some(idx) = self.dragging_node {
                if idx < graph.nodes.len() {
                    graph.nodes[idx].editor_pos[0] += delta.x / self.zoom;
                    graph.nodes[idx].editor_pos[1] += delta.y / self.zoom;
                    changed = true;
                }
            } else if let Some(pointer) = ui.ctx().pointer_latest_pos() {
                let mut found = false;
                for (i, node) in graph.nodes.iter().enumerate() {
                    let rect = self.node_rect(node, origin);
                    if rect.contains(pointer) {
                        self.dragging_node = Some(i);
                        found = true;
                        break;
                    }
                }
                if !found {
                    self.scroll += delta;
                }
            }
        }

        if response.drag_stopped() {
            self.dragging_node = None;
            self.linking_from = None;
        }

        // Right-click: add node menu
        if response.secondary_clicked() {
            self.context_menu_pos = ui.ctx().pointer_latest_pos();
        }

        if let Some(menu_pos) = self.context_menu_pos {
            let mut close_menu = false;
            egui::Area::new(egui::Id::new("logic_graph_ctx"))
                .fixed_pos(menu_pos)
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.label("Add Logic Node");
                        ui.separator();
                        let kinds: &[(&str, LogicNodeKind)] = &[
                            ("On Start", LogicNodeKind::OnStart),
                            ("On Update", LogicNodeKind::OnUpdate),
                            ("Branch", LogicNodeKind::Branch),
                            ("For Each", LogicNodeKind::ForEach { component: "Transform".into() }),
                            ("Sequence (3)", LogicNodeKind::Sequence { count: 3 }),
                            ("Self Entity", LogicNodeKind::SelfEntity),
                            ("Spawn Entity", LogicNodeKind::SpawnEntity),
                            ("Despawn Entity", LogicNodeKind::DespawnEntity),
                            ("Set Position", LogicNodeKind::SetPosition),
                            ("Apply Force", LogicNodeKind::ApplyForce),
                            ("Print", LogicNodeKind::Print),
                            ("Play Audio", LogicNodeKind::PlayAudio),
                            ("Float (0)", LogicNodeKind::FloatLiteral(0.0)),
                            ("String", LogicNodeKind::StringLiteral(String::new())),
                            ("Bool (true)", LogicNodeKind::BoolLiteral(true)),
                            ("Vec3", LogicNodeKind::Vec3Construct),
                            ("Add", LogicNodeKind::Add),
                            ("Subtract", LogicNodeKind::Subtract),
                            ("Multiply", LogicNodeKind::Multiply),
                            ("Divide", LogicNodeKind::Divide),
                            (">", LogicNodeKind::GreaterThan),
                            ("<", LogicNodeKind::LessThan),
                            ("==", LogicNodeKind::Equals),
                            ("AND", LogicNodeKind::And),
                            ("OR", LogicNodeKind::Or),
                            ("NOT", LogicNodeKind::Not),
                        ];
                        for (name, kind) in kinds {
                            if ui.button(*name).clicked() {
                                let world_pos = [
                                    (menu_pos.x - origin.x) / self.zoom,
                                    (menu_pos.y - origin.y) / self.zoom,
                                ];
                                let id = graph.add_node(kind.clone());
                                if let Some(node) = graph.nodes.iter_mut().find(|n| n.id == id) {
                                    node.editor_pos = world_pos;
                                }
                                changed = true;
                                close_menu = true;
                            }
                        }
                    });
                });
            if close_menu || response.clicked() {
                self.context_menu_pos = None;
            }
        }

        // Zoom
        let scroll_delta = ui.input(|i| i.smooth_scroll_delta.y);
        if canvas_rect.contains(ui.ctx().pointer_latest_pos().unwrap_or_default()) && scroll_delta.abs() > 0.1 {
            let factor = 1.0 + scroll_delta * 0.002;
            self.zoom = (self.zoom * factor).clamp(0.25, 3.0);
        }

        changed
    }

    // ── Position helpers ───────────────────────────────────────────

    fn node_height(&self, node: &LogicNode) -> f32 {
        let total_slots = (node.exec_input_count() + node.exec_output_count())
            .max(node.data_input_count() + node.data_output_count())
            .max(1);
        (NODE_HEADER_H + total_slots as f32 * SLOT_HEIGHT + 8.0) * self.zoom
    }

    fn node_rect(&self, node: &LogicNode, origin: Vec2) -> Rect {
        let min = Pos2::new(
            node.editor_pos[0] * self.zoom + origin.x,
            node.editor_pos[1] * self.zoom + origin.y,
        );
        Rect::from_min_size(min, Vec2::new(NODE_WIDTH * self.zoom, self.node_height(node)))
    }

    fn exec_input_pos(&self, node: &LogicNode, origin: Vec2) -> Pos2 {
        Pos2::new(
            node.editor_pos[0] * self.zoom + origin.x,
            node.editor_pos[1] * self.zoom + origin.y + NODE_HEADER_H * 0.5 * self.zoom,
        )
    }

    fn exec_output_pos(&self, node: &LogicNode, slot: u32, origin: Vec2) -> Pos2 {
        Pos2::new(
            node.editor_pos[0] * self.zoom + origin.x + NODE_WIDTH * self.zoom,
            node.editor_pos[1] * self.zoom + origin.y + (NODE_HEADER_H * 0.5 + slot as f32 * SLOT_HEIGHT) * self.zoom,
        )
    }

    fn data_input_pos(&self, node: &LogicNode, slot: u32, origin: Vec2) -> Pos2 {
        let y_offset = NODE_HEADER_H + node.exec_input_count() as f32 * SLOT_HEIGHT + slot as f32 * SLOT_HEIGHT + SLOT_HEIGHT * 0.5;
        Pos2::new(
            node.editor_pos[0] * self.zoom + origin.x,
            node.editor_pos[1] * self.zoom + origin.y + y_offset * self.zoom,
        )
    }

    fn data_output_pos(&self, node: &LogicNode, slot: u32, origin: Vec2) -> Pos2 {
        let y_offset = NODE_HEADER_H + node.exec_output_count() as f32 * SLOT_HEIGHT + slot as f32 * SLOT_HEIGHT + SLOT_HEIGHT * 0.5;
        Pos2::new(
            node.editor_pos[0] * self.zoom + origin.x + NODE_WIDTH * self.zoom,
            node.editor_pos[1] * self.zoom + origin.y + y_offset * self.zoom,
        )
    }

    fn draw_grid(&self, painter: &egui::Painter, rect: Rect) {
        let step = GRID_SPACING * self.zoom;
        let offset_x = self.scroll.x % step;
        let offset_y = self.scroll.y % step;
        let color = Color32::from_rgb(35, 35, 38);
        let mut x = rect.min.x + offset_x;
        while x < rect.max.x {
            painter.line_segment([Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)], Stroke::new(1.0, color));
            x += step;
        }
        let mut y = rect.min.y + offset_y;
        while y < rect.max.y {
            painter.line_segment([Pos2::new(rect.min.x, y), Pos2::new(rect.max.x, y)], Stroke::new(1.0, color));
            y += step;
        }
    }

    fn draw_bezier(&self, painter: &egui::Painter, from: Pos2, to: Pos2, color: Color32) {
        let dx = (to.x - from.x).abs() * 0.5;
        let cp1 = Pos2::new(from.x + dx, from.y);
        let cp2 = Pos2::new(to.x - dx, to.y);
        let points: Vec<Pos2> = (0..=32)
            .map(|i| {
                let t = i as f32 / 32.0;
                let inv = 1.0 - t;
                Pos2::new(
                    inv.powi(3) * from.x + 3.0 * inv.powi(2) * t * cp1.x + 3.0 * inv * t.powi(2) * cp2.x + t.powi(3) * to.x,
                    inv.powi(3) * from.y + 3.0 * inv.powi(2) * t * cp1.y + 3.0 * inv * t.powi(2) * cp2.y + t.powi(3) * to.y,
                )
            })
            .collect();
        for pair in points.windows(2) {
            painter.line_segment([pair[0], pair[1]], Stroke::new(2.0 * self.zoom, color));
        }
    }

    fn node_color(&self, kind: &LogicNodeKind) -> Color32 {
        match kind {
            LogicNodeKind::OnUpdate | LogicNodeKind::OnStart | LogicNodeKind::OnEvent { .. }
            | LogicNodeKind::OnKeyPressed { .. } => Color32::from_rgb(180, 60, 60),
            LogicNodeKind::Branch | LogicNodeKind::ForEach { .. } | LogicNodeKind::Sequence { .. } => Color32::from_rgb(180, 140, 60),
            LogicNodeKind::GetComponent { .. } | LogicNodeKind::SetComponent { .. }
            | LogicNodeKind::SpawnEntity | LogicNodeKind::DespawnEntity | LogicNodeKind::SelfEntity => Color32::from_rgb(60, 130, 180),
            LogicNodeKind::Add | LogicNodeKind::Subtract | LogicNodeKind::Multiply | LogicNodeKind::Divide
            | LogicNodeKind::GreaterThan | LogicNodeKind::LessThan | LogicNodeKind::Equals
            | LogicNodeKind::And | LogicNodeKind::Or | LogicNodeKind::Not
            | LogicNodeKind::Vec3Construct => Color32::from_rgb(140, 100, 180),
            LogicNodeKind::FloatLiteral(_) | LogicNodeKind::StringLiteral(_) | LogicNodeKind::BoolLiteral(_) => Color32::from_rgb(60, 160, 80),
            LogicNodeKind::GetVariable { .. } | LogicNodeKind::SetVariable { .. } => Color32::from_rgb(60, 160, 130),
            _ => Color32::from_rgb(100, 100, 110),
        }
    }
}
