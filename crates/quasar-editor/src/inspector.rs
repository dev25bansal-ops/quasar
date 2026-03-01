//! Inspector panel — shows details of the selected entity.

use quasar_core::ecs::Entity;

/// Draw the inspector panel for the selected entity.
pub fn inspector_panel(ctx: &egui::Context, selected: Option<Entity>) {
    egui::SidePanel::right("inspector")
        .default_width(280.0)
        .show(ctx, |ui| {
            ui.heading("🔍 Inspector");
            ui.separator();

            match selected {
                Some(entity) => {
                    ui.label(format!(
                        "Entity: [{}:{}]",
                        entity.index(), entity.generation()
                    ));
                    ui.separator();

                    // Transform section
                    egui::CollapsingHeader::new("Transform")
                        .default_open(true)
                        .show(ui, |ui| {
                            ui.label("Position: (read from ECS)");
                            ui.label("Rotation: (read from ECS)");
                            ui.label("Scale:    (read from ECS)");
                            ui.small("(Live editing coming soon)");
                        });

                    // Components section
                    egui::CollapsingHeader::new("Components")
                        .default_open(true)
                        .show(ui, |ui| {
                            ui.label("Component list populated at runtime.");
                            ui.small("Attach a Name component for display.");
                        });

                    ui.separator();
                    if ui.button("🗑 Despawn Entity").clicked() {
                        log::info!(
                            "Despawn requested for entity [{}:{}]",
                            entity.index(),
                            entity.generation()
                        );
                        // Actual despawn handled by the editor integration system.
                    }
                }
                None => {
                    ui.label("Select an entity in the Hierarchy panel.");
                }
            }
        });
}
