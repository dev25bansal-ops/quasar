//! AI Blackboard Debugger Panel - inspect and modify AI blackboard values.

use egui::{Color32, RichText};
use quasar_core::ai::{Blackboard, BlackboardValue};
use std::collections::HashMap;

/// State for the blackboard debugger panel.
pub struct BlackboardDebugger {
    pub selected_entity: Option<u64>,
    pub filter: String,
    pub show_type_column: bool,
    pub edit_buffer: HashMap<String, String>,
}

impl Default for BlackboardDebugger {
    fn default() -> Self {
        Self {
            selected_entity: None,
            filter: String::new(),
            show_type_column: true,
            edit_buffer: HashMap::new(),
        }
    }
}

impl BlackboardDebugger {
    pub fn new() -> Self {
        Self::default()
    }

    /// Draw the blackboard debugger panel.
    pub fn ui(&mut self, ctx: &egui::Context, blackboards: &[(u64, &Blackboard, Option<&str>)]) {
        egui::Window::new("🧠 AI Blackboard Debugger")
            .default_size([400.0, 500.0])
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Entity:");
                    egui::ComboBox::from_id_salt("entity_select")
                        .width(200.0)
                        .selected_text(
                            self.selected_entity
                                .map_or("None".into(), |e| format!("{:?}", e)),
                        )
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.selected_entity, None, "None");
                            for (id, _, name) in blackboards {
                                let label =
                                    name.map_or_else(|| format!("{:?}", id), |n| n.to_string());
                                ui.selectable_value(&mut self.selected_entity, Some(*id), label);
                            }
                        });
                    ui.separator();
                    ui.checkbox(&mut self.show_type_column, "Show Types");
                });

                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("🔍");
                    ui.text_edit_singleline(&mut self.filter);
                });

                ui.separator();

                let selected_bb = self.selected_entity.and_then(|id| {
                    blackboards
                        .iter()
                        .find(|(eid, _, _)| *eid == id)
                        .map(|(_, bb, _)| *bb)
                });

                if let Some(bb) = selected_bb {
                    self.draw_blackboard_contents(ui, bb);
                } else {
                    ui.label("Select an entity to view its blackboard");
                }
            });
    }

    fn draw_blackboard_contents(&mut self, ui: &mut egui::Ui, blackboard: &Blackboard) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            let filter_lower = self.filter.to_lowercase();

            let mut keys: Vec<_> = blackboard.keys().collect();
            keys.sort();

            for key in keys {
                if !filter_lower.is_empty() && !key.to_lowercase().contains(&filter_lower) {
                    continue;
                }

                if let Some(value) = blackboard.get(key) {
                    ui.push_id(key, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(key)
                                    .strong()
                                    .color(Color32::from_rgb(150, 200, 255)),
                            );

                            if self.show_type_column {
                                let type_str = match value {
                                    BlackboardValue::Bool(_) => "bool",
                                    BlackboardValue::Int(_) => "int",
                                    BlackboardValue::Float(_) => "float",
                                    BlackboardValue::String(_) => "string",
                                    BlackboardValue::Vec2(_) => "vec2",
                                    BlackboardValue::Vec3(_) => "vec3",
                                    BlackboardValue::Entity(_) => "entity",
                                    BlackboardValue::Timestamp(_) => "timestamp",
                                    BlackboardValue::Duration(_) => "duration",
                                };
                                ui.label(RichText::new(format!("[{}]", type_str)).weak());
                            }

                            ui.separator();

                            let (value_str, color) = match value {
                                BlackboardValue::Bool(b) => (
                                    b.to_string(),
                                    if *b {
                                        Color32::LIGHT_GREEN
                                    } else {
                                        Color32::LIGHT_RED
                                    },
                                ),
                                BlackboardValue::Int(i) => (i.to_string(), Color32::WHITE),
                                BlackboardValue::Float(f) => {
                                    (format!("{:.3}", f), Color32::from_rgb(255, 200, 100))
                                }
                                BlackboardValue::String(s) => {
                                    (format!("\"{}\"", s), Color32::LIGHT_BLUE)
                                }
                                BlackboardValue::Vec2(v) => (
                                    format!("({}, {})", v[0], v[1]),
                                    Color32::from_rgb(200, 255, 200),
                                ),
                                BlackboardValue::Vec3(v) => (
                                    format!("({}, {}, {})", v[0], v[1], v[2]),
                                    Color32::from_rgb(200, 255, 200),
                                ),
                                BlackboardValue::Entity(e) => {
                                    (format!("Entity({:?})", e), Color32::from_rgb(255, 150, 255))
                                }
                                BlackboardValue::Timestamp(t) => {
                                    (format!("T({})", t), Color32::GRAY)
                                }
                                BlackboardValue::Duration(d) => {
                                    (format!("{:.2}s", d), Color32::GRAY)
                                }
                            };

                            ui.label(RichText::new(&value_str).color(color));

                            if blackboard.is_dirty(key) {
                                ui.label(RichText::new("*").color(Color32::YELLOW));
                            }
                        });
                    });
                }
            }
        });
    }
}

/// Trait to access keys from blackboard (workaround for private field).
pub trait BlackboardExt {
    fn keys(&self) -> Vec<&str>;
}

impl BlackboardExt for Blackboard {
    fn keys(&self) -> Vec<&str> {
        Blackboard::keys(self).map(|s| s.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debugger_default() {
        let debugger = BlackboardDebugger::default();
        assert!(debugger.selected_entity.is_none());
        assert!(debugger.show_type_column);
    }

    #[test]
    fn debugger_new() {
        let debugger = BlackboardDebugger::new();
        assert!(debugger.filter.is_empty());
    }
}
