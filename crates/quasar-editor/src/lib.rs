//! # Quasar Editor
//!
//! Visual scene editor built with [`egui`].
//!
//! Provides a runtime GUI overlay for inspecting entities, viewing logs,
//! and tweaking component values — press F12 to toggle.

pub mod asset_browser;
pub mod console;
pub mod editor_state;
pub mod gizmos;
pub mod hierarchy;
pub mod inspector;
pub mod renderer;
pub mod shader_graph_editor;

pub use asset_browser::{AssetBrowser, AssetEntry, AssetKind};
pub use shader_graph_editor::ShaderGraphEditor;
pub use editor_state::{
    EditCommand, EditorMode, EditorState, SetMaterialCommand, SetPositionCommand,
    SetRotationCommand, SetScaleCommand, UndoStack, WorldSnapshot,
};
pub use gizmos::{GizmoAxis, GizmoMode, GizmoRenderer, GizmoState};
pub use inspector::{InspectorAction, InspectorData};
use quasar_core::ecs::Entity;

/// Editor actions that require world access
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorAction {
    Play,
    Stop,
    Undo,
    Redo,
    StepFrame,
}

/// Editor state — tracks visible panels and the selected entity.
pub struct Editor {
    /// Master toggle — when false, no editor UI is drawn.
    pub enabled: bool,
    /// Show the scene hierarchy panel.
    pub show_hierarchy: bool,
    /// Show the inspector/property panel.
    pub show_inspector: bool,
    /// Show the debug console / log panel.
    pub show_console: bool,
    /// Show performance metrics overlay.
    pub show_metrics: bool,
    /// Show the asset browser panel.
    pub show_asset_browser: bool,
    /// Show the shader graph editor panel.
    pub show_shader_graph: bool,
    /// Shader graph editor state.
    pub shader_graph_editor: ShaderGraphEditor,
    /// The currently selected entity (if any).
    pub selected_entity: Option<Entity>,
    /// Console log buffer.
    pub console: console::ConsoleLog,
    /// Editor state for Play/Pause/Stop and undo/redo
    pub state: EditorState,
    /// Asset browser panel.
    pub asset_browser: AssetBrowser,
    /// Show the Lua REPL panel.
    pub show_lua_repl: bool,
    /// Lua REPL input buffer.
    pub lua_repl_input: String,
    /// Lua REPL output history.
    pub lua_repl_output: Vec<String>,
    /// Profiler frame times ring buffer (last N frames).
    pub profiler_frame_times: std::collections::VecDeque<f32>,
    /// Maximum frame times to store.
    pub profiler_max_frames: usize,
    /// Pending Lua expression from the REPL to be evaluated by the runner.
    pub pending_lua_eval: Option<String>,
}

impl Editor {
    pub fn new() -> Self {
        Self {
            enabled: false,
            show_hierarchy: true,
            show_inspector: true,
            show_console: false,
            show_metrics: true,
            show_asset_browser: false,
            show_shader_graph: false,
            shader_graph_editor: ShaderGraphEditor::new(),
            selected_entity: None,
            console: console::ConsoleLog::new(),
            state: EditorState::new(),
            asset_browser: AssetBrowser::new("assets"),
            show_lua_repl: false,
            lua_repl_input: String::new(),
            lua_repl_output: Vec::new(),
            profiler_frame_times: std::collections::VecDeque::with_capacity(240),
            profiler_max_frames: 240,
            pending_lua_eval: None,
        }
    }

    /// Toggle the editor overlay on/off.
    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
    }

    /// Render the full editor UI. Call this from your egui integration each frame.
    ///
    /// `inspector_data` should be `Some` when an entity is selected and the
    /// caller has read its components. Edited values are written in-place;
    /// the function returns `(bool, Option<InspectorAction>, Option<EditorAction>)` where:
    /// - `bool` indicates if anything was changed so the caller can write back to ECS.
    /// - `Option<InspectorAction>` contains any action requested (despawn/spawn).
    /// - `Option<EditorAction>` contains editor actions like Play/Pause/Stop.
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        entity_names: &[(Entity, String)],
        inspector_data: Option<&mut InspectorData>,
    ) -> (bool, Option<InspectorAction>, Option<EditorAction>) {
        if !self.enabled {
            return (false, None, None);
        }

        let mut editor_action = None;

        // Top menu bar
        egui::TopBottomPanel::top("editor_menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.label("🚀 Quasar Editor");
                ui.separator();
                ui.toggle_value(&mut self.show_hierarchy, "📋 Hierarchy");
                ui.toggle_value(&mut self.show_inspector, "🔍 Inspector");
                ui.toggle_value(&mut self.show_console, "📝 Console");
                ui.toggle_value(&mut self.show_metrics, "📊 Metrics");
                ui.toggle_value(&mut self.show_asset_browser, "📁 Assets");
                ui.toggle_value(&mut self.show_shader_graph, "🔗 Shader Graph");
                ui.toggle_value(&mut self.show_lua_repl, "🖥 Lua REPL");
                ui.separator();

                // Play/Pause/Stop buttons
                let play_label = match self.state.mode {
                    EditorMode::Stopped => "▶ Play",
                    EditorMode::Playing => "⏸ Pause",
                    EditorMode::Paused => "▶ Resume",
                };
                if ui.button(play_label).clicked() {
                    match self.state.mode {
                        EditorMode::Stopped => {
                            editor_action = Some(EditorAction::Play);
                        }
                        EditorMode::Playing => {
                            self.state.mode = EditorMode::Paused;
                        }
                        EditorMode::Paused => {
                            self.state.mode = EditorMode::Playing;
                        }
                    }
                }
                if ui.button("⏹ Stop").clicked() && self.state.mode != EditorMode::Stopped {
                    editor_action = Some(EditorAction::Stop);
                }
                if ui
                    .add_enabled(
                        self.state.mode == EditorMode::Paused,
                        egui::Button::new("⏭ Step"),
                    )
                    .clicked()
                {
                    editor_action = Some(EditorAction::StepFrame);
                }

                ui.separator();

                // Undo/Redo buttons
                if ui
                    .add_enabled(
                        self.state.undo_stack.can_undo(),
                        egui::Button::new("↶ Undo"),
                    )
                    .clicked()
                {
                    editor_action = Some(EditorAction::Undo);
                }
                if ui
                    .add_enabled(
                        self.state.undo_stack.can_redo(),
                        egui::Button::new("↷ Redo"),
                    )
                    .clicked()
                {
                    editor_action = Some(EditorAction::Redo);
                }
            });
        });

        // Hierarchy panel
        if self.show_hierarchy {
            hierarchy::hierarchy_panel(ctx, &mut self.selected_entity, entity_names);
        }

        // Inspector panel
        let mut inspector_changed = false;
        let mut inspector_action = None;
        if self.show_inspector {
            let (changed, action) =
                inspector::inspector_panel(ctx, self.selected_entity, inspector_data);
            inspector_changed = changed;
            inspector_action = action;
        }

        // Console panel
        if self.show_console {
            self.console.panel(ctx);
        }

        // Metrics overlay with profiler frame graph
        if self.show_metrics {
            egui::Window::new("📊 Metrics")
                .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-10.0, 40.0))
                .resizable(false)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label(format!("Entities: {}", entity_names.len()));
                    ui.label(format!("Mode: {:?}", self.state.mode));

                    // Frame time graph
                    if !self.profiler_frame_times.is_empty() {
                        let avg: f32 =
                            self.profiler_frame_times.iter().sum::<f32>()
                                / self.profiler_frame_times.len() as f32;
                        let max_ft = self
                            .profiler_frame_times
                            .iter()
                            .cloned()
                            .fold(0.0f32, f32::max);
                        ui.label(format!("FPS: {:.0}", 1.0 / avg.max(0.0001)));
                        ui.label(format!(
                            "Frame: {:.2} ms (max {:.2} ms)",
                            avg * 1000.0,
                            max_ft * 1000.0
                        ));

                        // Simple bar-graph of recent frame times
                        let graph_height = 60.0;
                        let bar_width = 1.5;
                        let num_bars = self.profiler_frame_times.len();
                        let desired_width = num_bars as f32 * bar_width;
                        let (rect, _resp) = ui.allocate_exact_size(
                            egui::vec2(desired_width.max(100.0), graph_height),
                            egui::Sense::hover(),
                        );
                        let painter = ui.painter_at(rect);
                        painter.rect_filled(rect, 2.0, egui::Color32::from_rgb(20, 20, 25));
                        let scale = graph_height / (max_ft * 1000.0).max(1.0);
                        for (i, &ft) in self.profiler_frame_times.iter().enumerate() {
                            let ms = ft * 1000.0;
                            let h = ms * scale;
                            let x = rect.min.x + i as f32 * bar_width;
                            let color = if ms > 33.3 {
                                egui::Color32::from_rgb(255, 80, 80)
                            } else if ms > 16.6 {
                                egui::Color32::YELLOW
                            } else {
                                egui::Color32::from_rgb(80, 200, 80)
                            };
                            painter.rect_filled(
                                egui::Rect::from_min_max(
                                    egui::pos2(x, rect.max.y - h),
                                    egui::pos2(x + bar_width, rect.max.y),
                                ),
                                0.0,
                                color,
                            );
                        }
                    }

                    ui.separator();
                    ui.label("F12: toggle editor");
                    ui.label("Ctrl+Z/Y: Undo/Redo");
                });
        }

        // Lua REPL panel
        if self.show_lua_repl {
            egui::Window::new("🖥 Lua REPL")
                .default_size([400.0, 300.0])
                .resizable(true)
                .show(ctx, |ui| {
                    // Output history
                    egui::ScrollArea::vertical()
                        .max_height(200.0)
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            for line in &self.lua_repl_output {
                                ui.monospace(line);
                            }
                        });
                    ui.separator();
                    // Input line
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.lua_repl_input)
                            .desired_width(f32::INFINITY)
                            .font(egui::TextStyle::Monospace)
                            .hint_text("Lua expression…"),
                    );
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        if !self.lua_repl_input.is_empty() {
                            let cmd = self.lua_repl_input.clone();
                            self.lua_repl_output.push(format!("> {}", cmd));
                            self.lua_repl_input.clear();
                            // The actual eval is done by the runner via `pending_lua_eval`.
                            self.pending_lua_eval = Some(cmd);
                        }
                        response.request_focus();
                    }
                });
        }

        // Asset browser panel
        if self.show_asset_browser {
            let _drag_path = self.asset_browser.panel(ctx);
        }

        (inspector_changed, inspector_action, editor_action)
    }

    /// Handle editor action that requires world access
    pub fn handle_action(&mut self, action: EditorAction, world: &mut quasar_core::ecs::World) {
        match action {
            EditorAction::Play => {
                self.state.play(world);
            }
            EditorAction::Stop => {
                self.state.stop(world);
            }
            EditorAction::Undo => {
                if let Some(desc) = self.state.undo_stack.undo(world) {
                    log::info!("Undo: {}", desc);
                }
            }
            EditorAction::Redo => {
                if let Some(desc) = self.state.undo_stack.redo(world) {
                    log::info!("Redo: {}", desc);
                }
            }
            EditorAction::StepFrame => {
                self.state.step_frame();
            }
        }
    }

    /// Record a frame time for the profiler overlay.
    pub fn push_frame_time(&mut self, dt: f32) {
        if self.profiler_frame_times.len() >= self.profiler_max_frames {
            self.profiler_frame_times.pop_front();
        }
        self.profiler_frame_times.push_back(dt);
    }

    /// Push a Lua REPL result string into the output history.
    pub fn push_lua_result(&mut self, result: &str) {
        self.lua_repl_output.push(result.to_string());
    }

    /// Take a pending Lua eval command (if any).
    pub fn take_pending_lua_eval(&mut self) -> Option<String> {
        self.pending_lua_eval.take()
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}
