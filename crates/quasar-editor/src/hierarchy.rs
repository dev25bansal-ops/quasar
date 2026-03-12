//! Hierarchy panel — shows all entities in a tree-like list.
//!
//! Supports multi-select with Ctrl+Click.

use quasar_core::ecs::Entity;

/// Draw the scene hierarchy panel.
pub fn hierarchy_panel(
    ctx: &egui::Context,
    selected: &mut Vec<Entity>,
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
                    let is_selected = selected.contains(entity);
                    let label = format!("{} [{}:{}]", name, entity.index(), entity.generation());

                    let response = ui.selectable_label(is_selected, &label);
                    if response.clicked() {
                        let ctrl = ui.input(|i| i.modifiers.ctrl || i.modifiers.command);
                        if ctrl {
                            if is_selected {
                                selected.retain(|e| e != entity);
                            } else {
                                selected.push(*entity);
                            }
                        } else {
                            selected.clear();
                            selected.push(*entity);
                        }
                    }
                }
            });

            ui.separator();
            ui.label(format!(
                "Total: {} | Selected: {}",
                entities.len(),
                selected.len()
            ));
        });
}
