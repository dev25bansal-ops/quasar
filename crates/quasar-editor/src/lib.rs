//! # Quasar Editor
//!
//! Visual scene editor built with [`egui`].
//!
//! Provides a runtime GUI overlay for inspecting entities, viewing logs,
//! and tweaking component values — press F12 to toggle.

pub mod console;
pub mod hierarchy;
pub mod inspector;
pub mod renderer;

pub use inspector::{InspectorAction, InspectorData};
use quasar_core::ecs::Entity;

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
    /// The currently selected entity (if any).
    pub selected_entity: Option<Entity>,
    /// Console log buffer.
    pub console: console::ConsoleLog,
}

impl Editor {
    pub fn new() -> Self {
        Self {
            enabled: false,
            show_hierarchy: true,
            show_inspector: true,
            show_console: false,
            show_metrics: true,
            selected_entity: None,
            console: console::ConsoleLog::new(),
        }
    }

    /// Toggle the editor overlay on/off.
    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
    }

    /// Render the full editor UI. Call this from your egui integration each frame.
    ///
    /// `inspector_data` should be `Some` when an entity is selected and the
    /// caller has read its components.  Edited values are written in-place;
    /// the function returns `(bool, Option<InspectorAction>)` where:
    /// - `bool` indicates if anything was changed so the caller can write back to ECS.
    /// - `Option<InspectorAction>` contains any action requested (despawn/spawn).
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        entity_names: &[(Entity, String)],
        inspector_data: Option<&mut InspectorData>,
    ) -> (bool, Option<InspectorAction>) {
        if !self.enabled {
            return (false, None);
        }

        // Top menu bar
        egui::TopBottomPanel::top("editor_menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.label("🚀 Quasar Editor");
                ui.separator();
                ui.toggle_value(&mut self.show_hierarchy, "📋 Hierarchy");
                ui.toggle_value(&mut self.show_inspector, "🔍 Inspector");
                ui.toggle_value(&mut self.show_console, "📝 Console");
                ui.toggle_value(&mut self.show_metrics, "📊 Metrics");
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
                    ui.separator();
                    ui.label("Press F12 to toggle editor");
                });
        }

        (inspector_changed, inspector_action)
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}
