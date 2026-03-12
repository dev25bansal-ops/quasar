//! # Quasar Editor
//!
//! Visual scene editor built with [`egui`].
//!
//! Provides a runtime GUI overlay for inspecting entities, viewing logs,
//! and tweaking component values — press F12 to toggle.

#![deny(clippy::unwrap_used, clippy::expect_used)]

pub mod asset_browser;
pub mod console;
pub mod editor_state;
pub mod gizmos;
pub mod hierarchy;
pub mod inspector;
pub mod inspector_commands;
pub mod logic_graph;
pub mod logic_graph_editor;
pub mod reflect;
pub mod renderer;
pub mod shader_graph_editor;

pub use asset_browser::{AssetBrowser, AssetEntry, AssetKind};
pub use editor_state::{
    DeleteEntityCommand, EditCommand, EditorMode, EditorState, SetMaterialCommand,
    SetPositionCommand, SetRotationCommand, SetScaleCommand, SpawnEntityCommand, TransformData,
    UndoStack, WorldSnapshot,
};
pub use gizmos::{GizmoAxis, GizmoMode, GizmoRenderer, GizmoState};
pub use inspector::{InspectorAction, InspectorData};
pub use logic_graph::{LogicGraph, LogicGraphCompiler, LogicNodeKind};
pub use logic_graph_editor::LogicGraphEditorState;
use quasar_core::ecs::Entity;
pub use quasar_derive::Inspect as DeriveInspect;
pub use reflect::{
    widget_bool, widget_color3, widget_color4, widget_f32, widget_f64, widget_i32, widget_string,
    widget_u32, widget_vec3, FieldDescriptor, FieldMeta, Inspect, InspectFn, ReflectionRegistry,
};
pub use shader_graph_editor::ShaderGraphEditor;

// ---------------------------------------------------------------------------
// Animation Timeline Panel
// ---------------------------------------------------------------------------

/// Interpolation mode for a keyframe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterpolationMode {
    Step,
    Linear,
    CubicSpline,
}

impl Default for InterpolationMode {
    fn default() -> Self {
        Self::Linear
    }
}

/// A channel (track lane) in the timeline.
#[derive(Debug, Clone)]
pub struct TimelineChannel {
    pub name: String,
    /// Keyframe times in seconds.
    pub keyframe_times: Vec<f32>,
    /// Keyframe values (parallel to `keyframe_times`).
    pub keyframe_values: Vec<f32>,
    /// Interpolation mode per keyframe (parallel to `keyframe_times`).
    pub interp_modes: Vec<InterpolationMode>,
}

/// State for the animation timeline editor panel.
pub struct TimelinePanel {
    /// Current scrub position in seconds.
    pub scrub_time: f32,
    /// Horizontal zoom (pixels per second).
    pub zoom: f32,
    /// Whether playback is active.
    pub playing: bool,
    /// Channels to display.
    pub channels: Vec<TimelineChannel>,
    /// Horizontal scroll offset in seconds.
    pub scroll_offset: f32,
    /// Keyframe currently being dragged: (channel_idx, keyframe_idx).
    pub dragging_keyframe: Option<(usize, usize)>,
    /// Keyframe currently selected for value editing: (channel_idx, keyframe_idx).
    pub selected_keyframe: Option<(usize, usize)>,
}

impl TimelinePanel {
    pub fn new() -> Self {
        Self {
            scrub_time: 0.0,
            zoom: 100.0,
            playing: false,
            channels: Vec::new(),
            scroll_offset: 0.0,
            dragging_keyframe: None,
            selected_keyframe: None,
        }
    }

    /// Render the timeline panel using egui.
    pub fn ui(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("timeline_panel")
            .default_height(180.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("\u{1F3AC} Timeline");
                    ui.separator();
                    if ui
                        .button(if self.playing {
                            "\u{23F8} Pause"
                        } else {
                            "\u{25B6} Play"
                        })
                        .clicked()
                    {
                        self.playing = !self.playing;
                    }
                    if ui.button("\u{23F9} Stop").clicked() {
                        self.playing = false;
                        self.scrub_time = 0.0;
                    }
                    ui.separator();
                    ui.add(
                        egui::DragValue::new(&mut self.scrub_time)
                            .speed(0.01)
                            .prefix("Time: ")
                            .suffix(" s"),
                    );
                    ui.separator();
                    ui.add(egui::Slider::new(&mut self.zoom, 20.0..=500.0).text("Zoom"));
                });
                ui.separator();

                let track_height = 24.0;
                let total_height = self.channels.len() as f32 * track_height;
                let available = ui.available_size();

                egui::ScrollArea::both()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        let (rect, response) = ui.allocate_exact_size(
                            egui::vec2(
                                available.x.max(self.zoom * 10.0),
                                total_height.max(available.y),
                            ),
                            egui::Sense::click_and_drag(),
                        );
                        let painter = ui.painter_at(rect);

                        // Background
                        painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(30, 30, 35));

                        // Draw channels
                        for (i, ch) in self.channels.iter().enumerate() {
                            let y = rect.min.y + i as f32 * track_height;
                            // Channel label
                            painter.text(
                                egui::pos2(rect.min.x + 4.0, y + 4.0),
                                egui::Align2::LEFT_TOP,
                                &ch.name,
                                egui::FontId::monospace(11.0),
                                egui::Color32::LIGHT_GRAY,
                            );
                            // Row separator
                            painter.line_segment(
                                [
                                    egui::pos2(rect.min.x, y + track_height),
                                    egui::pos2(rect.max.x, y + track_height),
                                ],
                                egui::Stroke::new(0.5, egui::Color32::from_gray(60)),
                            );
                            // Keyframe diamonds
                            for (kf_idx, &t) in ch.keyframe_times.iter().enumerate() {
                                let x = rect.min.x + 80.0 + (t - self.scroll_offset) * self.zoom;
                                if x >= rect.min.x && x <= rect.max.x {
                                    let center = egui::pos2(x, y + track_height * 0.5);
                                    let size = 5.0;
                                    let diamond = vec![
                                        egui::pos2(center.x, center.y - size),
                                        egui::pos2(center.x + size, center.y),
                                        egui::pos2(center.x, center.y + size),
                                        egui::pos2(center.x - size, center.y),
                                    ];
                                    let is_selected = self.selected_keyframe == Some((i, kf_idx));
                                    let color = if is_selected {
                                        egui::Color32::from_rgb(255, 100, 100)
                                    } else {
                                        egui::Color32::from_rgb(255, 200, 50)
                                    };
                                    painter.add(egui::Shape::convex_polygon(
                                        diamond,
                                        color,
                                        egui::Stroke::NONE,
                                    ));
                                }
                            }
                        }

                        // Scrub cursor
                        let scrub_x =
                            rect.min.x + 80.0 + (self.scrub_time - self.scroll_offset) * self.zoom;
                        if scrub_x >= rect.min.x && scrub_x <= rect.max.x {
                            painter.line_segment(
                                [
                                    egui::pos2(scrub_x, rect.min.y),
                                    egui::pos2(scrub_x, rect.max.y),
                                ],
                                egui::Stroke::new(2.0, egui::Color32::from_rgb(80, 180, 255)),
                            );
                        }

                        // Click to scrub
                        if response.clicked() {
                            if let Some(pos) = response.interact_pointer_pos() {
                                let new_time =
                                    self.scroll_offset + (pos.x - rect.min.x - 80.0) / self.zoom;
                                self.scrub_time = new_time.max(0.0);
                            }
                        }

                        // Keyframe drag-to-retime and click-to-select.
                        if let Some(pos) = response.interact_pointer_pos() {
                            if response.drag_started() {
                                // Find the closest keyframe diamond within 6px.
                                let mut best: Option<(usize, usize, f32)> = None;
                                for (ch_idx, ch) in self.channels.iter().enumerate() {
                                    let cy = rect.min.y
                                        + ch_idx as f32 * track_height
                                        + track_height * 0.5;
                                    for (kf_idx, &t) in ch.keyframe_times.iter().enumerate() {
                                        let kx = rect.min.x
                                            + 80.0
                                            + (t - self.scroll_offset) * self.zoom;
                                        let dist =
                                            ((pos.x - kx).powi(2) + (pos.y - cy).powi(2)).sqrt();
                                        if dist < 6.0 {
                                            if best.is_none() || dist < best.unwrap().2 {
                                                best = Some((ch_idx, kf_idx, dist));
                                            }
                                        }
                                    }
                                }
                                if let Some((ch, kf, _)) = best {
                                    self.dragging_keyframe = Some((ch, kf));
                                    self.selected_keyframe = Some((ch, kf));
                                }
                            }
                            // While dragging, retime the keyframe.
                            if response.dragged() {
                                if let Some((ch_idx, kf_idx)) = self.dragging_keyframe {
                                    let new_time = self.scroll_offset
                                        + (pos.x - rect.min.x - 80.0) / self.zoom;
                                    if ch_idx < self.channels.len()
                                        && kf_idx < self.channels[ch_idx].keyframe_times.len()
                                    {
                                        self.channels[ch_idx].keyframe_times[kf_idx] =
                                            new_time.max(0.0);
                                    }
                                }
                            }
                        }
                        if response.drag_stopped() {
                            self.dragging_keyframe = None;
                        }
                    });
            });

        // Selected keyframe value editor popup.
        if let Some((ch_idx, kf_idx)) = self.selected_keyframe {
            if ch_idx < self.channels.len() && kf_idx < self.channels[ch_idx].keyframe_times.len() {
                let t = self.channels[ch_idx].keyframe_times[kf_idx];
                let popup_x = 80.0 + (t - self.scroll_offset) * self.zoom;
                egui::Area::new(egui::Id::new("kf_editor"))
                    .fixed_pos(egui::pos2(
                        popup_x.max(100.0),
                        ctx.screen_rect().max.y - 220.0,
                    ))
                    .show(ctx, |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            ui.label(format!("Keyframe [{}/{}]", ch_idx, kf_idx));
                            ui.horizontal(|ui| {
                                ui.label("Value:");
                                ui.add(
                                    egui::DragValue::new(
                                        &mut self.channels[ch_idx].keyframe_values[kf_idx],
                                    )
                                    .speed(0.01),
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.label("Interp:");
                                let mode = &mut self.channels[ch_idx].interp_modes[kf_idx];
                                if ui
                                    .selectable_label(
                                        *mode == InterpolationMode::Step,
                                        "\u{25AA} Step",
                                    )
                                    .clicked()
                                {
                                    *mode = InterpolationMode::Step;
                                }
                                if ui
                                    .selectable_label(
                                        *mode == InterpolationMode::Linear,
                                        "\u{2500} Linear",
                                    )
                                    .clicked()
                                {
                                    *mode = InterpolationMode::Linear;
                                }
                                if ui
                                    .selectable_label(
                                        *mode == InterpolationMode::CubicSpline,
                                        "\u{25C6} Cubic",
                                    )
                                    .clicked()
                                {
                                    *mode = InterpolationMode::CubicSpline;
                                }
                            });
                            if ui.button("Close").clicked() {
                                self.selected_keyframe = None;
                            }
                        });
                    });
            } else {
                self.selected_keyframe = None;
            }
        }
    }
}

impl Default for TimelinePanel {
    fn default() -> Self {
        Self::new()
    }
}

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
    /// The currently selected entities (multi-select with Ctrl+Click).
    pub selected_entities: Vec<Entity>,
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
    /// Show the animation timeline panel.
    pub show_timeline: bool,
    /// Animation timeline panel state.
    pub timeline_panel: TimelinePanel,
    /// Show the logic graph editor panel.
    pub show_logic_graph: bool,
    /// Logic graph editor state.
    pub logic_graph_editor_state: LogicGraphEditorState,
    /// Active logic graph being edited.
    pub logic_graph: LogicGraph,
    /// Show the GPU profiler panel.
    pub show_gpu_profiler: bool,
    /// GPU pass timing data (name, duration_ms) from the last frame.
    pub gpu_pass_timings: Vec<(String, f64)>,
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
            selected_entities: Vec::new(),
            console: console::ConsoleLog::new(),
            state: EditorState::new(),
            asset_browser: AssetBrowser::new("assets"),
            show_lua_repl: false,
            lua_repl_input: String::new(),
            lua_repl_output: Vec::new(),
            profiler_frame_times: std::collections::VecDeque::with_capacity(240),
            profiler_max_frames: 240,
            pending_lua_eval: None,
            show_timeline: false,
            timeline_panel: TimelinePanel::new(),
            show_logic_graph: false,
            logic_graph_editor_state: LogicGraphEditorState::new(),
            logic_graph: LogicGraph::new("untitled"),
            show_gpu_profiler: false,
            gpu_pass_timings: Vec::new(),
        }
    }

    /// Toggle the editor overlay on/off.
    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
    }

    /// Render the full editor UI. Call this from your egui integration each frame.
    ///
    /// `inspector_data` should be `Some` when an entity is selected and the
    /// caller has read its components. Returns `(Vec<Box<dyn EditCommand>>, Option<EditorAction>)` where:
    /// - commands contains any entity edit commands generated by the inspector.
    /// - editor_action contains editor actions like Play/Pause/Stop.
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        entity_names: &[(Entity, String)],
        inspector_data: Option<InspectorData>,
    ) -> (Vec<Box<dyn EditCommand>>, Option<EditorAction>) {
        if !self.enabled {
            return (Vec::new(), None);
        }

        let mut commands: Vec<Box<dyn EditCommand>> = Vec::new();
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
                ui.toggle_value(&mut self.show_timeline, "🎬 Timeline");
                ui.toggle_value(&mut self.show_logic_graph, "📜 Logic Graph");
                ui.toggle_value(&mut self.show_gpu_profiler, "⏱ GPU Profiler");
                ui.separator();

                // Play/Pause/Stop buttons
                let play_label = match self.state.mode {
                    EditorMode::Stopped => "▶ Play",
                    EditorMode::Playing => "⏸ Pause",
                    EditorMode::Paused => "▶ Resume",
                    EditorMode::PrefabEdit => "📦 Prefab",
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
                        EditorMode::PrefabEdit => {} // button disabled in prefab mode
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
            hierarchy::hierarchy_panel(ctx, &mut self.selected_entities, entity_names);
        }

        // Inspector panel
        if self.show_inspector {
            let default_data = InspectorData {
                transform: quasar_math::Transform::IDENTITY,
                material: Some(quasar_render::MaterialOverride::default()),
            };
            let actual_data = inspector_data.unwrap_or(default_data);
            commands.extend(inspector::inspector_panel(
                ctx,
                &self.selected_entities,
                actual_data,
            ));
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
                        let avg: f32 = self.profiler_frame_times.iter().sum::<f32>()
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

        // Animation timeline panel
        if self.show_timeline {
            self.timeline_panel.ui(ctx);
        }

        // Logic graph editor panel
        if self.show_logic_graph {
            self.logic_graph_editor_state
                .panel(ctx, &mut self.logic_graph);
        }

        // GPU profiler panel
        if self.show_gpu_profiler {
            egui::Window::new("⏱ GPU Profiler")
                .default_size([350.0, 250.0])
                .resizable(true)
                .show(ctx, |ui| {
                    if self.gpu_pass_timings.is_empty() {
                        ui.label("No GPU timing data. Wire wgpu timestamp queries to Editor::gpu_pass_timings.");
                    } else {
                        let total_ms: f64 = self.gpu_pass_timings.iter().map(|(_, ms)| ms).sum();
                        ui.label(format!("GPU frame: {:.2} ms", total_ms));
                        ui.separator();
                        let max_ms = self.gpu_pass_timings.iter().map(|(_, ms)| *ms).fold(0.0f64, f64::max);
                        for (name, ms) in &self.gpu_pass_timings {
                            let frac = if max_ms > 0.0 { *ms / max_ms } else { 0.0 };
                            ui.horizontal(|ui| {
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(120.0, 14.0),
                                    egui::Sense::hover(),
                                );
                                let painter = ui.painter_at(rect);
                                painter.rect_filled(rect, 2.0, egui::Color32::from_rgb(30, 30, 35));
                                let bar_rect = egui::Rect::from_min_size(
                                    rect.min,
                                    egui::vec2(120.0 * frac as f32, 14.0),
                                );
                                let color = if *ms > 4.0 {
                                    egui::Color32::from_rgb(255, 80, 80)
                                } else if *ms > 2.0 {
                                    egui::Color32::YELLOW
                                } else {
                                    egui::Color32::from_rgb(80, 200, 80)
                                };
                                painter.rect_filled(bar_rect, 2.0, color);
                                ui.label(format!("{}: {:.2} ms", name, ms));
                            });
                        }
                    }
                });
        }

        // Keyboard shortcuts
        ctx.input(|i| {
            if i.modifiers.ctrl && i.key_pressed(egui::Key::Z) {
                if i.modifiers.shift {
                    editor_action = Some(EditorAction::Redo);
                } else {
                    editor_action = Some(EditorAction::Undo);
                }
            }
            if i.modifiers.ctrl && i.key_pressed(egui::Key::Y) {
                editor_action = Some(EditorAction::Redo);
            }
        });

        (commands, editor_action)
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
