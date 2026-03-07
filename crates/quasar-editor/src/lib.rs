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

pub use asset_browser::{AssetBrowser, AssetEntry, AssetKind};
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
    /// The currently selected entity (if any).
    pub selected_entity: Option<Entity>,
    /// Console log buffer.
    pub console: console::ConsoleLog,
    /// Editor state for Play/Pause/Stop and undo/redo
    pub state: EditorState,
    /// Asset browser panel.
    pub asset_browser: AssetBrowser,
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
            selected_entity: None,
            console: console::ConsoleLog::new(),
            state: EditorState::new(),
            asset_browser: AssetBrowser::new("assets"),
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

        // Metrics overlay
        if self.show_metrics {
            egui::Window::new("📊 Metrics")
                .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-10.0, 40.0))
                .resizable(false)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label(format!("Entities: {}", entity_names.len()));
                    ui.label(format!("Mode: {:?}", self.state.mode));
                    ui.separator();
                    ui.label("Press F12 to toggle editor");
                    ui.label("Ctrl+Z: Undo | Ctrl+Y: Redo");
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
        }
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}
