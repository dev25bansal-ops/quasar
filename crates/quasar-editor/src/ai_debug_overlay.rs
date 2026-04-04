//! AI Debugging Overlay for Quasar Engine.
//!
//! Provides:
//! - **GOAP visualization** — action planning state display
//! - **Utility AI visualization** — consideration scores and curves
//! - **Blackboard inspection** — real-time blackboard value display
//! - **Navigation debug** — path visualization, navmesh overlay
//! - **Perception debug** — sensor ranges, detection states

use egui::{Color32, Pos2, Vec2};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiDebugMode {
    None,
    Goap,
    Utility,
    Blackboard,
    Navigation,
    Perception,
    All,
}

#[derive(Debug, Clone)]
pub struct GoapDebugInfo {
    pub agent_id: u64,
    pub current_goal: Option<String>,
    pub current_plan: Vec<String>,
    pub plan_index: usize,
    pub world_state: HashMap<String, String>,
    pub goal_state: HashMap<String, String>,
    pub planning_time_ms: f64,
    pub plan_cost: f32,
}

impl GoapDebugInfo {
    pub fn new(agent_id: u64) -> Self {
        Self {
            agent_id,
            current_goal: None,
            current_plan: Vec::new(),
            plan_index: 0,
            world_state: HashMap::new(),
            goal_state: HashMap::new(),
            planning_time_ms: 0.0,
            plan_cost: 0.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct UtilityConsiderationDebug {
    pub name: String,
    pub input: f32,
    pub output: f32,
    pub curve_type: String,
    pub weight: f32,
}

#[derive(Debug, Clone)]
pub struct UtilityActionDebug {
    pub name: String,
    pub score: f32,
    pub weighted_score: f32,
    pub considerations: Vec<UtilityConsiderationDebug>,
    pub is_selected: bool,
}

#[derive(Debug, Clone)]
pub struct UtilityDebugInfo {
    pub agent_id: u64,
    pub actions: Vec<UtilityActionDebug>,
    pub selected_action: Option<String>,
    pub decision_time_ms: f64,
}

impl UtilityDebugInfo {
    pub fn new(agent_id: u64) -> Self {
        Self {
            agent_id,
            actions: Vec::new(),
            selected_action: None,
            decision_time_ms: 0.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BlackboardDebugEntry {
    pub key: String,
    pub value: String,
    pub type_name: String,
    pub changed_this_frame: bool,
}

#[derive(Debug, Clone)]
pub struct BlackboardDebugInfo {
    pub entity_id: u64,
    pub entries: Vec<BlackboardDebugEntry>,
}

#[derive(Debug, Clone)]
pub struct NavPathDebug {
    pub path_id: u64,
    pub agent_id: u64,
    pub waypoints: Vec<[f32; 3]>,
    pub current_index: usize,
    pub path_status: String,
}

#[derive(Debug, Clone)]
pub struct NavMeshDebug {
    pub tiles: Vec<NavTileDebug>,
    pub agents: Vec<NavAgentDebug>,
}

#[derive(Debug, Clone)]
pub struct NavTileDebug {
    pub position: [f32; 3],
    pub size: f32,
    pub area_type: u8,
    pub walkable: bool,
}

#[derive(Debug, Clone)]
pub struct NavAgentDebug {
    pub agent_id: u64,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub target: Option<[f32; 3]>,
    pub path_status: String,
}

#[derive(Debug, Clone)]
pub struct PerceptionDebug {
    pub entity_id: u64,
    pub position: [f32; 3],
    pub sight_range: f32,
    pub sight_angle: f32,
    pub hearing_range: f32,
    pub detected_entities: Vec<u64>,
    pub last_known_positions: HashMap<u64, [f32; 3]>,
}

pub struct AiDebugOverlay {
    pub mode: AiDebugMode,
    pub enabled: bool,
    pub selected_agent: Option<u64>,
    pub goap_info: HashMap<u64, GoapDebugInfo>,
    pub utility_info: HashMap<u64, UtilityDebugInfo>,
    pub blackboard_info: HashMap<u64, BlackboardDebugInfo>,
    pub nav_paths: Vec<NavPathDebug>,
    pub nav_mesh: Option<NavMeshDebug>,
    pub perception_info: Vec<PerceptionDebug>,
    pub show_world_overlay: bool,
    pub show_score_graphs: bool,
    pub paused: bool,
    pub frame_history: Vec<AiFrameSnapshot>,
    pub max_history_frames: usize,
}

#[derive(Debug, Clone)]
pub struct AiFrameSnapshot {
    pub frame: u64,
    pub timestamp_ms: f64,
    pub goap_planning_count: usize,
    pub utility_decisions_count: usize,
    pub total_ai_time_ms: f64,
}

impl AiDebugOverlay {
    pub fn new() -> Self {
        Self {
            mode: AiDebugMode::None,
            enabled: false,
            selected_agent: None,
            goap_info: HashMap::new(),
            utility_info: HashMap::new(),
            blackboard_info: HashMap::new(),
            nav_paths: Vec::new(),
            nav_mesh: None,
            perception_info: Vec::new(),
            show_world_overlay: true,
            show_score_graphs: true,
            paused: false,
            frame_history: Vec::new(),
            max_history_frames: 300,
        }
    }

    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
    }

    pub fn cycle_mode(&mut self) {
        self.mode = match self.mode {
            AiDebugMode::None => AiDebugMode::Goap,
            AiDebugMode::Goap => AiDebugMode::Utility,
            AiDebugMode::Utility => AiDebugMode::Blackboard,
            AiDebugMode::Blackboard => AiDebugMode::Navigation,
            AiDebugMode::Navigation => AiDebugMode::Perception,
            AiDebugMode::Perception => AiDebugMode::All,
            AiDebugMode::All => AiDebugMode::None,
        };
    }

    pub fn clear(&mut self) {
        self.goap_info.clear();
        self.utility_info.clear();
        self.blackboard_info.clear();
        self.nav_paths.clear();
        self.nav_mesh = None;
        self.perception_info.clear();
    }

    pub fn update_goap(&mut self, info: GoapDebugInfo) {
        self.goap_info.insert(info.agent_id, info);
    }

    pub fn update_utility(&mut self, info: UtilityDebugInfo) {
        self.utility_info.insert(info.agent_id, info);
    }

    pub fn update_blackboard(&mut self, info: BlackboardDebugInfo) {
        self.blackboard_info.insert(info.entity_id, info);
    }

    pub fn update_nav_path(&mut self, path: NavPathDebug) {
        self.nav_paths.retain(|p| p.path_id != path.path_id);
        self.nav_paths.push(path);
    }

    pub fn update_nav_mesh(&mut self, mesh: NavMeshDebug) {
        self.nav_mesh = Some(mesh);
    }

    pub fn update_perception(&mut self, info: PerceptionDebug) {
        self.perception_info
            .retain(|p| p.entity_id != info.entity_id);
        self.perception_info.push(info);
    }

    pub fn record_frame(&mut self, frame: u64, timestamp_ms: f64) {
        let goap_count = self.goap_info.len();
        let utility_count = self.utility_info.len();
        let goap_time: f64 = self.goap_info.values().map(|i| i.planning_time_ms).sum();
        let utility_time: f64 = self.utility_info.values().map(|i| i.decision_time_ms).sum();

        let snapshot = AiFrameSnapshot {
            frame,
            timestamp_ms,
            goap_planning_count: goap_count,
            utility_decisions_count: utility_count,
            total_ai_time_ms: goap_time + utility_time,
        };

        self.frame_history.push(snapshot);
        if self.frame_history.len() > self.max_history_frames {
            self.frame_history.remove(0);
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.enabled {
            return;
        }

        egui::Window::new("AI Debug Overlay")
            .default_size([350.0, 500.0])
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Mode:");
                    egui::ComboBox::from_id_salt("ai_debug_mode")
                        .selected_text(format!("{:?}", self.mode))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.mode, AiDebugMode::None, "None");
                            ui.selectable_value(&mut self.mode, AiDebugMode::Goap, "GOAP");
                            ui.selectable_value(&mut self.mode, AiDebugMode::Utility, "Utility");
                            ui.selectable_value(
                                &mut self.mode,
                                AiDebugMode::Blackboard,
                                "Blackboard",
                            );
                            ui.selectable_value(
                                &mut self.mode,
                                AiDebugMode::Navigation,
                                "Navigation",
                            );
                            ui.selectable_value(
                                &mut self.mode,
                                AiDebugMode::Perception,
                                "Perception",
                            );
                            ui.selectable_value(&mut self.mode, AiDebugMode::All, "All");
                        });
                    ui.toggle_value(&mut self.paused, "Pause");
                    if ui.button("Clear").clicked() {
                        self.clear();
                    }
                });

                ui.separator();

                ui.checkbox(&mut self.show_world_overlay, "Show World Overlay");
                ui.checkbox(&mut self.show_score_graphs, "Show Score Graphs");

                ui.separator();

                let agent_ids: Vec<u64> = self
                    .goap_info
                    .keys()
                    .chain(self.utility_info.keys())
                    .chain(self.blackboard_info.keys())
                    .copied()
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .collect();

                ui.horizontal(|ui| {
                    ui.label("Agent:");
                    egui::ComboBox::from_id_salt("agent_select")
                        .selected_text(
                            self.selected_agent
                                .map(|id| format!("Agent {}", id))
                                .unwrap_or_else(|| "All".to_string()),
                        )
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.selected_agent, None, "All");
                            for id in &agent_ids {
                                ui.selectable_value(
                                    &mut self.selected_agent,
                                    Some(*id),
                                    format!("Agent {}", id),
                                );
                            }
                        });
                });

                ui.separator();

                egui::ScrollArea::vertical().show(ui, |ui| {
                    if self.mode == AiDebugMode::Goap || self.mode == AiDebugMode::All {
                        self.show_goap_panel(ui);
                    }

                    if self.mode == AiDebugMode::Utility || self.mode == AiDebugMode::All {
                        self.show_utility_panel(ui);
                    }

                    if self.mode == AiDebugMode::Blackboard || self.mode == AiDebugMode::All {
                        self.show_blackboard_panel(ui);
                    }

                    if self.mode == AiDebugMode::Navigation || self.mode == AiDebugMode::All {
                        self.show_navigation_panel(ui);
                    }

                    if self.mode == AiDebugMode::Perception || self.mode == AiDebugMode::All {
                        self.show_perception_panel(ui);
                    }

                    if self.mode != AiDebugMode::None {
                        self.show_performance_panel(ui);
                    }
                });
            });
    }

    fn show_goap_panel(&self, ui: &mut egui::Ui) {
        ui.collapsing("GOAP Planning", |ui| {
            let info_iter: Vec<_> = if let Some(id) = self.selected_agent {
                self.goap_info.get(&id).into_iter().collect()
            } else {
                self.goap_info.values().collect()
            };

            if info_iter.is_empty() {
                ui.label("No GOAP agents active");
                return;
            }

            for info in info_iter {
                ui.group(|ui| {
                    ui.label(format!("Agent {}", info.agent_id));

                    ui.horizontal(|ui| {
                        ui.label("Goal:");
                        ui.label(info.current_goal.as_deref().unwrap_or("(none)"));
                    });

                    ui.label("Plan:");
                    if info.current_plan.is_empty() {
                        ui.label("  (no plan)");
                    } else {
                        for (i, action) in info.current_plan.iter().enumerate() {
                            let prefix = if i == info.plan_index {
                                "▶ "
                            } else if i < info.plan_index {
                                "✓ "
                            } else {
                                "  "
                            };
                            ui.label(format!("{}{}. {}", prefix, i + 1, action));
                        }
                    }

                    ui.label(format!("Plan Cost: {:.2}", info.plan_cost));
                    ui.label(format!("Planning: {:.2} ms", info.planning_time_ms));

                    ui.collapsing("World State", |ui| {
                        for (k, v) in &info.world_state {
                            ui.label(format!("{}: {}", k, v));
                        }
                    });

                    ui.collapsing("Goal State", |ui| {
                        for (k, v) in &info.goal_state {
                            let has = info.world_state.get(k) == Some(v);
                            let color = if has { Color32::GREEN } else { Color32::RED };
                            ui.colored_label(color, format!("{}: {}", k, v));
                        }
                    });
                });
            }
        });
    }

    fn show_utility_panel(&self, ui: &mut egui::Ui) {
        ui.collapsing("Utility AI", |ui| {
            let info_iter: Vec<_> = if let Some(id) = self.selected_agent {
                self.utility_info.get(&id).into_iter().collect()
            } else {
                self.utility_info.values().collect()
            };

            if info_iter.is_empty() {
                ui.label("No Utility AI agents active");
                return;
            }

            for info in info_iter {
                ui.group(|ui| {
                    ui.label(format!("Agent {}", info.agent_id));
                    ui.label(format!("Decision Time: {:.2} ms", info.decision_time_ms));

                    let mut actions = info.actions.clone();
                    actions.sort_by(|a, b| {
                        b.weighted_score
                            .partial_cmp(&a.weighted_score)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });

                    ui.label("Action Scores:");
                    for action in &actions {
                        let selected = action.is_selected
                            || info.selected_action.as_deref() == Some(&action.name);
                        let color = if selected {
                            Color32::GREEN
                        } else {
                            Color32::LIGHT_GRAY
                        };

                        ui.horizontal(|ui| {
                            ui.colored_label(
                                color,
                                format!("{}: {:.3}", action.name, action.weighted_score),
                            );
                        });

                        if self.show_score_graphs {
                            self.show_score_bar(ui, action.weighted_score);
                        }

                        if ui
                            .collapsing(format!("Considerations ({})", action.considerations.len()))
                            .inner
                            .open
                        {
                            for cons in &action.considerations {
                                ui.horizontal(|ui| {
                                    ui.label(format!("  {}: {:.3}", cons.name, cons.output));
                                    ui.small(format!("({} -> {:.2})", cons.input, cons.output));
                                });
                            }
                        }
                    }
                });
            }
        });
    }

    fn show_score_bar(&self, ui: &mut egui::Ui, score: f32) {
        let bar_width = 100.0;
        let bar_height = 10.0;
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(bar_width, bar_height), egui::Sense::hover());
        let painter = ui.painter_at(rect);

        painter.rect_filled(rect, 2.0, Color32::from_gray(40));

        let fill_width = (score.clamp(0.0, 1.0) * bar_width).max(1.0);
        let fill_rect = egui::Rect::from_min_size(rect.min, egui::vec2(fill_width, bar_height));

        let color = if score > 0.7 {
            Color32::GREEN
        } else if score > 0.4 {
            Color32::YELLOW
        } else {
            Color32::RED
        };

        painter.rect_filled(fill_rect, 2.0, color);
    }

    fn show_blackboard_panel(&self, ui: &mut egui::Ui) {
        ui.collapsing("Blackboard", |ui| {
            let info_iter: Vec<_> = if let Some(id) = self.selected_agent {
                self.blackboard_info.get(&id).into_iter().collect()
            } else {
                self.blackboard_info.values().collect()
            };

            if info_iter.is_empty() {
                ui.label("No blackboard data available");
                return;
            }

            for info in info_iter {
                ui.group(|ui| {
                    ui.label(format!("Entity {}", info.entity_id));

                    for entry in &info.entries {
                        let color = if entry.changed_this_frame {
                            Color32::YELLOW
                        } else {
                            Color32::WHITE
                        };
                        ui.horizontal(|ui| {
                            ui.colored_label(color, &entry.key);
                            ui.label(":");
                            ui.colored_label(color, &entry.value);
                            ui.small(&entry.type_name);
                        });
                    }
                });
            }
        });
    }

    fn show_navigation_panel(&self, ui: &mut egui::Ui) {
        ui.collapsing("Navigation", |ui| {
            ui.label(format!("Active Paths: {}", self.nav_paths.len()));

            for path in &self.nav_paths {
                ui.group(|ui| {
                    ui.label(format!("Path {} (Agent {})", path.path_id, path.agent_id));
                    ui.label(format!("Status: {}", path.path_status));
                    ui.label(format!(
                        "Progress: {}/{}",
                        path.current_index,
                        path.waypoints.len()
                    ));

                    if path.waypoints.is_empty() {
                        ui.label("  (no waypoints)");
                    } else {
                        for (i, wp) in path.waypoints.iter().enumerate() {
                            let prefix = if i == path.current_index {
                                "▶"
                            } else if i < path.current_index {
                                "✓"
                            } else {
                                " "
                            };
                            ui.label(format!(
                                "  {} WP{}: ({:.1}, {:.1}, {:.1})",
                                prefix, i, wp[0], wp[1], wp[2]
                            ));
                        }
                    }
                });
            }

            if let Some(mesh) = &self.nav_mesh {
                ui.separator();
                ui.label(format!("NavMesh Tiles: {}", mesh.tiles.len()));
                ui.label(format!("NavMesh Agents: {}", mesh.agents.len()));

                for agent in &mesh.agents {
                    ui.group(|ui| {
                        ui.label(format!("Agent {}", agent.agent_id));
                        ui.label(format!(
                            "Position: ({:.1}, {:.1}, {:.1})",
                            agent.position[0], agent.position[1], agent.position[2]
                        ));
                        if let Some(target) = agent.target {
                            ui.label(format!(
                                "Target: ({:.1}, {:.1}, {:.1})",
                                target[0], target[1], target[2]
                            ));
                        }
                        ui.label(format!("Status: {}", agent.path_status));
                    });
                }
            }
        });
    }

    fn show_perception_panel(&self, ui: &mut egui::Ui) {
        ui.collapsing("Perception", |ui| {
            ui.label(format!("Entities: {}", self.perception_info.len()));

            for info in &self.perception_info {
                let is_selected = self.selected_agent == Some(info.entity_id);
                if self.selected_agent.is_some() && !is_selected {
                    continue;
                }

                ui.group(|ui| {
                    ui.label(format!("Entity {}", info.entity_id));
                    ui.label(format!(
                        "Position: ({:.1}, {:.1}, {:.1})",
                        info.position[0], info.position[1], info.position[2]
                    ));
                    ui.label(format!("Sight Range: {:.1}", info.sight_range));
                    ui.label(format!(
                        "Sight Angle: {:.0}°",
                        info.sight_angle.to_degrees()
                    ));
                    ui.label(format!("Hearing Range: {:.1}", info.hearing_range));
                    ui.label(format!(
                        "Detected: {} entities",
                        info.detected_entities.len()
                    ));

                    if !info.detected_entities.is_empty() {
                        ui.collapsing("Detected Entities", |ui| {
                            for &id in &info.detected_entities {
                                ui.label(format!("  Entity {}", id));
                                if let Some(pos) = info.last_known_positions.get(&id) {
                                    ui.label(format!(
                                        "    Last Known: ({:.1}, {:.1}, {:.1})",
                                        pos[0], pos[1], pos[2]
                                    ));
                                }
                            }
                        });
                    }
                });
            }
        });
    }

    fn show_performance_panel(&self, ui: &mut egui::Ui) {
        ui.collapsing("Performance", |ui| {
            if self.frame_history.is_empty() {
                ui.label("No performance data yet");
                return;
            }

            let recent: Vec<_> = self.frame_history.iter().rev().take(60).collect();
            let avg_time: f64 =
                recent.iter().map(|s| s.total_ai_time_ms).sum::<f64>() / recent.len().max(1) as f64;
            let max_time: f64 = recent
                .iter()
                .map(|s| s.total_ai_time_ms)
                .fold(0.0, f64::max);

            ui.label(format!("Avg AI Time: {:.2} ms", avg_time));
            ui.label(format!("Max AI Time: {:.2} ms", max_time));
            ui.label(format!(
                "GOAP Agents: {}",
                recent.first().map(|s| s.goap_planning_count).unwrap_or(0)
            ));
            ui.label(format!(
                "Utility Agents: {}",
                recent
                    .first()
                    .map(|s| s.utility_decisions_count)
                    .unwrap_or(0)
            ));

            let graph_height = 40.0;
            let graph_width = ui.available_width().max(200.0);
            let (rect, _) =
                ui.allocate_exact_size(egui::vec2(graph_width, graph_height), egui::Sense::hover());
            let painter = ui.painter_at(rect);

            painter.rect_filled(rect, 2.0, Color32::from_gray(30));

            let max_graph_time = (max_time * 1.2).max(1.0);
            for (i, snapshot) in recent.iter().enumerate() {
                let x = rect.min.x + (i as f32 / recent.len().max(1) as f32) * graph_width;
                let h = (snapshot.total_ai_time_ms / max_graph_time * graph_height as f64) as f32;
                let color = if snapshot.total_ai_time_ms > 16.0 {
                    Color32::RED
                } else if snapshot.total_ai_time_ms > 8.0 {
                    Color32::YELLOW
                } else {
                    Color32::GREEN
                };
                painter.line_segment(
                    [egui::pos2(x, rect.max.y), egui::pos2(x, rect.max.y - h)],
                    egui::Stroke::new(1.0, color),
                );
            }
        });
    }
}

impl Default for AiDebugOverlay {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ai_debug_overlay_creation() {
        let overlay = AiDebugOverlay::new();
        assert!(!overlay.enabled);
        assert_eq!(overlay.mode, AiDebugMode::None);
    }

    #[test]
    fn ai_debug_toggle() {
        let mut overlay = AiDebugOverlay::new();
        overlay.toggle();
        assert!(overlay.enabled);
    }

    #[test]
    fn ai_debug_cycle_mode() {
        let mut overlay = AiDebugOverlay::new();
        overlay.cycle_mode();
        assert_eq!(overlay.mode, AiDebugMode::Goap);
        overlay.cycle_mode();
        assert_eq!(overlay.mode, AiDebugMode::Utility);
    }

    #[test]
    fn goap_debug_info() {
        let mut info = GoapDebugInfo::new(1);
        info.current_goal = Some("Attack".to_string());
        info.current_plan.push("FindTarget");
        info.current_plan.push("Approach");
        info.current_plan.push("Attack");

        assert_eq!(info.agent_id, 1);
        assert_eq!(info.current_plan.len(), 3);
    }

    #[test]
    fn utility_debug_info() {
        let mut info = UtilityDebugInfo::new(1);
        info.actions.push(UtilityActionDebug {
            name: "Idle".to_string(),
            score: 0.5,
            weighted_score: 0.5,
            considerations: vec![UtilityConsiderationDebug {
                name: "Health".to_string(),
                input: 1.0,
                output: 1.0,
                curve_type: "Linear".to_string(),
                weight: 1.0,
            }],
            is_selected: true,
        });

        assert_eq!(info.actions.len(), 1);
    }

    #[test]
    fn blackboard_debug() {
        let mut info = BlackboardDebugInfo {
            entity_id: 1,
            entries: vec![BlackboardDebugEntry {
                key: "health".to_string(),
                value: "100".to_string(),
                type_name: "f32".to_string(),
                changed_this_frame: false,
            }],
        };

        assert_eq!(info.entries.len(), 1);
    }

    #[test]
    fn frame_snapshot_recording() {
        let mut overlay = AiDebugOverlay::new();
        overlay.update_goap(GoapDebugInfo::new(1));
        overlay.record_frame(1, 16.67);

        assert_eq!(overlay.frame_history.len(), 1);
    }
}
