//! # Quasar Editor
//!
//! Visual scene editor built with [`egui`].
//!
//! Provides a GUI overlay for inspecting and editing ECS entities,
//! components, and scene hierarchy at runtime.
//!
//! **Status**: Scaffolded — full implementation coming in Week 3–4.

/// Editor state — tracks UI panels and selection.
pub struct Editor {
    pub enabled: bool,
    pub show_hierarchy: bool,
    pub show_inspector: bool,
    pub show_console: bool,
}

impl Editor {
    pub fn new() -> Self {
        Self {
            enabled: false,
            show_hierarchy: true,
            show_inspector: true,
            show_console: false,
        }
    }

    /// Toggle the editor overlay on/off.
    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
    }

    /// Render editor UI (placeholder).
    pub fn ui(&mut self, ctx: &egui::Context) {
        if !self.enabled {
            return;
        }

        egui::TopBottomPanel::top("editor_menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.label("🚀 Quasar Editor");
                ui.separator();
                ui.toggle_value(&mut self.show_hierarchy, "Hierarchy");
                ui.toggle_value(&mut self.show_inspector, "Inspector");
                ui.toggle_value(&mut self.show_console, "Console");
            });
        });

        if self.show_hierarchy {
            egui::SidePanel::left("hierarchy").show(ctx, |ui| {
                ui.heading("Scene Hierarchy");
                ui.label("(coming soon)");
            });
        }

        if self.show_inspector {
            egui::SidePanel::right("inspector").show(ctx, |ui| {
                ui.heading("Inspector");
                ui.label("(coming soon)");
            });
        }
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}
