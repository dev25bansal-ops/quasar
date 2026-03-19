//! Shader graph editor panel — visual node graph UI using egui.
//!
//! Renders a canvas with draggable nodes, connection lines, and a
//! right-click menu for adding new nodes.  Operates directly on a
//! [`ShaderGraph`] from `quasar_render`.

use egui::{Color32, Pos2, Rect, Stroke, StrokeKind, Vec2};
use quasar_render::{ShaderConnection, ShaderGraph, ShaderNode, ShaderNodeKind};

// ── Constants ──────────────────────────────────────────────────────

const NODE_WIDTH: f32 = 160.0;
const NODE_HEADER_H: f32 = 24.0;
const SLOT_HEIGHT: f32 = 18.0;
const SLOT_RADIUS: f32 = 5.0;
const GRID_SPACING: f32 = 20.0;

// ── Interaction state ──────────────────────────────────────────────

/// Persistent state for the shader graph editor panel.
pub struct ShaderGraphEditor {
    /// Canvas scroll offset.
    pub scroll: Vec2,
    /// Node currently being dragged (index into graph.nodes).
    dragging_node: Option<usize>,
    /// In-progress link: (from_node_id, from_slot).
    linking_from: Option<(u32, u32)>,
    /// Position where the context menu was opened.
    context_menu_pos: Option<Pos2>,
    /// Zoom factor.
    pub zoom: f32,
}

impl Default for ShaderGraphEditor {
    fn default() -> Self {
        Self {
            scroll: Vec2::ZERO,
            dragging_node: None,
            linking_from: None,
            context_menu_pos: None,
            zoom: 1.0,
        }
    }
}

impl ShaderGraphEditor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Draw the shader graph editor panel.
    /// Returns `true` if the graph was modified.
    pub fn ui(&mut self, ctx: &egui::Context, graph: &mut ShaderGraph) -> bool {
        let mut changed = false;

        egui::CentralPanel::default().show(ctx, |ui| {
            changed = self.canvas(ui, graph);
        });

        changed
    }

    /// Draw the shader graph editor inside a pre-existing panel/window.
    pub fn panel(&mut self, ctx: &egui::Context, graph: &mut ShaderGraph) -> bool {
        let mut changed = false;

        egui::Window::new("🔗 Shader Graph")
            .default_size([800.0, 500.0])
            .resizable(true)
            .show(ctx, |ui| {
                changed = self.canvas(ui, graph);
            });

        changed
    }

    // ── Canvas ─────────────────────────────────────────────────────

    fn canvas(&mut self, ui: &mut egui::Ui, graph: &mut ShaderGraph) -> bool {
        let mut changed = false;
        let (response, painter) = ui.allocate_painter(
            ui.available_size_before_wrap(),
            egui::Sense::click_and_drag(),
        );
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
                let from_pos = self.output_slot_pos(from_node, conn.from_slot, origin);
                let to_pos = self.input_slot_pos(to_node, conn.to_slot, origin);
                self.draw_bezier_connection(
                    &painter,
                    from_pos,
                    to_pos,
                    Color32::from_rgb(120, 200, 255),
                );
            }
        }

        // In-progress link
        if let Some((from_id, from_slot)) = self.linking_from {
            if let Some(from_node) = graph.nodes.iter().find(|n| n.id == from_id) {
                let from_pos = self.output_slot_pos(from_node, from_slot, origin);
                if let Some(pointer) = ui.ctx().pointer_latest_pos() {
                    self.draw_bezier_connection(
                        &painter,
                        from_pos,
                        pointer,
                        Color32::from_rgb(255, 200, 50),
                    );
                }
            }
        }

        // Draw nodes
        let mut hovered_input: Option<(u32, u32)> = None;
        for node in &graph.nodes {
            let node_rect = self.node_rect(node, origin);
            if !canvas_rect.intersects(node_rect) {
                continue;
            }

            // Node background
            painter.rect_filled(node_rect, 6.0, Color32::from_rgb(45, 45, 48));
            painter.rect(
                node_rect,
                6.0,
                Color32::TRANSPARENT,
                Stroke::new(1.0, Color32::from_rgb(80, 80, 85)),
                StrokeKind::Outside,
            );

            // Header
            let header_rect = Rect::from_min_size(
                node_rect.min,
                Vec2::new(NODE_WIDTH * self.zoom, NODE_HEADER_H * self.zoom),
            );
            let header_color = self.node_header_color(&node.kind);
            painter.rect_filled(header_rect, 6.0, header_color);
            painter.text(
                header_rect.center(),
                egui::Align2::CENTER_CENTER,
                &node.label,
                egui::FontId::proportional(12.0 * self.zoom),
                Color32::WHITE,
            );

            // Input slots
            for i in 0..node.input_count() {
                let pos = self.input_slot_pos(node, i, origin);
                painter.circle_filled(
                    pos,
                    SLOT_RADIUS * self.zoom,
                    Color32::from_rgb(100, 200, 100),
                );
                if let Some(ptr) = ui.ctx().pointer_latest_pos() {
                    if pos.distance(ptr) < SLOT_RADIUS * self.zoom * 2.0 {
                        hovered_input = Some((node.id, i));
                    }
                }
            }

            // Output slots
            for i in 0..node.output_count() {
                let pos = self.output_slot_pos(node, i, origin);
                painter.circle_filled(
                    pos,
                    SLOT_RADIUS * self.zoom,
                    Color32::from_rgb(200, 100, 100),
                );
            }
        }

        // Handle node dragging
        if response.dragged_by(egui::PointerButton::Primary) {
            let delta = response.drag_delta();
            if let Some(idx) = self.dragging_node {
                if idx < graph.nodes.len() {
                    graph.nodes[idx].editor_pos[0] += delta.x / self.zoom;
                    graph.nodes[idx].editor_pos[1] += delta.y / self.zoom;
                    changed = true;
                }
            } else if let Some(pointer) = ui.ctx().pointer_latest_pos() {
                // Check if we're starting a drag on a node or an output slot
                let mut found = false;
                for (i, node) in graph.nodes.iter().enumerate() {
                    // Check output slots first for link creation
                    for s in 0..node.output_count() {
                        let pos = self.output_slot_pos(node, s, origin);
                        if pos.distance(pointer) < SLOT_RADIUS * self.zoom * 2.0 {
                            self.linking_from = Some((node.id, s));
                            found = true;
                            break;
                        }
                    }
                    if found {
                        break;
                    }

                    let node_rect = self.node_rect(node, origin);
                    if node_rect.contains(pointer) {
                        self.dragging_node = Some(i);
                        found = true;
                        break;
                    }
                }
                if !found {
                    // Pan canvas
                    self.scroll += delta;
                }
            }
        }

        if response.drag_stopped() {
            // Complete link if hovering an input slot
            if let (Some((from_id, from_slot)), Some((to_id, to_slot))) =
                (self.linking_from, hovered_input)
            {
                if from_id != to_id {
                    graph.connections.push(ShaderConnection {
                        from_node: from_id,
                        from_slot,
                        to_node: to_id,
                        to_slot,
                    });
                    changed = true;
                }
            }
            self.dragging_node = None;
            self.linking_from = None;
        }

        // Right-click context menu for adding nodes
        if response.secondary_clicked() {
            self.context_menu_pos = ui.ctx().pointer_latest_pos();
        }

        if let Some(menu_pos) = self.context_menu_pos {
            let mut close_menu = false;
            egui::Area::new(egui::Id::new("shader_graph_ctx"))
                .fixed_pos(menu_pos)
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.label("Add Node");
                        ui.separator();
                        let kinds: &[(&str, ShaderNodeKind)] = &[
                            (
                                "Texture Sample",
                                ShaderNodeKind::TextureSample { binding_slot: 0 },
                            ),
                            ("TexCoord", ShaderNodeKind::TexCoord { set: 0 }),
                            ("World Position", ShaderNodeKind::WorldPosition),
                            ("World Normal", ShaderNodeKind::WorldNormal),
                            ("View Direction", ShaderNodeKind::ViewDirection),
                            ("Time", ShaderNodeKind::Time),
                            ("Float", ShaderNodeKind::ConstFloat(0.0)),
                            ("Vec3", ShaderNodeKind::ConstVec3([0.0; 3])),
                            ("Color", ShaderNodeKind::ConstColor([1.0; 4])),
                            ("Add", ShaderNodeKind::Add),
                            ("Subtract", ShaderNodeKind::Subtract),
                            ("Multiply", ShaderNodeKind::Multiply),
                            ("Lerp", ShaderNodeKind::Lerp),
                            ("Fresnel", ShaderNodeKind::Fresnel),
                            ("Normalize", ShaderNodeKind::Normalize),
                            ("Dot", ShaderNodeKind::Dot),
                            ("PBR Output", ShaderNodeKind::PbrOutput),
                        ];
                        for (name, kind) in kinds {
                            if ui.button(*name).clicked() {
                                let new_id =
                                    graph.nodes.iter().map(|n| n.id).max().unwrap_or(0) + 1;
                                let world_pos = [
                                    (menu_pos.x - origin.x) / self.zoom,
                                    (menu_pos.y - origin.y) / self.zoom,
                                ];
                                let mut node = ShaderNode::new(new_id, kind.clone());
                                node.label = name.to_string();
                                node.editor_pos = world_pos;
                                graph.nodes.push(node);
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

        // Zoom with scroll
        let scroll_delta = ui.input(|i| i.smooth_scroll_delta.y);
        if canvas_rect.contains(ui.ctx().pointer_latest_pos().unwrap_or_default())
            && scroll_delta.abs() > 0.1
        {
            let factor = 1.0 + scroll_delta * 0.002;
            self.zoom = (self.zoom * factor).clamp(0.25, 3.0);
        }

        changed
    }

    // ── Helpers ────────────────────────────────────────────────────

    fn node_height(&self, node: &ShaderNode) -> f32 {
        let slots = node.input_count().max(node.output_count()).max(1);
        (NODE_HEADER_H + slots as f32 * SLOT_HEIGHT + 6.0) * self.zoom
    }

    fn node_rect(&self, node: &ShaderNode, origin: Vec2) -> Rect {
        let min = Pos2::new(
            node.editor_pos[0] * self.zoom + origin.x,
            node.editor_pos[1] * self.zoom + origin.y,
        );
        Rect::from_min_size(
            min,
            Vec2::new(NODE_WIDTH * self.zoom, self.node_height(node)),
        )
    }

    fn input_slot_pos(&self, node: &ShaderNode, slot: u32, origin: Vec2) -> Pos2 {
        Pos2::new(
            node.editor_pos[0] * self.zoom + origin.x,
            node.editor_pos[1] * self.zoom
                + origin.y
                + (NODE_HEADER_H + slot as f32 * SLOT_HEIGHT + SLOT_HEIGHT * 0.5) * self.zoom,
        )
    }

    fn output_slot_pos(&self, node: &ShaderNode, slot: u32, origin: Vec2) -> Pos2 {
        Pos2::new(
            node.editor_pos[0] * self.zoom + origin.x + NODE_WIDTH * self.zoom,
            node.editor_pos[1] * self.zoom
                + origin.y
                + (NODE_HEADER_H + slot as f32 * SLOT_HEIGHT + SLOT_HEIGHT * 0.5) * self.zoom,
        )
    }

    fn draw_grid(&self, painter: &egui::Painter, rect: Rect) {
        let step = GRID_SPACING * self.zoom;
        let offset_x = self.scroll.x % step;
        let offset_y = self.scroll.y % step;
        let color = Color32::from_rgb(35, 35, 38);

        let mut x = rect.min.x + offset_x;
        while x < rect.max.x {
            painter.line_segment(
                [Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)],
                Stroke::new(1.0, color),
            );
            x += step;
        }
        let mut y = rect.min.y + offset_y;
        while y < rect.max.y {
            painter.line_segment(
                [Pos2::new(rect.min.x, y), Pos2::new(rect.max.x, y)],
                Stroke::new(1.0, color),
            );
            y += step;
        }
    }

    fn draw_bezier_connection(
        &self,
        painter: &egui::Painter,
        from: Pos2,
        to: Pos2,
        color: Color32,
    ) {
        let dx = (to.x - from.x).abs() * 0.5;
        let cp1 = Pos2::new(from.x + dx, from.y);
        let cp2 = Pos2::new(to.x - dx, to.y);
        let points: Vec<Pos2> = (0..=32)
            .map(|i| {
                let t = i as f32 / 32.0;
                let inv = 1.0 - t;
                Pos2::new(
                    inv.powi(3) * from.x
                        + 3.0 * inv.powi(2) * t * cp1.x
                        + 3.0 * inv * t.powi(2) * cp2.x
                        + t.powi(3) * to.x,
                    inv.powi(3) * from.y
                        + 3.0 * inv.powi(2) * t * cp1.y
                        + 3.0 * inv * t.powi(2) * cp2.y
                        + t.powi(3) * to.y,
                )
            })
            .collect();
        for pair in points.windows(2) {
            painter.line_segment([pair[0], pair[1]], Stroke::new(2.0 * self.zoom, color));
        }
    }

    fn node_header_color(&self, kind: &ShaderNodeKind) -> Color32 {
        match kind {
            ShaderNodeKind::PbrOutput => Color32::from_rgb(180, 60, 60),
            ShaderNodeKind::TextureSample { .. } => Color32::from_rgb(60, 130, 180),
            ShaderNodeKind::ConstFloat(_)
            | ShaderNodeKind::ConstVec2(_)
            | ShaderNodeKind::ConstVec3(_)
            | ShaderNodeKind::ConstVec4(_)
            | ShaderNodeKind::ConstColor(_) => Color32::from_rgb(60, 160, 80),
            ShaderNodeKind::Add
            | ShaderNodeKind::Subtract
            | ShaderNodeKind::Multiply
            | ShaderNodeKind::Divide
            | ShaderNodeKind::Power
            | ShaderNodeKind::Sqrt
            | ShaderNodeKind::Abs
            | ShaderNodeKind::Dot
            | ShaderNodeKind::Cross
            | ShaderNodeKind::Normalize
            | ShaderNodeKind::Length
            | ShaderNodeKind::Saturate
            | ShaderNodeKind::Negate
            | ShaderNodeKind::OneMinus
            | ShaderNodeKind::Clamp => Color32::from_rgb(140, 100, 180),
            ShaderNodeKind::Lerp | ShaderNodeKind::Smoothstep => Color32::from_rgb(180, 140, 60),
            ShaderNodeKind::Fresnel => Color32::from_rgb(60, 180, 160),
            _ => Color32::from_rgb(100, 100, 110),
        }
    }
}
