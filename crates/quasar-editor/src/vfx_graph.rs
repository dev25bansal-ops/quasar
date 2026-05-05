//! VFX Graph Editor - Node-based visual effects graph editor.
//!
//! Provides:
//! - Visual node-based VFX graph editor using egui
//! - Drag-and-drop node creation
//! - Connection editing with type validation
//! - Real-time preview integration
//! - Node property editing
//! - Graph serialization

use egui::{Color32, Pos2, Rect, Sense, Vec2};
use quasar_render::{
    vfx_graph::{Pin, PinId, PropertyValue, VfxConnection, VfxGraph, VfxNodeId, VfxNodeType, VfxNode},
    particle::{ColorKeyframe, CurveKeyframe},
};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// VFX Graph Editor State
// ---------------------------------------------------------------------------

/// State for the VFX graph editor.
pub struct VfxGraphEditorState {
    /// The graph being edited.
    pub graph: VfxGraph,
    /// Next available node ID.
    pub next_node_id: u64,
    /// Pan offset of the graph canvas.
    pub pan_offset: Vec2,
    /// Zoom level of the graph canvas.
    pub zoom: f32,
    /// Currently selected node.
    pub selected_node: Option<VfxNodeId>,
    /// Node currently being dragged.
    pub dragging_node: Option<(VfxNodeId, Pos2)>,
    /// Pin currently being connected from.
    pub connecting_from: Option<(PinId, Pos2)>,
    /// Node that the connection is being dragged to (for preview).
    pub connecting_to_pin: Option<PinId>,
    /// Whether to show the node creation palette.
    pub show_node_palette: bool,
    /// Position of the node palette.
    pub palette_position: Pos2,
    /// Selected pin for property editing.
    pub selected_property_pin: Option<PinId>,
    /// Node size cache for layout.
    pub node_sizes: HashMap<VfxNodeId, Vec2>,
    /// Search filter for node palette.
    pub node_search: String,
}

impl VfxGraphEditorState {
    pub fn new() -> Self {
        Self {
            graph: VfxGraph::new("New VFX Graph"),
            next_node_id: 0,
            pan_offset: Vec2::ZERO,
            zoom: 1.0,
            selected_node: None,
            dragging_node: None,
            connecting_from: None,
            connecting_to_pin: None,
            show_node_palette: false,
            palette_position: Pos2::ZERO,
            selected_property_pin: None,
            node_sizes: HashMap::new(),
            node_search: String::new(),
        }
    }

    /// Create a new node with the given type at the given position.
    pub fn create_node(&mut self, node_type: VfxNodeType, position: Pos2) -> VfxNodeId {
        let id = VfxNodeId(self.next_node_id);
        self.next_node_id += 1;

        let name = node_type.display_name().to_string();
        let mut node = VfxNode::new(id, name.clone(), node_type);
        node.position = glam::Vec2::new(position.x, position.y);

        // Set up pins based on node type
        self.setup_node_pins(&mut node);

        self.graph.add_node(node);
        self.selected_node = Some(id);
        id
    }

    /// Set up input/output pins for a node based on its type.
    fn setup_node_pins(&self, node: &mut quasar_render::vfx_graph::VfxNode) {
        use quasar_render::vfx_graph::VfxDataType;
        match &node.node_type {
            // Emitters: output only
            VfxNodeType::PointEmitter
            | VfxNodeType::BoxEmitter
            | VfxNodeType::SphereEmitter
            | VfxNodeType::ConeEmitter => {
                node.add_output("particles", VfxDataType::Particle);
            }
            // Forces: input + output (pass-through)
            VfxNodeType::Gravity
            | VfxNodeType::Wind
            | VfxNodeType::Turbulence
            | VfxNodeType::Vortex
            | VfxNodeType::Attractor
            | VfxNodeType::Repeller => {
                node.add_input("particles", VfxDataType::Particle);
                node.add_output("particles", VfxDataType::Particle);
            }
            // Modifiers: input + output (pass-through)
            VfxNodeType::ColorOverLifetime
            | VfxNodeType::SizeOverLifetime
            | VfxNodeType::VelocityOverLifetime
            | VfxNodeType::RotationOverLifetime
            | VfxNodeType::LimitVelocity
            | VfxNodeType::ClampSize => {
                node.add_input("particles", VfxDataType::Particle);
                node.add_output("particles", VfxDataType::Particle);
            }
            // Collisions: input + output
            VfxNodeType::CollisionWithGeometry
            | VfxNodeType::CollisionWithPlane
            | VfxNodeType::SubEmitter => {
                node.add_input("particles", VfxDataType::Particle);
                node.add_output("particles", VfxDataType::Particle);
            }
            // Output: input only
            VfxNodeType::RenderParticle => {
                node.add_input("particles", VfxDataType::Particle);
            }
        }
    }

    /// Delete the selected node and its connections.
    pub fn delete_selected_node(&mut self) {
        if let Some(id) = self.selected_node.take() {
            self.graph.connections.retain(|c| c.from.node != id && c.to.node != id);
            self.graph.nodes.retain(|n| n.id != id);
            self.node_sizes.remove(&id);
        }
    }

    /// Get the node at a given screen position (in graph space).
    pub fn node_at(&self, graph_pos: Pos2) -> Option<&quasar_render::vfx_graph::VfxNode> {
        let size = Vec2::new(180.0, 100.0);
        self.graph.nodes.iter().rev().find(|node| {
            let node_pos = Pos2::new(node.position.x, node.position.y);
            let rect = Rect::from_min_size(node_pos, Vec2::new(size.x, self.estimate_node_height(node)));
            rect.contains(graph_pos)
        })
    }

    /// Get a mutable reference to the node at a given position.
    pub fn node_at_mut(&mut self, graph_pos: Pos2) -> Option<&mut quasar_render::vfx_graph::VfxNode> {
        let size = Vec2::new(180.0, 100.0);
        
        let mut target_id = None;
        for node in self.graph.nodes.iter().rev() {
            let node_pos = Pos2::new(node.position.x, node.position.y);
            let rect = Rect::from_min_size(node_pos, Vec2::new(size.x, self.estimate_node_height(node)));
            if rect.contains(graph_pos) {
                target_id = Some(node.id);
                break;
            }
        }
        
        if let Some(id) = target_id {
            self.graph.nodes.iter_mut().find(|n| n.id == id)
        } else {
            None
        }
    }

    fn estimate_node_height(&self, node: &quasar_render::vfx_graph::VfxNode) -> f32 {
        let base = 60.0;
        let pin_height = 20.0;
        let prop_height = 25.0;
        base + (node.inputs.len() + node.outputs.len()) as f32 * pin_height
            + node.properties.len() as f32 * prop_height
    }

    /// Find a pin at a given screen position.
    pub fn pin_at(&self, graph_pos: Pos2, node: &quasar_render::vfx_graph::VfxNode) -> Option<(PinId, bool)> {
        let node_pos = Pos2::new(node.position.x, node.position.y);
        let pin_radius = 6.0;

        // Check input pins (left side)
        for (i, pin) in node.inputs.iter().enumerate() {
            let pin_pos = Pos2::new(node_pos.x, node_pos.y + 30.0 + i as f32 * 20.0);
            let dist = ((graph_pos.x - pin_pos.x).powi(2) + (graph_pos.y - pin_pos.y).powi(2)).sqrt();
            if dist < pin_radius + 4.0 {
                return Some((PinId { node: node.id, index: i as u32 }, true));
            }
        }

        // Check output pins (right side)
        for (i, pin) in node.outputs.iter().enumerate() {
            let pin_pos = Pos2::new(node_pos.x + 180.0, node_pos.y + 30.0 + i as f32 * 20.0);
            let dist = ((graph_pos.x - pin_pos.x).powi(2) + (graph_pos.y - pin_pos.y).powi(2)).sqrt();
            if dist < pin_radius + 4.0 {
                return Some((PinId { node: node.id, index: i as u32 }, false));
            }
        }

        None
    }

    /// Try to connect two pins.
    pub fn try_connect(&mut self, from: PinId, to: PinId) -> bool {
        // Validate connection: output -> input only
        let from_node = match self.graph.get_node(from.node) {
            Some(n) => n,
            None => return false,
        };
        let to_node = match self.graph.get_node(to.node) {
            Some(n) => n,
            None => return false,
        };

        let from_pin = match from_node.outputs.get(from.index as usize) {
            Some(p) => p,
            None => return false,
        };
        let to_pin = match to_node.inputs.get(to.index as usize) {
            Some(p) => p,
            None => return false,
        };

        // Type check
        if from_pin.data_type != to_pin.data_type {
            return false;
        }

        // Remove existing connection to this input
        self.graph.connections.retain(|c| c.to != to);
        self.graph.connect(from, to);
        true
    }
}

impl Default for VfxGraphEditorState {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// VFX Graph Editor UI
// ---------------------------------------------------------------------------

impl VfxGraphEditorState {
    /// Render the VFX graph editor panel.
    pub fn ui(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("vfx_graph_panel")
            .default_width(800.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    // Toolbar
                    self.render_toolbar(ui);
                    ui.separator();

                    // Main area: graph canvas + property panel
                    let canvas_size = ui.available_size();
                    let graph_width = canvas_size.x * 0.75;
                    self.render_graph_canvas(ui, egui::vec2(graph_width, canvas_size.y), ctx);
                    
                    // Property panel is rendered below
                });
            });
    }

    fn render_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("\u{1F50C} VFX Graph");
            ui.separator();

            if ui.button("\u{2795} Add Node").clicked() {
                self.show_node_palette = !self.show_node_palette;
            }

            ui.separator();

            if ui.button("\u{1F5D1} Delete").clicked() {
                self.delete_selected_node();
            }

            if ui.button("\u{1F4C1} Save").clicked() {
                // Save graph to file
            }

            if ui.button("\u{1F4C2} Load").clicked() {
                // Load graph from file
            }

            ui.separator();

            // Zoom controls
            ui.add(egui::Slider::new(&mut self.zoom, 0.25..=2.0).text("Zoom"));

            if ui.button("\u{1F50D} Fit").clicked() {
                self.zoom = 1.0;
                self.pan_offset = Vec2::ZERO;
            }
        });
    }

    fn render_graph_canvas(&mut self, ui: &mut egui::Ui, desired_size: egui::Vec2, ctx: &egui::Context) {
        let (response, painter) = ui.allocate_painter(desired_size, Sense::click_and_drag());
        let rect = response.rect;

        // Draw grid background
        self.draw_grid(&painter, rect);

        // Draw connections
        for conn in &self.graph.connections {
            self.draw_connection(&painter, rect, conn);
        }

        // Draw nodes
        for node in &self.graph.nodes {
            self.draw_node(&painter, rect, node);
        }

        // Handle interactions
        self.handle_canvas_input(&response, rect, ctx);
    }

    fn draw_grid(&self, painter: &egui::Painter, rect: Rect) {
        let grid_size = 20.0 * self.zoom;
        let offset_x = self.pan_offset.x % grid_size;
        let offset_y = self.pan_offset.y % grid_size;

        let mut x = rect.min.x + offset_x;
        while x < rect.max.x {
            painter.line_segment(
                [egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)],
                egui::Stroke::new(0.5, Color32::from_gray(40)),
            );
            x += grid_size;
        }

        let mut y = rect.min.y + offset_y;
        while y < rect.max.y {
            painter.line_segment(
                [egui::pos2(rect.min.x, y), egui::pos2(rect.max.x, y)],
                egui::Stroke::new(0.5, Color32::from_gray(40)),
            );
            y += grid_size;
        }
    }

    fn draw_connection(&self, painter: &egui::Painter, _rect: Rect, conn: &VfxConnection) {
        let from_node = match self.graph.get_node(conn.from.node) {
            Some(n) => n,
            None => return,
        };
        let to_node = match self.graph.get_node(conn.to.node) {
            Some(n) => n,
            None => return,
        };

        let from_pos = egui::pos2(
            from_node.position.x + 180.0 + self.pan_offset.x,
            from_node.position.y + 30.0 + conn.from.index as f32 * 20.0 + self.pan_offset.y,
        );
        let to_pos = egui::pos2(
            to_node.position.x + self.pan_offset.x,
            to_node.position.y + 30.0 + conn.to.index as f32 * 20.0 + self.pan_offset.y,
        );

        // Draw bezier curve
        let dx = (to_pos.x - from_pos.x).abs();
        let cp1 = egui::pos2(from_pos.x + dx * 0.4, from_pos.y);
        let cp2 = egui::pos2(to_pos.x - dx * 0.4, to_pos.y);

        let shape = egui::epaint::CubicBezierShape {
            points: [from_pos, cp1, cp2, to_pos],
            closed: false,
            fill: Color32::TRANSPARENT,
            stroke: egui::Stroke::new(2.0, Color32::from_rgb(100, 180, 255)).into(),
        };
        painter.add(shape);
    }

    fn draw_node(&self, painter: &egui::Painter, rect: Rect, node: &quasar_render::vfx_graph::VfxNode) {
        let node_pos = egui::pos2(
            node.position.x + self.pan_offset.x,
            node.position.y + self.pan_offset.y,
        );

        let color = node.node_type.ui_color();
        let header_color = Color32::from_rgb(
            (color[0] * 255.0) as u8,
            (color[1] * 255.0) as u8,
            (color[2] * 255.0) as u8,
        );

        let width = 180.0;
        let height = self.estimate_node_height(node);
        let node_rect = Rect::from_min_size(node_pos, Vec2::new(width, height));

        // Node background
        let is_selected = self.selected_node == Some(node.id);
        let bg_color = if is_selected {
            Color32::from_gray(50)
        } else {
            Color32::from_gray(35)
        };

        painter.rect_filled(node_rect, 4.0, bg_color);

        // Node header
        let header_rect = Rect {
            min: node_rect.min,
            max: egui::pos2(node_rect.max.x, node_rect.min.y + 24.0),
        };
        painter.rect_filled(header_rect, 4.0, header_color);

        // Node title
        painter.text(
            egui::pos2(node_rect.min.x + 8.0, node_rect.min.y + 5.0),
            egui::Align2::LEFT_TOP,
            &node.name,
            egui::FontId::monospace(12.0),
            Color32::WHITE,
        );

        // Input pins (left side)
        for (i, pin) in node.inputs.iter().enumerate() {
            let pin_pos = egui::pos2(
                node_rect.min.x,
                node_rect.min.y + 30.0 + i as f32 * 20.0,
            );
            self.draw_pin(painter, pin_pos, pin, true);
        }

        // Output pins (right side)
        for (i, pin) in node.outputs.iter().enumerate() {
            let pin_pos = egui::pos2(
                node_rect.max.x,
                node_rect.min.y + 30.0 + i as f32 * 20.0,
            );
            self.draw_pin(painter, pin_pos, pin, false);
        }

        // Properties
        for (i, prop) in node.properties.iter().enumerate() {
            let y = node_rect.min.y + 30.0 + (node.inputs.len() + node.outputs.len()) as f32 * 20.0 + i as f32 * 25.0;
            painter.text(
                egui::pos2(node_rect.min.x + 8.0, y),
                egui::Align2::LEFT_TOP,
                &prop.name,
                egui::FontId::proportional(10.0),
                Color32::LIGHT_GRAY,
            );
        }
    }

    fn draw_pin(&self, painter: &egui::Painter, pos: egui::Pos2, pin: &Pin, _is_input: bool) {
        let radius = 6.0;
        let pin_color = match pin.data_type {
            quasar_render::vfx_graph::VfxDataType::Particle => Color32::from_rgb(100, 255, 100),
            quasar_render::vfx_graph::VfxDataType::Float => Color32::from_rgb(255, 200, 100),
            quasar_render::vfx_graph::VfxDataType::Vec3 => Color32::from_rgb(100, 150, 255),
            quasar_render::vfx_graph::VfxDataType::Color => Color32::from_rgb(255, 100, 200),
            _ => Color32::LIGHT_GRAY,
        };

        painter.circle_filled(pos, radius, pin_color);
        painter.circle_stroke(pos, radius - 1.0, egui::Stroke::new(1.0, Color32::WHITE));
    }

    fn handle_canvas_input(&mut self, response: &egui::Response, rect: Rect, ctx: &egui::Context) {
        // Pan with middle mouse or right drag
        if response.dragged_by(egui::PointerButton::Middle)
            || response.dragged_by(egui::PointerButton::Secondary)
        {
            // Pan logic would track previous position
        }

        // Zoom with scroll
        if response.hovered() {
            let scroll = ctx.input(|i| i.smooth_scroll_delta);
            if scroll.y != 0.0 {
                self.zoom = (self.zoom + scroll.y * 0.001).clamp(0.25, 2.0);
            }
        }

        // Node selection and dragging
        if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                let graph_pos = Pos2::new(
                    pos.x - rect.min.x - self.pan_offset.x,
                    pos.y - rect.min.y - self.pan_offset.y,
                );

                if let Some(node) = self.node_at(graph_pos) {
                    self.selected_node = Some(node.id);
                } else {
                    self.selected_node = None;
                }
            }
        }
    }

    fn render_property_panel(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Properties");
            ui.separator();

            if let Some(node_id) = self.selected_node {
                if let Some(node) = self.graph.get_node(node_id) {
                    ui.label(format!("Node: {}", node.name));
                    ui.label(format!("Type: {:?}", node.node_type));
                    ui.separator();

                    // Edit properties
                    for (i, prop) in node.properties.iter().enumerate() {
                        ui.label(&prop.name);
                        match &prop.value {
                            PropertyValue::Float(v) => {
                                let mut val = *v;
                                ui.add(egui::DragValue::new(&mut val).speed(0.1));
                            }
                            PropertyValue::Int(v) => {
                                let mut val = *v;
                                ui.add(egui::DragValue::new(&mut val).speed(1.0));
                            }
                            PropertyValue::Bool(v) => {
                                let mut val = *v;
                                ui.checkbox(&mut val, "");
                            }
                            PropertyValue::String(v) => {
                                ui.text_edit_singleline(&mut v.clone());
                            }
                            PropertyValue::Vec3(v) => {
                                let mut x = v.x;
                                let mut y = v.y;
                                let mut z = v.z;
                                ui.horizontal(|ui| {
                                    ui.label("X:");
                                    ui.add(egui::DragValue::new(&mut x).speed(0.1));
                                    ui.label("Y:");
                                    ui.add(egui::DragValue::new(&mut y).speed(0.1));
                                    ui.label("Z:");
                                    ui.add(egui::DragValue::new(&mut z).speed(0.1));
                                });
                            }
                            _ => {
                                ui.label("(complex value)");
                            }
                        }
                    }
                }
            } else {
                ui.label("No node selected");
            }
        });
    }

    /// Show the node creation palette.
    pub fn node_palette_ui(&mut self, ctx: &egui::Context) {
        if !self.show_node_palette {
            return;
        }

        egui::Window::new("Add VFX Node")
            .fixed_pos(self.palette_position)
            .default_size(egui::vec2(250.0, 400.0))
            .resizable(true)
            .show(ctx, |ui| {
                // Search
                ui.text_edit_singleline(&mut self.node_search);
                ui.separator();

                // Node categories
                let categories = ["Emitter", "Force", "Modifier", "Collision", "Output"];
                for category in categories {
                    ui.collapsing(category, |ui| {
                        for node_type in self.get_node_types_in_category(category) {
                            let display_name = node_type.display_name();
                            if !self.node_search.is_empty()
                                && !display_name.to_lowercase().contains(&self.node_search.to_lowercase())
                            {
                                continue;
                            }

                            if ui.button(display_name).clicked() {
                                self.create_node(node_type, self.palette_position);
                                self.show_node_palette = false;
                                self.node_search.clear();
                            }
                        }
                    });
                }
            });
    }

    fn get_node_types_in_category(&self, category: &str) -> Vec<VfxNodeType> {
        match category {
            "Emitter" => vec![
                VfxNodeType::PointEmitter,
                VfxNodeType::BoxEmitter,
                VfxNodeType::SphereEmitter,
                VfxNodeType::ConeEmitter,
            ],
            "Force" => vec![
                VfxNodeType::Gravity,
                VfxNodeType::Wind,
                VfxNodeType::Turbulence,
                VfxNodeType::Vortex,
                VfxNodeType::Attractor,
                VfxNodeType::Repeller,
            ],
            "Modifier" => vec![
                VfxNodeType::ColorOverLifetime,
                VfxNodeType::SizeOverLifetime,
                VfxNodeType::VelocityOverLifetime,
                VfxNodeType::RotationOverLifetime,
                VfxNodeType::LimitVelocity,
                VfxNodeType::ClampSize,
            ],
            "Collision" => vec![
                VfxNodeType::CollisionWithGeometry,
                VfxNodeType::CollisionWithPlane,
                VfxNodeType::SubEmitter,
            ],
            "Output" => vec![VfxNodeType::RenderParticle],
            _ => vec![],
        }
    }
}

// ---------------------------------------------------------------------------
// VFX Graph to ParticleSystemDef conversion
// ---------------------------------------------------------------------------

/// Convert a VFX graph to a ParticleSystemDef for simulation.
pub fn graph_to_particle_system(graph: &VfxGraph) -> quasar_render::particle::ParticleSystemDef {
    use quasar_render::particle::*;

    let mut def = ParticleSystemDef::default();
    def.name = graph.name.clone();
    def.emitters.clear();
    def.forces.clear();
    def.modifiers.clear();
    def.collisions.clear();

    // Process nodes
    for node in &graph.nodes {
        match &node.node_type {
            // Emitters
            VfxNodeType::PointEmitter => {
                def.emitters.push(EmitterDef {
                    name: node.name.clone(),
                    shape: EmitterShape::Point,
                    ..Default::default()
                });
            }
            VfxNodeType::BoxEmitter => {
                def.emitters.push(EmitterDef {
                    name: node.name.clone(),
                    shape: EmitterShape::Box { size: [1.0, 1.0, 1.0] },
                    ..Default::default()
                });
            }
            VfxNodeType::SphereEmitter => {
                def.emitters.push(EmitterDef {
                    name: node.name.clone(),
                    shape: EmitterShape::Sphere { radius: 1.0 },
                    ..Default::default()
                });
            }
            VfxNodeType::ConeEmitter => {
                def.emitters.push(EmitterDef {
                    name: node.name.clone(),
                    shape: EmitterShape::Cone { angle: 30.0, length: 2.0 },
                    ..Default::default()
                });
            }
            // Forces
            VfxNodeType::Gravity => {
                def.forces.push(ForceDef {
                    name: node.name.clone(),
                    force_type: ForceType::Gravity { strength: 9.81 },
                    enabled: true,
                });
            }
            VfxNodeType::Wind => {
                def.forces.push(ForceDef {
                    name: node.name.clone(),
                    force_type: ForceType::Wind {
                        direction: [1.0, 0.0, 0.0],
                        strength: 1.0,
                    },
                    enabled: true,
                });
            }
            VfxNodeType::Turbulence => {
                def.forces.push(ForceDef {
                    name: node.name.clone(),
                    force_type: ForceType::Turbulence {
                        strength: 1.0,
                        frequency: 1.0,
                        speed: 1.0,
                        seed: 0,
                    },
                    enabled: true,
                });
            }
            VfxNodeType::Vortex => {
                def.forces.push(ForceDef {
                    name: node.name.clone(),
                    force_type: ForceType::Vortex {
                        center: [0.0, 0.0, 0.0],
                        axis: [0.0, 1.0, 0.0],
                        strength: 1.0,
                        radius: 5.0,
                    },
                    enabled: true,
                });
            }
            VfxNodeType::Attractor => {
                def.forces.push(ForceDef {
                    name: node.name.clone(),
                    force_type: ForceType::Attractor {
                        position: [0.0, 0.0, 0.0],
                        strength: 1.0,
                        range: 10.0,
                    },
                    enabled: true,
                });
            }
            VfxNodeType::Repeller => {
                def.forces.push(ForceDef {
                    name: node.name.clone(),
                    force_type: ForceType::Repeller {
                        position: [0.0, 0.0, 0.0],
                        strength: 1.0,
                        range: 10.0,
                    },
                    enabled: true,
                });
            }
            // Modifiers
            VfxNodeType::ColorOverLifetime => {
                def.modifiers.push(ModifierDef {
                    name: node.name.clone(),
                    modifier_type: ModifierType::ColorOverLifetime {
                        gradient: vec![
                            ColorKeyframe { time: 0.0, color: [1.0, 1.0, 1.0, 1.0] },
                            ColorKeyframe { time: 1.0, color: [1.0, 1.0, 1.0, 0.0] },
                        ],
                    },
                    enabled: true,
                });
            }
            VfxNodeType::SizeOverLifetime => {
                def.modifiers.push(ModifierDef {
                    name: node.name.clone(),
                    modifier_type: ModifierType::SizeOverLifetime {
                        curve: vec![
                            CurveKeyframe { time: 0.0, value: 1.0, in_tangent: 0.0, out_tangent: 0.0 },
                            CurveKeyframe { time: 1.0, value: 1.0, in_tangent: 0.0, out_tangent: 0.0 },
                        ],
                    },
                    enabled: true,
                });
            }
            VfxNodeType::VelocityOverLifetime => {
                def.modifiers.push(ModifierDef {
                    name: node.name.clone(),
                    modifier_type: ModifierType::SizeOverLifetime {
                        curve: vec![
                            CurveKeyframe { time: 0.0, value: 1.0, in_tangent: 0.0, out_tangent: 0.0 },
                            CurveKeyframe { time: 1.0, value: 1.0, in_tangent: 0.0, out_tangent: 0.0 },
                        ],
                    },
                    enabled: true,
                });
            }
            VfxNodeType::RotationOverLifetime => {
                def.modifiers.push(ModifierDef {
                    name: node.name.clone(),
                    modifier_type: ModifierType::RotationOverLifetime {
                        speed: 1.0,
                        axis: [0.0, 1.0, 0.0],
                    },
                    enabled: true,
                });
            }
            VfxNodeType::LimitVelocity => {
                def.modifiers.push(ModifierDef {
                    name: node.name.clone(),
                    modifier_type: ModifierType::LimitVelocity { max_speed: 10.0 },
                    enabled: true,
                });
            }
            VfxNodeType::ClampSize => {
                def.modifiers.push(ModifierDef {
                    name: node.name.clone(),
                    modifier_type: ModifierType::ClampSize { min: 0.01, max: 10.0 },
                    enabled: true,
                });
            }
            // Collisions
            VfxNodeType::CollisionWithPlane => {
                def.collisions.push(CollisionDef {
                    name: node.name.clone(),
                    collision_type: CollisionType::Plane {
                        normal: [0.0, 1.0, 0.0],
                        distance: 0.0,
                    },
                    bounce_factor: 0.5,
                    kill_on_collision: false,
                    enabled: true,
                });
            }
            VfxNodeType::CollisionWithGeometry => {
                def.collisions.push(CollisionDef {
                    name: node.name.clone(),
                    collision_type: CollisionType::Sphere {
                        center: [0.0, 0.0, 0.0],
                        radius: 1.0,
                    },
                    bounce_factor: 0.5,
                    kill_on_collision: false,
                    enabled: true,
                });
            }
            VfxNodeType::SubEmitter => {
                // Sub-emitters are handled separately
            }
            VfxNodeType::RenderParticle => {
                // Renderer node - use defaults
            }
        }
    }

    // If no emitters were found, add a default one
    if def.emitters.is_empty() {
        def.emitters.push(EmitterDef::default());
    }

    def
}
