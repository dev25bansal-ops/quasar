//! AI Behavior Visual Editor Panel for Quasar Engine.
//!
//! Provides:
//! - **Tab-based interface** — manage multiple behavior trees
//! - **Node palette** — drag-and-drop node creation
//! - **Graph editor** — visual behavior tree designer
//! - **Property editor** — configure node parameters
//! - **Simulation controls** — real-time AI testing
//! - **Tree management** — save, load, import, export

#![allow(deprecated)]

use egui::{Color32, RichText, ScrollArea, Sense, Stroke, Ui, Vec2, Pos2, Rect};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::behavior_tree_graph::{BtGraphState, BtEditorNodeType, BtEditorNode, BtEditorConnection};
use quasar_core::SimulationState;
use crate::bt_simulation::*;
use crate::bt_serialization::{BtSerializer, BtDeserializer};

/// Represents a saved behavior tree in the editor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiBehaviorTree {
    pub id: u64,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub graph: BtGraphState,
    pub created_at: f64,
    pub modified_at: f64,
    pub is_dirty: bool,
}

impl AiBehaviorTree {
    pub fn new(id: u64, name: &str) -> Self {
        Self {
            id,
            name: name.to_string(),
            description: String::new(),
            tags: Vec::new(),
            graph: BtGraphState::new(name),
            created_at: 0.0,
            modified_at: 0.0,
            is_dirty: true,
        }
    }
}

/// Available AI behavior templates.
#[derive(Debug, Clone)]
pub struct BehaviorTemplate {
    pub name: &'static str,
    pub description: &'static str,
    pub category: TemplateCategory,
    pub icon: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateCategory {
    Movement,
    Combat,
    Social,
    Utility,
    Patrol,
    Flee,
    Chase,
    Search,
}

impl BehaviorTemplate {
    pub const TEMPLATES: &'static [Self] = &[
        Self {
            name: "Patrol Route",
            description: "Patrol between waypoints, return to start",
            category: TemplateCategory::Patrol,
            icon: "\u{1F6B6}",
        },
        Self {
            name: "Chase & Attack",
            description: "Detect target, chase, and attack when in range",
            category: TemplateCategory::Combat,
            icon: "\u{2694}",
        },
        Self {
            name: "Flee Danger",
            description: "Detect threat, flee to safe location",
            category: TemplateCategory::Flee,
            icon: "\u{1F3C3}",
        },
        Self {
            name: "Guard Post",
            description: "Stay at post, investigate disturbances",
            category: TemplateCategory::Patrol,
            icon: "\u{1F6E1}",
        },
        Self {
            name: "Wander & Idle",
            description: "Random wandering with idle periods",
            category: TemplateCategory::Movement,
            icon: "\u{1F3B2}",
        },
        Self {
            name: "Flock Behavior",
            description: "Separation, alignment, cohesion with group",
            category: TemplateCategory::Social,
            icon: "\u{1F426}",
        },
        Self {
            name: "Search & Investigate",
            description: "Search area, investigate points of interest",
            category: TemplateCategory::Search,
            icon: "\u{1F50D}",
        },
        Self {
            name: "Resource Gather",
            description: "Find, collect, and deliver resources",
            category: TemplateCategory::Utility,
            icon: "\u{26CF}",
        },
    ];
}

/// Main AI Behavior Editor state.
pub struct AiBehaviorEditor {
    /// Whether the AI editor panel is visible.
    pub visible: bool,
    /// Currently open behavior trees.
    pub trees: Vec<AiBehaviorTree>,
    /// Index of the currently active tree tab.
    pub active_tree_idx: Option<usize>,
    /// Node palette visibility.
    pub show_node_palette: bool,
    /// Property editor visibility.
    pub show_property_editor: bool,
    /// Blackboard editor visibility.
    pub show_blackboard_editor: bool,
    /// Simulation panel visibility.
    pub show_simulation: bool,
    /// Template browser visibility.
    pub show_templates: bool,
    /// Search filter for node palette.
    pub node_search: String,
    /// Selected template category filter.
    pub template_category_filter: Option<TemplateCategory>,
    /// Tree search filter.
    pub tree_search: String,
    /// Next available tree ID.
    pub next_tree_id: u64,
    /// Simulation engine (shared across trees).
    pub simulation: BtSimulation,
    /// Pending save path for the active tree.
    pub save_path: String,
    /// Pending load path.
    pub load_path: String,
    /// Notification messages.
    pub notifications: Vec<String>,
    /// Undo history for the active tree.
    undo_stack: Vec<BtGraphState>,
    redo_stack: Vec<BtGraphState>,
}

impl AiBehaviorEditor {
    pub fn new() -> Self {
        Self {
            visible: false,
            trees: Vec::new(),
            active_tree_idx: None,
            show_node_palette: true,
            show_property_editor: true,
            show_blackboard_editor: false,
            show_simulation: false,
            show_templates: false,
            node_search: String::new(),
            template_category_filter: None,
            tree_search: String::new(),
            next_tree_id: 1,
            simulation: BtSimulation::new(),
            save_path: String::new(),
            load_path: String::new(),
            notifications: Vec::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    /// Toggle the AI editor panel visibility.
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    /// Create a new empty behavior tree and select it.
    pub fn create_new_tree(&mut self, name: &str) -> u64 {
        let id = self.next_tree_id;
        self.next_tree_id += 1;
        let tree = AiBehaviorTree::new(id, name);
        self.trees.push(tree);
        let idx = self.trees.len() - 1;
        self.active_tree_idx = Some(idx);
        id
    }

    /// Create a behavior tree from a template.
    pub fn create_from_template(&mut self, template: &BehaviorTemplate) -> u64 {
        let id = self.next_tree_id;
        self.next_tree_id += 1;
        let mut tree = AiBehaviorTree::new(id, template.name);
        tree.description = template.description.to_string();
        tree.graph = Self::build_template_graph(template);
        self.trees.push(tree);
        let idx = self.trees.len() - 1;
        self.active_tree_idx = Some(idx);
        id
    }

    /// Remove the behavior tree at the given index.
    pub fn remove_tree(&mut self, idx: usize) {
        if idx < self.trees.len() {
            self.trees.remove(idx);
            if self.trees.is_empty() {
                self.active_tree_idx = None;
            } else if self.active_tree_idx.map_or(false, |i| i >= idx) {
                self.active_tree_idx = self.active_tree_idx.map(|i| i.saturating_sub(1));
            }
        }
    }

    /// Save the active tree to JSON string.
    pub fn save_active_tree(&mut self) -> Option<String> {
        let idx = self.active_tree_idx?;
        let tree = &mut self.trees[idx];
        match BtSerializer::serialize_tree(&tree.graph) {
            Ok(json) => {
                tree.is_dirty = false;
                let name = tree.name.clone();
                self.add_notification(format!("Saved tree '{}'", name));
                Some(json)
            }
            Err(e) => {
                self.add_notification(format!("Save failed: {}", e));
                None
            }
        }
    }

    /// Load a behavior tree from JSON string into the active tree.
    pub fn load_active_tree(&mut self, json: &str) {
        let idx = match self.active_tree_idx {
            Some(i) => i,
            None => {
                // Create a new tree to load into
                self.create_new_tree("Imported Tree");
                self.active_tree_idx.unwrap()
            }
        };
        match BtDeserializer::deserialize_tree(json) {
            Ok(graph) => {
                self.save_undo_state();
                self.trees[idx].graph = graph;
                self.trees[idx].is_dirty = true;
                self.add_notification("Tree loaded successfully".to_string());
            }
            Err(e) => {
                self.add_notification(format!("Load failed: {}", e));
            }
        }
    }

    /// Export the active tree to a JSON file path.
    pub fn export_to_file(&mut self, path: &str) {
        if let Some(json) = self.save_active_tree() {
            if std::fs::write(path, &json).is_ok() {
                self.add_notification(format!("Exported to {}", path));
            } else {
                self.add_notification(format!("Failed to write {}", path));
            }
        }
    }

    /// Import a tree from a JSON file.
    pub fn import_from_file(&mut self, path: &str, name: &str) {
        match std::fs::read_to_string(path) {
            Ok(json) => {
                if let Ok(graph) = BtDeserializer::deserialize_tree(&json) {
                    let id = self.create_new_tree(name);
                    if let Some(idx) = self.active_tree_idx {
                        self.trees[idx].graph = graph;
                        self.add_notification(format!("Imported from {}", path));
                    }
                    let _ = id;
                }
            }
            Err(e) => {
                self.add_notification(format!("Import failed: {}", e));
            }
        }
    }

    /// Build a template graph for the given template.
    fn build_template_graph(template: &BehaviorTemplate) -> BtGraphState {
        let mut graph = BtGraphState::new(template.name);

        match template.name {
            "Patrol Route" => {
                let root = graph.add_node(BtEditorNodeType::Selector, "Root", Pos2::new(250.0, 30.0));
                let patrol_seq = graph.add_node(BtEditorNodeType::Sequence, "Patrol", Pos2::new(100.0, 150.0));
                let move_wp = graph.add_node(BtEditorNodeType::Action, "MoveToWaypoint", Pos2::new(50.0, 270.0));
                let wait = graph.add_node(BtEditorNodeType::Wait, "WaitAtWP", Pos2::new(150.0, 270.0));
                let check_alert = graph.add_node(BtEditorNodeType::Condition, "IsAlerted?", Pos2::new(300.0, 150.0));
                let investigate = graph.add_node(BtEditorNodeType::Action, "Investigate", Pos2::new(400.0, 270.0));

                graph.add_connection(root, patrol_seq);
                graph.add_connection(root, check_alert);
                graph.add_connection(patrol_seq, move_wp);
                graph.add_connection(patrol_seq, wait);
                graph.add_connection(check_alert, investigate);
            }
            "Chase & Attack" => {
                let root = graph.add_node(BtEditorNodeType::Selector, "Root", Pos2::new(250.0, 30.0));
                let in_range = graph.add_node(BtEditorNodeType::Sequence, "InRange", Pos2::new(100.0, 150.0));
                let attack = graph.add_node(BtEditorNodeType::Action, "Attack", Pos2::new(50.0, 270.0));
                let chase = graph.add_node(BtEditorNodeType::Action, "ChaseTarget", Pos2::new(250.0, 150.0));
                let detect = graph.add_node(BtEditorNodeType::Condition, "HasTarget?", Pos2::new(400.0, 150.0));
                let search = graph.add_node(BtEditorNodeType::Action, "SearchArea", Pos2::new(450.0, 270.0));

                graph.add_connection(root, in_range);
                graph.add_connection(root, chase);
                graph.add_connection(root, detect);
                graph.add_connection(in_range, attack);
                graph.add_connection(detect, search);
            }
            "Flee Danger" => {
                let root = graph.add_node(BtEditorNodeType::Selector, "Root", Pos2::new(200.0, 30.0));
                let check_danger = graph.add_node(BtEditorNodeType::Condition, "IsInDanger?", Pos2::new(100.0, 150.0));
                let flee = graph.add_node(BtEditorNodeType::Action, "FleeToSafety", Pos2::new(50.0, 270.0));
                let hide = graph.add_node(BtEditorNodeType::Action, "FindCover", Pos2::new(200.0, 270.0));

                graph.add_connection(root, check_danger);
                graph.add_connection(check_danger, flee);
                graph.add_connection(root, hide);
            }
            _ => {
                // Generic default template
                let root = graph.add_node(BtEditorNodeType::Selector, "Root", Pos2::new(200.0, 30.0));
                let seq = graph.add_node(BtEditorNodeType::Sequence, "Sequence", Pos2::new(100.0, 150.0));
                let action = graph.add_node(BtEditorNodeType::Action, "DoAction", Pos2::new(50.0, 270.0));
                graph.add_connection(root, seq);
                graph.add_connection(root, action);
                graph.add_connection(seq, action);
            }
        }

        graph
    }

    /// Save the current state for undo.
    fn save_undo_state(&mut self) {
        if let Some(idx) = self.active_tree_idx {
            self.undo_stack.push(self.trees[idx].graph.clone());
            self.redo_stack.clear();
            if self.undo_stack.len() > 50 {
                self.undo_stack.remove(0);
            }
        }
    }

    /// Undo the last graph change.
    pub fn undo(&mut self) {
        if let Some(idx) = self.active_tree_idx {
            if let Some(state) = self.undo_stack.pop() {
                self.redo_stack.push(self.trees[idx].graph.clone());
                self.trees[idx].graph = state;
            }
        }
    }

    /// Redo a previously undone graph change.
    pub fn redo(&mut self) {
        if let Some(idx) = self.active_tree_idx {
            if let Some(state) = self.redo_stack.pop() {
                self.undo_stack.push(self.trees[idx].graph.clone());
                self.trees[idx].graph = state;
            }
        }
    }

    /// Add a notification message.
    fn add_notification(&mut self, msg: String) {
        self.notifications.push(msg);
        if self.notifications.len() > 20 {
            self.notifications.remove(0);
        }
    }

    /// Show the full AI editor UI (call from the main editor).
    pub fn ui(&mut self, ctx: &egui::Context) {
        if !self.visible {
            return;
        }

        egui::Window::new("\u{1F9E0} AI Behavior Editor")
            .default_size([900.0, 600.0])
            .resizable(true)
            .show(ctx, |ui| {
                self.top_toolbar(ui);
                ui.separator();
                ui.horizontal(|ui| {
                    // Left sidebar: tree list + node palette
                    ui.vertical(|ui| {
                        ui.set_max_width(220.0);
                        self.tree_list_panel(ui);
                        ui.separator();
                        if self.show_node_palette {
                            self.node_palette_panel(ui);
                        }
                    });
                    ui.separator();
                    // Center: graph editor
                    ui.vertical(|ui| {
                        self.graph_editor_panel(ui);
                    });
                    ui.separator();
                    // Right sidebar: properties
                    ui.vertical(|ui| {
                        ui.set_max_width(250.0);
                        if self.show_property_editor {
                            self.property_editor_panel(ui);
                        }
                    });
                });
                ui.separator();
                // Bottom: simulation / notifications
                if self.show_simulation {
                    self.simulation_panel(ui);
                    ui.separator();
                }
                self.notifications_panel(ui);
            });

        // Template browser as a separate modal-like window
        if self.show_templates {
            self.template_browser_window(ctx);
        }
    }

    // -----------------------------------------------------------------------
    // Sub-panels
    // -----------------------------------------------------------------------

    fn top_toolbar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if ui.button("\u{2795} New Tree").clicked() {
                let id = self.create_new_tree(&format!("Tree {}", self.next_tree_id));
                self.add_notification(format!("Created new tree (ID: {})", id));
            }
            if ui.button("\u{1F4C2} Templates").clicked() {
                self.show_templates = !self.show_templates;
            }
            ui.separator();
            if ui.button("\u{2B06} Import").clicked() {
                // In a real app, this would open a file dialog
                self.load_path = "behavior_tree.json".to_string();
            }
            if ui.button("\u{2B07} Export").clicked() {
                self.save_active_tree();
            }
            ui.separator();
            if ui.button("\u{21A9} Undo").clicked() {
                self.undo();
            }
            if ui.button("\u{21AA} Redo").clicked() {
                self.redo();
            }
            ui.separator();
            ui.toggle_value(&mut self.show_node_palette, "Palette");
            ui.toggle_value(&mut self.show_property_editor, "Properties");
            ui.toggle_value(&mut self.show_blackboard_editor, "Blackboard");
            ui.toggle_value(&mut self.show_simulation, "Simulate");
        });
    }

    fn tree_list_panel(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Behavior Trees").strong());
            ui.text_edit_singleline(&mut self.tree_search);
        });
        ui.separator();

        ScrollArea::vertical()
            .max_height(150.0)
            .show(ui, |ui| {
                let search_lower = self.tree_search.to_lowercase();
                let mut select_idx: Option<usize> = None;
                let mut delete_idx: Option<usize> = None;
                for (idx, tree) in self.trees.iter().enumerate() {
                    if !search_lower.is_empty() && !tree.name.to_lowercase().contains(&search_lower) {
                        continue;
                    }
                    let selected = self.active_tree_idx == Some(idx);
                    let label = if tree.is_dirty {
                        format!("{} *", tree.name)
                    } else {
                        tree.name.clone()
                    };
                    if ui.selectable_label(selected, label).clicked() {
                        select_idx = Some(idx);
                    }
                    if ui.ctx().input(|i| i.pointer.secondary_clicked()) {
                        delete_idx = Some(idx);
                    }
                }
                if let Some(idx) = select_idx {
                    self.active_tree_idx = Some(idx);
                }
                if let Some(idx) = delete_idx {
                    self.remove_tree(idx);
                }
            });
    }

    fn node_palette_panel(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Node Palette").strong());
            ui.text_edit_singleline(&mut self.node_search);
        });
        ui.separator();

        ScrollArea::vertical()
            .max_height(300.0)
            .show(ui, |ui| {
                let search_lower = self.node_search.to_lowercase();
                for node_type in BtEditorNodeType::all_types() {
                    let name = node_type.name();
                    if !search_lower.is_empty() && !name.to_lowercase().contains(&search_lower) {
                        continue;
                    }
                    let color = node_type.color();
                    let btn = ui.horizontal(|ui| {
                        ui.colored_label(color, format!("{}\u{2002}{}", node_type.icon(), name));
                        ui.button("+").clicked()
                    });
                    if btn.response.clicked() {
                        if let Some(idx) = self.active_tree_idx {
                            let graph = &mut self.trees[idx].graph;
                            let center = graph.viewport_center();
                            graph.add_node(*node_type, &format!("{} {}", name, graph.next_node_id), Pos2::new(center.x, center.y + 100.0));
                            self.save_undo_state();
                            self.trees[idx].is_dirty = true;
                        }
                    }
                }
            });
    }

    fn graph_editor_panel(&mut self, ui: &mut Ui) {
        if let Some(idx) = self.active_tree_idx {
            let tree = &mut self.trees[idx];
            ui.horizontal(|ui| {
                ui.label(RichText::new(&tree.name).strong());
                if tree.is_dirty {
                    ui.label(RichText::new("*").color(Color32::YELLOW));
                }
            });
            ui.separator();

            // Show simulation overlay if active
            if self.show_simulation && self.simulation.is_running() {
                self.simulation.load_from_graph(&tree.graph);
                self.simulation.step();
            }

            // Render the graph editor
            let available = ui.available_size();
            let (rect, response) = ui.allocate_exact_size(available, Sense::click_and_drag());
            if response.hovered() {
                let mut graph_state = BtGraphState::from_existing(&tree.graph);
                tree.graph = graph_state.handle_interaction(ui, &response, available);
            }

            // Use the behavior_tree_graph module's graph drawing
            tree.graph.draw_graph(ui, &rect);
        } else {
            ui.centered_and_justified(|ui| {
                ui.vertical_centered(|ui| {
                    ui.label("\u{1F9E0}");
                    ui.label("No behavior tree selected");
                    if ui.button("\u{2795} Create New Tree").clicked() {
                        self.create_new_tree("My Behavior Tree");
                    }
                    if ui.button("\u{1F4C2} Browse Templates").clicked() {
                        self.show_templates = true;
                    }
                });
            });
        }
    }

    fn property_editor_panel(&mut self, ui: &mut Ui) {
        ui.label(RichText::new("Properties").strong());
        ui.separator();

        if let Some(idx) = self.active_tree_idx {
            let selected_node = self.trees[idx].graph.selected_node;
            let node_data = selected_node.and_then(|id| {
                self.trees[idx].graph.nodes.get(&id).cloned()
            });

            if let Some(selected) = selected_node {
                if let Some(node) = node_data {
                    let mut pending_name: Option<String> = None;
                    let mut pending_props: Vec<(String, String)> = vec![];

                    ui.horizontal(|ui| {
                        ui.label("Name:");
                        let mut name = node.name.clone();
                        if ui.text_edit_singleline(&mut name).changed() {
                            pending_name = Some(name);
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Type:");
                        ui.label(node.node_type.name());
                    });
                    ui.separator();

                    match node.node_type {
                        BtEditorNodeType::Action => {
                            ui.horizontal(|ui| {
                                ui.label("Action Name:");
                                if let Some(val) = node.properties.get("action_name") {
                                    let mut v = val.clone();
                                    if ui.text_edit_singleline(&mut v).changed() {
                                        pending_props.push(("action_name".to_string(), v));
                                    }
                                }
                            });
                        }
                        BtEditorNodeType::Condition => {
                            ui.horizontal(|ui| {
                                ui.label("Key:");
                                if let Some(val) = node.properties.get("key") {
                                    let mut v = val.clone();
                                    if ui.text_edit_singleline(&mut v).changed() {
                                        pending_props.push(("key".to_string(), v));
                                    }
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label("Expected Value:");
                                if let Some(val) = node.properties.get("expected") {
                                    let mut v = val.clone();
                                    if ui.text_edit_singleline(&mut v).changed() {
                                        pending_props.push(("expected".to_string(), v));
                                    }
                                }
                            });
                        }
                        BtEditorNodeType::Wait => {
                            ui.horizontal(|ui| {
                                ui.label("Duration (s):");
                                if let Some(val) = node.properties.get("duration") {
                                    let mut v: f32 = val.parse().unwrap_or(1.0);
                                    if ui.add(egui::DragValue::new(&mut v).speed(0.1)).changed() {
                                        pending_props.push(("duration".to_string(), v.to_string()));
                                    }
                                }
                            });
                        }
                        BtEditorNodeType::Repeater => {
                            ui.horizontal(|ui| {
                                ui.label("Count:");
                                if let Some(val) = node.properties.get("count") {
                                    let mut v: i32 = val.parse().unwrap_or(-1);
                                    if ui.add(egui::DragValue::new(&mut v).speed(1.0)).changed() {
                                        pending_props.push(("count".to_string(), v.to_string()));
                                    }
                                }
                            });
                        }
                        BtEditorNodeType::Parallel => {
                            ui.horizontal(|ui| {
ui.label("Policy:");
                                let current_policy = node.properties.get("policy").cloned().unwrap_or_else(|| "RequireAll".to_string());
                                let mut new_policy: Option<String> = None;
                                egui::ComboBox::from_id_salt("parallel_policy")
                                    .selected_text(&current_policy)
                                    .show_ui(ui, |ui| {
                                        if ui.selectable_label(current_policy == "RequireAll", "Require All").clicked() {
                                            new_policy = Some("RequireAll".to_string());
                                        }
                                        if ui.selectable_label(current_policy == "RequireOne", "Require One").clicked() {
                                            new_policy = Some("RequireOne".to_string());
                                        }
                                    });
                                if let Some(p) = new_policy {
                                    pending_props.push(("policy".to_string(), p));
                                }
                            });
                        }
                        _ => {}
                    }

                    ui.separator();
                    let mut delete_node = false;
                    if ui.button("\u{1F5D1} Delete Node").clicked() {
                        delete_node = true;
                    }

                    // Apply pending actions
                    if let Some(name) = pending_name {
                        self.trees[idx].graph.update_node_name(selected, &name);
                        self.save_undo_state();
                        self.trees[idx].is_dirty = true;
                    }
                    for (key, value) in pending_props {
                        self.trees[idx].graph.update_node_property(selected, key, value);
                        self.save_undo_state();
                        self.trees[idx].is_dirty = true;
                    }
                    if delete_node {
                        self.trees[idx].graph.remove_node(selected);
                        self.save_undo_state();
                        self.trees[idx].is_dirty = true;
                    }
                } else {
                    ui.label("No node selected");
                }
            } else {
                ui.label("Select a node to edit its properties");
                ui.separator();
                ui.collapsing("Tree Settings", |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Name:");
                        let mut name = self.trees[idx].name.clone();
                        if ui.text_edit_singleline(&mut name).changed() {
                            self.trees[idx].name = name;
                            self.trees[idx].is_dirty = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Description:");
                        let mut desc = self.trees[idx].description.clone();
                        if ui.text_edit_multiline(&mut desc).changed() {
                            self.trees[idx].description = desc;
                            self.trees[idx].is_dirty = true;
                        }
                    });
                });
            }
        } else {
            ui.label("No tree selected");
        }
    }

    fn simulation_panel(&mut self, ui: &mut Ui) {
        let trace_data: Vec<_> = self.simulation.trace().iter().map(|e| (e.tick, e.node_name.clone(), e.status.clone())).collect();
        let bb_snapshot: Vec<_> = self.simulation.blackboard_snapshot().into_iter().map(|(k, _v)| (k.to_string(), String::from("value"))).collect();
        let stats = self.simulation.stats();
        let is_running = self.simulation.is_running();
        let is_paused = self.simulation.is_paused();
        let active_tree_idx = self.active_tree_idx;

        ui.collapsing("\u{25B6} Simulation", |ui| {
            ui.horizontal(|ui| {
                if is_running {
                    if ui.button("\u{23F8} Pause").clicked() {
                        self.simulation.pause();
                    }
                    if ui.button("\u{23F9} Stop").clicked() {
                        self.simulation.stop();
                    }
                } else {
                    if ui.button("\u{25B6} Start").clicked() {
                        if let Some(idx) = active_tree_idx {
                            let tree = &self.trees[idx];
                            self.simulation.load_from_graph(&tree.graph);
                            self.simulation.start();
                        }
                    }
                }
                ui.separator();
                if ui.button("\u{23ED} Step").clicked() {
                    if is_paused {
                        self.simulation.step();
                    }
                }
            });

            // Simulation stats
            ui.horizontal(|ui| {
                ui.label(format!("Tick: {}", stats.tick_count));
                ui.separator();
                ui.label(format!("Status: {:?}", stats.current_status));
                ui.separator();
                ui.label(format!("Active Nodes: {}", stats.active_nodes));
            });

            // Node execution trace
            ui.separator();
            ui.label("Execution Trace:");
            ScrollArea::vertical()
                .max_height(120.0)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for (tick, node_name, status) in &trace_data {
                        let status_str: &str = status.as_ref();
                        let color = match status_str {
                            "Running" => Color32::YELLOW,
                            "Success" => Color32::GREEN,
                            "Failure" => Color32::RED,
                            _ => Color32::LIGHT_GRAY,
                        };
                        ui.colored_label(color, format!("[{}] {} -> {}", tick, node_name, status));
                    }
                });

            // Blackboard display
            ui.separator();
            ui.collapsing("Blackboard", |ui| {
                for (key, value) in &bb_snapshot {
                    ui.label(format!("{}: {}", key, value));
                }
            });
        });
    }

    fn notifications_panel(&mut self, ui: &mut Ui) {
        if self.notifications.is_empty() {
            return;
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new("\u{1F4CB} Notifications").weak());
            if ui.button("Clear").clicked() {
                self.notifications.clear();
            }
        });
        ScrollArea::vertical()
            .max_height(60.0)
            .show(ui, |ui| {
                for (i, msg) in self.notifications.iter().rev().take(5).enumerate() {
                    ui.small(msg);
                }
            });
    }

    fn template_browser_window(&mut self, ctx: &egui::Context) {
        let mut show_templates = self.show_templates;
        let category_filter = self.template_category_filter;

        egui::Window::new("\u{1F4C2} Behavior Templates")
            .default_size([500.0, 400.0])
            .resizable(true)
            .open(&mut show_templates)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Category:");
                    egui::ComboBox::from_id_salt("tpl_cat")
                        .selected_text(
                            category_filter
                                .map(|c| format!("{:?}", c))
                                .unwrap_or_else(|| "All".to_string()),
                        )
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.template_category_filter, None, "All");
                            for cat in &[
                                TemplateCategory::Movement,
                                TemplateCategory::Combat,
                                TemplateCategory::Social,
                                TemplateCategory::Utility,
                                TemplateCategory::Patrol,
                                TemplateCategory::Flee,
                                TemplateCategory::Chase,
                                TemplateCategory::Search,
                            ] {
                                ui.selectable_value(&mut self.template_category_filter, Some(*cat), format!("{:?}", cat));
                            }
                        });
                });
                ui.separator();

                let mut pending_template: Option<&'static BehaviorTemplate> = None;
                ScrollArea::vertical().show(ui, |ui| {
                    for tpl in BehaviorTemplate::TEMPLATES {
                        if let Some(cat) = category_filter {
                            if tpl.category != cat {
                                continue;
                            }
                        }
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(format!("{} {}", tpl.icon, tpl.name));
                                ui.small(tpl.description);
                            });
                            if ui.button("Use Template").clicked() {
                                pending_template = Some(tpl);
                            }
                        });
                    }
                });

                if let Some(tpl) = pending_template {
                    self.create_from_template(tpl);
                    self.show_templates = false;
                    self.add_notification(format!("Created from template: {}", tpl.name));
                }
            });

        if !show_templates {
            self.show_templates = false;
        }
    }
}

impl Default for AiBehaviorEditor {
    fn default() -> Self {
        Self::new()
    }
}
