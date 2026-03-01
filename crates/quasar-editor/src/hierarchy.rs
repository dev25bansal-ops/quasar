//! Hierarchy panel — shows all entities in a tree-like list.

use quasar_core::ecs::Entity;

/// Draw the scene hierarchy panel.
pub fn hierarchy_panel(
    ctx: &egui::Context,
    selected: &mut Option<Entity>,
    entities: &[(Entity, String)],
) {
    egui::SidePanel::left("hierarchy")
        .default_width(220.0)
        .show(ctx, |ui| {
            ui.heading("📋 Scene Hierarchy");
            ui.separator();

            if entities.is_empty() {
                ui.label("(no entities)");
                return;
            }

            egui::ScrollArea::vertical().show(ui, |ui| {
                for (entity, name) in entities {
                    let is_selected = *selected == Some(*entity);
                    let label = format!("{} [{}:{}]", name, entity.index(), entity.generation());

                    let response = ui.selectable_label(is_selected, &label);
                    if response.clicked() {
                        *selected = Some(*entity);
                    }
                }
            });

            ui.separator();
            ui.label(format!("Total: {}", entities.len()));
        });
}
