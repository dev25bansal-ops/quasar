//! Network Lag Compensation Visualization - debug panel for netcode.

use egui::{Color32, Pos2, Rect, RichText, Stroke};
use std::collections::VecDeque;

pub const MAX_HISTORY: usize = 120;

#[derive(Debug, Clone, Copy)]
pub struct NetworkSample {
    pub timestamp: f64,
    pub rtt_ms: f32,
    pub server_tick_ms: f32,
    pub local_tick_ms: f32,
    pub predicted_tick: u64,
    pub confirmed_tick: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LagSource {
    NetworkLatency,
    ServerProcessing,
    ClientPrediction,
    InterpolationDelay,
}

#[derive(Debug, Clone)]
pub struct EntitySyncData {
    pub entity_id: u64,
    pub local_position: [f32; 3],
    pub server_position: [f32; 3],
    pub position_error: f32,
    pub is_interpolating: bool,
}

pub struct NetworkVisualizer {
    pub enabled: bool,
    pub samples: VecDeque<NetworkSample>,
    pub entity_sync: Vec<EntitySyncData>,
    pub show_timeline: bool,
    pub show_entity_positions: bool,
    pub show_prediction_window: bool,
    pub interpolation_delay_ticks: u32,
    pub prediction_window_ticks: u32,
    pub rtt_threshold_warning: f32,
    pub rtt_threshold_critical: f32,
}

impl Default for NetworkVisualizer {
    fn default() -> Self {
        Self {
            enabled: true,
            samples: VecDeque::with_capacity(MAX_HISTORY),
            entity_sync: Vec::new(),
            show_timeline: true,
            show_entity_positions: true,
            show_prediction_window: true,
            interpolation_delay_ticks: 3,
            prediction_window_ticks: 10,
            rtt_threshold_warning: 100.0,
            rtt_threshold_critical: 200.0,
        }
    }
}

impl NetworkVisualizer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_sample(&mut self, sample: NetworkSample) {
        if self.samples.len() >= MAX_HISTORY {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }

    pub fn clear_entity_sync(&mut self) {
        self.entity_sync.clear();
    }

    pub fn add_entity_sync(&mut self, data: EntitySyncData) {
        self.entity_sync.push(data);
    }

    pub fn ui(&mut self, ctx: &egui::Context) {
        if !self.enabled {
            return;
        }

        egui::Window::new("🌐 Network Lag Visualization")
            .default_size([600.0, 450.0])
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.show_timeline, "Timeline");
                    ui.checkbox(&mut self.show_entity_positions, "Entity Sync");
                    ui.checkbox(&mut self.show_prediction_window, "Prediction Window");
                    ui.separator();
                    ui.label(format!("Buffered: {} samples", self.samples.len()));
                });

                ui.separator();

                if self.show_timeline {
                    self.draw_timeline(ui);
                }

                if self.show_prediction_window {
                    self.draw_prediction_window(ui);
                }

                if self.show_entity_positions {
                    self.draw_entity_positions(ui);
                }
            });
    }

    fn draw_timeline(&self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label(RichText::new("📊 Network Timeline").strong());

            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), 120.0),
                egui::Sense::hover(),
            );

            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(20, 20, 25));

            if self.samples.is_empty() {
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "No samples yet",
                    egui::FontId::proportional(14.0),
                    Color32::GRAY,
                );
                return;
            }

            let max_rtt = self
                .samples
                .iter()
                .map(|s| s.rtt_ms)
                .fold(0.0, f32::max)
                .max(50.0);
            let bar_width = rect.width() / MAX_HISTORY as f32;

            for (i, sample) in self.samples.iter().enumerate() {
                let x = rect.min.x + i as f32 * bar_width;
                let rtt_height = (sample.rtt_ms / max_rtt) * (rect.height() - 20.0);

                let color = if sample.rtt_ms > self.rtt_threshold_critical {
                    Color32::from_rgb(255, 80, 80)
                } else if sample.rtt_ms > self.rtt_threshold_warning {
                    Color32::from_rgb(255, 200, 80)
                } else {
                    Color32::from_rgb(80, 200, 80)
                };

                painter.rect_filled(
                    Rect::from_min_size(
                        Pos2::new(x, rect.max.y - rtt_height - 10.0),
                        egui::vec2(bar_width - 1.0, rtt_height),
                    ),
                    0.0,
                    color,
                );
            }

            painter.text(
                Pos2::new(rect.min.x + 5.0, rect.min.y + 5.0),
                egui::Align2::LEFT_TOP,
                format!(
                    "RTT: {:.1}ms (max: {:.1}ms)",
                    self.samples.back().map_or(0.0, |s| s.rtt_ms),
                    max_rtt
                ),
                egui::FontId::proportional(11.0),
                Color32::WHITE,
            );

            painter.line_segment(
                [
                    Pos2::new(rect.min.x, rect.max.y - 10.0),
                    Pos2::new(rect.max.x, rect.max.y - 10.0),
                ],
                Stroke::new(1.0, Color32::from_rgb(60, 60, 70)),
            );
        });

        ui.add_space(8.0);
    }

    fn draw_prediction_window(&self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label(RichText::new("Prediction Window").strong());

            if let Some(sample) = self.samples.back() {
                let behind = sample.predicted_tick.saturating_sub(sample.confirmed_tick);

                ui.horizontal(|ui| {
                    ui.label(format!("Predicted Tick: {}", sample.predicted_tick));
                    ui.label("|");
                    ui.label(format!("Confirmed Tick: {}", sample.confirmed_tick));
                    ui.label("|");
                    let color = if behind > 5 {
                        Color32::LIGHT_RED
                    } else {
                        Color32::LIGHT_GREEN
                    };
                    ui.label(RichText::new(format!("Behind: {} ticks", behind)).color(color));
                });

                ui.add_space(4.0);

                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), 40.0),
                    egui::Sense::hover(),
                );

                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 2.0, Color32::from_rgb(25, 25, 30));

                let window_width = rect.width() * 0.6;
                let confirmed_x = rect.min.x + rect.width() * 0.2;
                let predicted_x = confirmed_x + window_width * (behind as f32 / 10.0).min(1.0);

                painter.rect_filled(
                    Rect::from_min_size(
                        Pos2::new(confirmed_x, rect.min.y + 10.0),
                        egui::vec2(window_width, 20.0),
                    ),
                    2.0,
                    Color32::from_rgb(60, 100, 60),
                );

                painter.circle_filled(
                    Pos2::new(confirmed_x, rect.center().y),
                    6.0,
                    Color32::LIGHT_GREEN,
                );
                painter.circle_filled(
                    Pos2::new(predicted_x.min(rect.max.x - 10.0), rect.center().y),
                    6.0,
                    Color32::YELLOW,
                );

                painter.text(
                    Pos2::new(confirmed_x, rect.min.y + 2.0),
                    egui::Align2::CENTER_BOTTOM,
                    "C",
                    egui::FontId::proportional(10.0),
                    Color32::WHITE,
                );
                painter.text(
                    Pos2::new(predicted_x.min(rect.max.x - 10.0), rect.min.y + 2.0),
                    egui::Align2::CENTER_BOTTOM,
                    "P",
                    egui::FontId::proportional(10.0),
                    Color32::WHITE,
                );
            } else {
                ui.label("No data");
            }
        });

        ui.add_space(8.0);
    }

    fn draw_entity_positions(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label(RichText::new("🎯 Entity Sync Errors").strong());

            egui::ScrollArea::vertical()
                .max_height(150.0)
                .show(ui, |ui| {
                    if self.entity_sync.is_empty() {
                        ui.label("No entity sync data");
                        return;
                    }

                    for entity in &self.entity_sync {
                        ui.horizontal(|ui| {
                            ui.label(format!("Entity {:?}:", entity.entity_id));

                            let error_color = if entity.position_error > 1.0 {
                                Color32::LIGHT_RED
                            } else if entity.position_error > 0.1 {
                                Color32::YELLOW
                            } else {
                                Color32::LIGHT_GREEN
                            };

                            ui.label(
                                RichText::new(format!("Δ = {:.3}m", entity.position_error))
                                    .color(error_color),
                            );

                            if entity.is_interpolating {
                                ui.label(RichText::new("[interp]").color(Color32::LIGHT_BLUE));
                            }
                        });
                    }
                });
        });
    }

    pub fn get_status(&self) -> (f32, f32, u64, u64) {
        self.samples.back().map_or((0.0, 0.0, 0, 0), |s| {
            (
                s.rtt_ms,
                s.server_tick_ms,
                s.predicted_tick,
                s.confirmed_tick,
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visualizer_default() {
        let viz = NetworkVisualizer::default();
        assert!(viz.enabled);
        assert!(viz.show_timeline);
    }

    #[test]
    fn push_samples() {
        let mut viz = NetworkVisualizer::new();

        for i in 0..150 {
            viz.push_sample(NetworkSample {
                timestamp: i as f64 * 0.016,
                rtt_ms: 50.0 + i as f32 % 30.0,
                server_tick_ms: 16.0,
                local_tick_ms: 16.0,
                predicted_tick: i,
                confirmed_tick: i.saturating_sub(5),
            });
        }

        assert_eq!(viz.samples.len(), MAX_HISTORY);
    }

    #[test]
    fn get_status_empty() {
        let viz = NetworkVisualizer::new();
        let (rtt, server, pred, conf) = viz.get_status();
        assert_eq!(rtt, 0.0);
    }

    #[test]
    fn entity_sync() {
        let mut viz = NetworkVisualizer::new();
        viz.add_entity_sync(EntitySyncData {
            entity_id: 42,
            local_position: [0.0, 0.0, 0.0],
            server_position: [0.1, 0.0, 0.0],
            position_error: 0.1,
            is_interpolating: true,
        });

        assert_eq!(viz.entity_sync.len(), 1);
        viz.clear_entity_sync();
        assert!(viz.entity_sync.is_empty());
    }
}
