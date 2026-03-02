//! Inspector panel — shows and edits details of the selected entity.

use quasar_core::ecs::Entity;
use quasar_math::{EulerRot, Quat, Transform};
use quasar_render::MaterialOverride;

/// Data bundle passed into the inspector for display and editing.
///
/// The caller (runner) fills this with the entity's current component values.
/// After [`inspector_panel`] returns, any modified fields can be written back
/// to the ECS world.
pub struct InspectorData {
    /// A copy of the entity's [`Transform`]. Mutated in-place by the editor.
    pub transform: Transform,
    /// Optional material override. `None` = entity has no `MaterialOverride`.
    pub material: Option<MaterialOverride>,
}

/// Draw the inspector panel for the selected entity.
///
/// Returns `true` if any value was changed (so the caller knows to write back).
pub fn inspector_panel(
    ctx: &egui::Context,
    selected: Option<Entity>,
    data: Option<&mut InspectorData>,
) -> bool {
    let mut changed = false;

    egui::SidePanel::right("inspector")
        .default_width(280.0)
        .show(ctx, |ui| {
            ui.heading("🔍 Inspector");
            ui.separator();

            let (entity, data) = match (selected, data) {
                (Some(e), Some(d)) => (e, d),
                (Some(e), None) => {
                    ui.label(format!(
                        "Entity [{}:{}] — no editable data",
                        e.index(),
                        e.generation()
                    ));
                    return;
                }
                _ => {
                    ui.label("Select an entity in the Hierarchy panel.");
                    return;
                }
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
                    ui.label("Position");
                    let t = &mut data.transform;
                    changed |= ui
                        .horizontal(|ui| {
                            let mut c = false;
                            c |= ui
                                .add(egui::DragValue::new(&mut t.position.x).speed(0.05).prefix("X "))
                                .changed();
                            c |= ui
                                .add(egui::DragValue::new(&mut t.position.y).speed(0.05).prefix("Y "))
                                .changed();
                            c |= ui
                                .add(egui::DragValue::new(&mut t.position.z).speed(0.05).prefix("Z "))
                                .changed();
                            c
                        })
                        .inner;

                    ui.label("Rotation (Euler °)");
                    let (mut rx, mut ry, mut rz) = {
                        let (y, x, z) = t.rotation.to_euler(EulerRot::YXZ);
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
                        t.rotation = Quat::from_euler(
                            EulerRot::YXZ,
                            ry.to_radians(),
                            rx.to_radians(),
                            rz.to_radians(),
                        );
                        changed = true;
                    }

                    ui.label("Scale");
                    changed |= ui
                        .horizontal(|ui| {
                            let mut c = false;
                            c |= ui
                                .add(
                                    egui::DragValue::new(&mut t.scale.x)
                                        .speed(0.01)
                                        .prefix("X ")
                                        .range(0.01..=100.0),
                                )
                                .changed();
                            c |= ui
                                .add(
                                    egui::DragValue::new(&mut t.scale.y)
                                        .speed(0.01)
                                        .prefix("Y ")
                                        .range(0.01..=100.0),
                                )
                                .changed();
                            c |= ui
                                .add(
                                    egui::DragValue::new(&mut t.scale.z)
                                        .speed(0.01)
                                        .prefix("Z ")
                                        .range(0.01..=100.0),
                                )
                                .changed();
                            c
                        })
                        .inner;
                });

            // ── Material section ─────────────────────────────────
            if let Some(mat) = &mut data.material {
                ui.separator();
                egui::CollapsingHeader::new("Material Override")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.label("Base Color");
                        changed |= ui
                            .horizontal(|ui| {
                                let mut c = false;
                                c |= ui
                                    .add(
                                        egui::DragValue::new(&mut mat.base_color[0])
                                            .speed(0.01)
                                            .prefix("R ")
                                            .range(0.0..=1.0),
                                    )
                                    .changed();
                                c |= ui
                                    .add(
                                        egui::DragValue::new(&mut mat.base_color[1])
                                            .speed(0.01)
                                            .prefix("G ")
                                            .range(0.0..=1.0),
                                    )
                                    .changed();
                                c |= ui
                                    .add(
                                        egui::DragValue::new(&mut mat.base_color[2])
                                            .speed(0.01)
                                            .prefix("B ")
                                            .range(0.0..=1.0),
                                    )
                                    .changed();
                                c
                            })
                            .inner;

                        changed |= ui
                            .add(
                                egui::Slider::new(&mut mat.roughness, 0.0..=1.0)
                                    .text("Roughness"),
                            )
                            .changed();
                        changed |= ui
                            .add(
                                egui::Slider::new(&mut mat.metallic, 0.0..=1.0).text("Metallic"),
                            )
                            .changed();
                        changed |= ui
                            .add(
                                egui::Slider::new(&mut mat.emissive, 0.0..=10.0)
                                    .text("Emissive"),
                            )
                            .changed();
                    });
            }

            ui.separator();
            if ui.button("🗑 Despawn Entity").clicked() {
                log::info!(
                    "Despawn requested for entity [{}:{}]",
                    entity.index(),
                    entity.generation()
                );
            }
        });

    changed
}
