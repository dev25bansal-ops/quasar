//! Inspector panel — shows and edits details of the selected entity.

use quasar_core::ecs::Entity;
use quasar_math::{EulerRot, Quat, Transform};
use quasar_render::MaterialOverride;

use crate::editor_state::{
    DeleteEntityCommand, EditCommand, SetMaterialCommand, SetPositionCommand, SetRotationCommand,
    SetScaleCommand, SpawnEntityCommand, TransformData,
};

pub enum InspectorAction {
    Despawn(Entity),
    Spawn,
}

/// Data bundle passed into the inspector for display and editing.
///
/// Stores initial values used to generate EditCommands after user edits.
pub struct InspectorData {
    /// A copy of the entity's current [`Transform`].
    pub transform: Transform,
    /// Optional material override. `None` = entity has no `MaterialOverride`.
    pub material: Option<MaterialOverride>,
}

/// Draw the inspector panel for the selected entity.
///
/// Returns `Vec<Box<dyn EditCommand>>` containing all mutations performed.
pub fn inspector_panel(
    ctx: &egui::Context,
    selected: &[Entity],
    data: InspectorData,
) -> Vec<Box<dyn EditCommand>> {
    let mut commands: Vec<Box<dyn EditCommand>> = Vec::new();
    let entity = selected.first().copied();

    egui::SidePanel::right("inspector")
        .default_width(280.0)
        .show(ctx, |ui| {
            ui.heading("🔍 Inspector");
            ui.separator();

            if selected.len() > 1 {
                ui.label(format!("{} entities selected", selected.len()));
                for e in selected {
                    ui.label(format!("  • [{}:{}]", e.index(), e.generation()));
                }
                return;
            }

            let Some(entity) = entity else {
                ui.label("Select an entity in the Hierarchy panel.");
                return;
            };

            ui.label(format!(
                "Entity: [{}:{}]",
                entity.index(),
                entity.generation()
            ));
            ui.separator();

            // ── Transform section ────────────────────────────────
            egui::CollapsingHeader::new("Transform")
                .default_open(true)
                .show(ui, |ui| {
                    let mut local_transform = data.transform;

                    ui.label("Position");
                    let pos_changed = ui
                        .horizontal(|ui| {
                            let mut c = false;
                            c |= ui
                                .add(
                                    egui::DragValue::new(&mut local_transform.position.x)
                                        .speed(0.05)
                                        .prefix("X "),
                                )
                                .changed();
                            c |= ui
                                .add(
                                    egui::DragValue::new(&mut local_transform.position.y)
                                        .speed(0.05)
                                        .prefix("Y "),
                                )
                                .changed();
                            c |= ui
                                .add(
                                    egui::DragValue::new(&mut local_transform.position.z)
                                        .speed(0.05)
                                        .prefix("Z "),
                                )
                                .changed();
                            c
                        })
                        .inner;

                    if pos_changed {
                        commands.push(Box::new(SetPositionCommand {
                            entity,
                            old_position: data.transform.position,
                            new_position: local_transform.position,
                        }));
                    }

                    ui.label("Rotation (Euler °)");
                    let (mut rx, mut ry, mut rz) = {
                        let (y, x, z) = local_transform.rotation.to_euler(EulerRot::YXZ);
                        (x.to_degrees(), y.to_degrees(), z.to_degrees())
                    };
                    let rot_changed = ui
                        .horizontal(|ui| {
                            let mut c = false;
                            c |= ui
                                .add(
                                    egui::DragValue::new(&mut rx)
                                        .speed(0.5)
                                        .prefix("X ")
                                        .range(-180.0..=180.0),
                                )
                                .changed();
                            c |= ui
                                .add(
                                    egui::DragValue::new(&mut ry)
                                        .speed(0.5)
                                        .prefix("Y ")
                                        .range(-180.0..=180.0),
                                )
                                .changed();
                            c |= ui
                                .add(
                                    egui::DragValue::new(&mut rz)
                                        .speed(0.5)
                                        .prefix("Z ")
                                        .range(-180.0..=180.0),
                                )
                                .changed();
                            c
                        })
                        .inner;

                    if rot_changed {
                        local_transform.rotation = Quat::from_euler(
                            EulerRot::YXZ,
                            ry.to_radians(),
                            rx.to_radians(),
                            rz.to_radians(),
                        );
                        commands.push(Box::new(SetRotationCommand {
                            entity,
                            old_rotation: data.transform.rotation,
                            new_rotation: local_transform.rotation,
                        }));
                    }

                    ui.label("Scale");
                    let scale_changed = ui
                        .horizontal(|ui| {
                            let mut c = false;
                            c |= ui
                                .add(
                                    egui::DragValue::new(&mut local_transform.scale.x)
                                        .speed(0.01)
                                        .prefix("X ")
                                        .range(0.01..=100.0),
                                )
                                .changed();
                            c |= ui
                                .add(
                                    egui::DragValue::new(&mut local_transform.scale.y)
                                        .speed(0.01)
                                        .prefix("Y ")
                                        .range(0.01..=100.0),
                                )
                                .changed();
                            c |= ui
                                .add(
                                    egui::DragValue::new(&mut local_transform.scale.z)
                                        .speed(0.01)
                                        .prefix("Z ")
                                        .range(0.01..=100.0),
                                )
                                .changed();
                            c
                        })
                        .inner;

                    if scale_changed {
                        commands.push(Box::new(SetScaleCommand {
                            entity,
                            old_scale: data.transform.scale,
                            new_scale: local_transform.scale,
                        }));
                    }
                });

            // ── Material section ─────────────────────────────────
            if let Some(ref old_mat) = data.material {
                ui.separator();
                egui::CollapsingHeader::new("Material Override")
                    .default_open(true)
                    .show(ui, |ui| {
                        let mut local_mat = *old_mat;

                        ui.label("Base Color");
                        let color_changed = ui
                            .horizontal(|ui| {
                                let mut c = false;
                                c |= ui
                                    .add(
                                        egui::DragValue::new(&mut local_mat.base_color[0])
                                            .speed(0.01)
                                            .prefix("R ")
                                            .range(0.0..=1.0),
                                    )
                                    .changed();
                                c |= ui
                                    .add(
                                        egui::DragValue::new(&mut local_mat.base_color[1])
                                            .speed(0.01)
                                            .prefix("G ")
                                            .range(0.0..=1.0),
                                    )
                                    .changed();
                                c |= ui
                                    .add(
                                        egui::DragValue::new(&mut local_mat.base_color[2])
                                            .speed(0.01)
                                            .prefix("B ")
                                            .range(0.0..=1.0),
                                    )
                                    .changed();
                                c
                            })
                            .inner;

                        let rough_changed = ui
                            .add(
                                egui::Slider::new(&mut local_mat.roughness, 0.0..=1.0)
                                    .text("Roughness"),
                            )
                            .changed();
                        let metal_changed = ui
                            .add(
                                egui::Slider::new(&mut local_mat.metallic, 0.0..=1.0)
                                    .text("Metallic"),
                            )
                            .changed();
                        let emissive_changed = ui
                            .add(
                                egui::Slider::new(&mut local_mat.emissive, 0.0..=10.0)
                                    .text("Emissive"),
                            )
                            .changed();

                        if color_changed || rough_changed || metal_changed || emissive_changed {
                            commands.push(Box::new(SetMaterialCommand {
                                entity,
                                old_base_color: old_mat.base_color,
                                new_base_color: local_mat.base_color,
                                old_roughness: old_mat.roughness,
                                new_roughness: local_mat.roughness,
                                old_metallic: old_mat.metallic,
                                new_metallic: local_mat.metallic,
                                old_emissive: old_mat.emissive,
                                new_emissive: local_mat.emissive,
                            }));
                        }
                    });
            }

            ui.separator();
            if ui.button("🗑 Despawn Entity").clicked() {
                commands.push(Box::new(DeleteEntityCommand::new(entity)));
            }
            if ui.button("➕ Spawn Entity").clicked() {
                let transform_data = TransformData {
                    position: [0.0, 0.0, 0.0],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scale: [1.0, 1.0, 1.0],
                };
                commands.push(Box::new(SpawnEntityCommand::new(transform_data)));
            }
        });

    commands
}
