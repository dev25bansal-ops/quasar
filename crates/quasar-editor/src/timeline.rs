//! Timeline Widget for Animation Editor
//!
//! Advanced timeline widget with:
//! - Playhead with scrubbing
//! - Zoom controls
//! - Keyframe visualization
//! - Drag-to-retime keyframes
//! - Click-to-select keyframes
//! - Channel tracks for different properties

use glam::FloatExt;
use quasar_core::animation::{KeyframeInterpolation, TransformKeyframe};

/// Represents a track (channel) in the timeline.
#[derive(Debug, Clone)]
pub struct TimelineTrack {
    /// Track name (e.g., "Position X", "Rotation Y", "Scale Z").
    pub name: String,
    /// Keyframe times in seconds.
    pub keyframe_times: Vec<f32>,
    /// Keyframe values (parallel to keyframe_times).
    pub keyframe_values: Vec<f32>,
    /// Interpolation mode per keyframe.
    pub interp_modes: Vec<KeyframeInterpolation>,
    /// Track color for visualization.
    pub color: egui::Color32,
    /// Whether this track is visible.
    pub visible: bool,
}

impl TimelineTrack {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            keyframe_times: Vec::new(),
            keyframe_values: Vec::new(),
            interp_modes: Vec::new(),
            color: egui::Color32::from_rgb(100, 150, 255),
            visible: true,
        }
    }

    pub fn with_color(mut self, color: egui::Color32) -> Self {
        self.color = color;
        self
    }

    /// Add a keyframe to this track.
    pub fn add_keyframe(&mut self, time: f32, value: f32, interp: KeyframeInterpolation) {
        self.keyframe_times.push(time);
        self.keyframe_values.push(value);
        self.interp_modes.push(interp);
        self.sort_by_time();
    }

    /// Remove a keyframe by index.
    pub fn remove_keyframe(&mut self, index: usize) {
        if index < self.keyframe_times.len() {
            self.keyframe_times.remove(index);
            self.keyframe_values.remove(index);
            self.interp_modes.remove(index);
        }
    }

    /// Update a keyframe's time.
    pub fn update_keyframe_time(&mut self, index: usize, time: f32) {
        if index < self.keyframe_times.len() {
            self.keyframe_times[index] = time.max(0.0);
            self.sort_by_time();
        }
    }

    /// Update a keyframe's value.
    pub fn update_keyframe_value(&mut self, index: usize, value: f32) {
        if index < self.keyframe_values.len() {
            self.keyframe_values[index] = value;
        }
    }

    /// Update a keyframe's interpolation mode.
    pub fn update_keyframe_interp(&mut self, index: usize, interp: KeyframeInterpolation) {
        if index < self.interp_modes.len() {
            self.interp_modes[index] = interp;
        }
    }

    /// Sort all keyframes by time.
    fn sort_by_time(&mut self) {
        let mut combined: Vec<_> = self
            .keyframe_times
            .iter()
            .zip(self.keyframe_values.iter())
            .zip(self.interp_modes.iter())
            .map(|((t, v), i)| (*t, *v, *i))
            .collect();

        combined.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

        self.keyframe_times = combined.iter().map(|(t, _, _)| *t).collect();
        self.keyframe_values = combined.iter().map(|(_, v, _)| *v).collect();
        self.interp_modes = combined.iter().map(|(_, _, i)| *i).collect();
    }

    /// Get value at a specific time (interpolated).
    pub fn sample(&self, time: f32) -> Option<f32> {
        if self.keyframe_times.is_empty() {
            return None;
        }

        if self.keyframe_times.len() == 1 {
            return Some(self.keyframe_values[0]);
        }

        // Find the surrounding keyframes
        for i in 0..self.keyframe_times.len() - 1 {
            let t1 = self.keyframe_times[i];
            let t2 = self.keyframe_times[i + 1];

            if time >= t1 && time <= t2 {
                let duration = t2 - t1;
                let t = if duration > 0.0 {
                    (time - t1) / duration
                } else {
                    0.0
                };

                let v1 = self.keyframe_values[i];
                let v2 = self.keyframe_values[i + 1];
                let interp = self.interp_modes[i];

                return Some(match interp {
                    KeyframeInterpolation::Step => v1,
                    KeyframeInterpolation::Linear => v1.lerp(v2, t),
                    KeyframeInterpolation::CubicSpline => {
                        // Hermite interpolation
                        let t2 = t * t;
                        let t3 = t2 * t;
                        let h1 = 2.0 * t3 - 3.0 * t2 + 1.0;
                        let h2 = -2.0 * t3 + 3.0 * t2;
                        h1 * v1 + h2 * v2
                    }
                });
            }
        }

        // Return last value if beyond last keyframe
        Some(*self.keyframe_values.last().unwrap())
    }
}

/// State for the timeline widget.
pub struct TimelineWidget {
    /// Current scrub position in seconds.
    pub scrub_time: f32,
    /// Horizontal zoom (pixels per second).
    pub zoom: f32,
    /// Whether playback is active.
    pub playing: bool,
    /// Playback speed multiplier.
    pub playback_speed: f32,
    /// Timeline duration in seconds.
    pub duration: f32,
    /// Whether the timeline is looped.
    pub looped: bool,
    /// Tracks to display.
    pub tracks: Vec<TimelineTrack>,
    /// Horizontal scroll offset in seconds.
    pub scroll_offset: f32,
    /// Keyframe currently being dragged: (track_idx, keyframe_idx).
    pub dragging_keyframe: Option<(usize, usize)>,
    /// Keyframe currently selected: (track_idx, keyframe_idx).
    pub selected_keyframe: Option<(usize, usize)>,
    /// Whether to show grid lines.
    pub show_grid: bool,
    /// Grid spacing in seconds.
    pub grid_spacing: f32,
    /// Track height in pixels.
    pub track_height: f32,
}

impl TimelineWidget {
    pub fn new() -> Self {
        Self {
            scrub_time: 0.0,
            zoom: 100.0,
            playing: false,
            playback_speed: 1.0,
            duration: 10.0,
            looped: true,
            tracks: Vec::new(),
            scroll_offset: 0.0,
            dragging_keyframe: None,
            selected_keyframe: None,
            show_grid: true,
            grid_spacing: 1.0,
            track_height: 32.0,
        }
    }

    /// Initialize tracks from TransformKeyframes.
    pub fn init_from_keyframes(&mut self, keyframes: &[TransformKeyframe]) {
        self.tracks.clear();

        // Create tracks for each property
        let mut pos_x_track =
            TimelineTrack::new("Position X").with_color(egui::Color32::from_rgb(255, 100, 100));
        let mut pos_y_track =
            TimelineTrack::new("Position Y").with_color(egui::Color32::from_rgb(100, 255, 100));
        let mut pos_z_track =
            TimelineTrack::new("Position Z").with_color(egui::Color32::from_rgb(100, 100, 255));

        let mut rot_x_track =
            TimelineTrack::new("Rotation X").with_color(egui::Color32::from_rgb(255, 200, 100));
        let mut rot_y_track =
            TimelineTrack::new("Rotation Y").with_color(egui::Color32::from_rgb(200, 100, 255));
        let mut rot_z_track =
            TimelineTrack::new("Rotation Z").with_color(egui::Color32::from_rgb(100, 255, 255));

        let mut scale_x_track =
            TimelineTrack::new("Scale X").with_color(egui::Color32::from_rgb(255, 150, 150));
        let mut scale_y_track =
            TimelineTrack::new("Scale Y").with_color(egui::Color32::from_rgb(150, 255, 150));
        let mut scale_z_track =
            TimelineTrack::new("Scale Z").with_color(egui::Color32::from_rgb(150, 150, 255));

        for kf in keyframes {
            pos_x_track.add_keyframe(kf.time, kf.position.x, kf.interpolation);
            pos_y_track.add_keyframe(kf.time, kf.position.y, kf.interpolation);
            pos_z_track.add_keyframe(kf.time, kf.position.z, kf.interpolation);

            rot_x_track.add_keyframe(kf.time, kf.rotation.x, kf.interpolation);
            rot_y_track.add_keyframe(kf.time, kf.rotation.y, kf.interpolation);
            rot_z_track.add_keyframe(kf.time, kf.rotation.z, kf.interpolation);

            scale_x_track.add_keyframe(kf.time, kf.scale.x, kf.interpolation);
            scale_y_track.add_keyframe(kf.time, kf.scale.y, kf.interpolation);
            scale_z_track.add_keyframe(kf.time, kf.scale.z, kf.interpolation);
        }

        self.tracks.push(pos_x_track);
        self.tracks.push(pos_y_track);
        self.tracks.push(pos_z_track);
        self.tracks.push(rot_x_track);
        self.tracks.push(rot_y_track);
        self.tracks.push(rot_z_track);
        self.tracks.push(scale_x_track);
        self.tracks.push(scale_y_track);
        self.tracks.push(scale_z_track);

        // Update duration based on keyframes
        if let Some(max_time) = keyframes
            .iter()
            .map(|kf| kf.time)
            .fold(None::<f32>, |a, b| {
                Some(match a {
                    Some(current) => current.max(b),
                    None => b,
                })
            })
        {
            self.duration = max_time.max(0.001);
            self.scrub_time = self.scrub_time.min(self.duration);
            self.scroll_offset = self.scroll_offset.min(self.duration);
        }
    }

    /// Update the timeline (call each frame during playback).
    pub fn update(&mut self, delta_seconds: f32) {
        if self.playing {
            self.scrub_time += delta_seconds * self.playback_speed;

            if self.looped {
                self.scrub_time %= self.duration.max(0.001);
            } else {
                self.scrub_time = self.scrub_time.min(self.duration);
            }
        }
    }

    /// Render the timeline widget.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        // Playback controls
        self.render_controls(ui);

        ui.separator();

        // Timeline area
        self.render_timeline(ui);
    }

    fn render_controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            // Play/Pause button
            if ui
                .button(if self.playing {
                    "⏸ Pause"
                } else {
                    "▶ Play"
                })
                .clicked()
            {
                self.playing = !self.playing;
            }

            // Stop button
            if ui.button("⏹ Stop").clicked() {
                self.playing = false;
                self.scrub_time = 0.0;
            }

            ui.separator();

            // Time display
            ui.label(format!("⏱ {:.2}s", self.scrub_time));

            // Time scrubber
            ui.add(
                egui::DragValue::new(&mut self.scrub_time)
                    .speed(0.01)
                    .clamp_range(0.0..=self.duration)
                    .prefix("Time: ")
                    .suffix("s"),
            );

            ui.separator();

            // Zoom control
            ui.add(
                egui::Slider::new(&mut self.zoom, 20.0..=500.0)
                    .text("Zoom")
                    .suffix(" px/s"),
            );

            // Playback speed
            ui.add(
                egui::DragValue::new(&mut self.playback_speed)
                    .speed(0.1)
                    .clamp_range(0.1..=5.0)
                    .prefix("Speed: ")
                    .suffix("x"),
            );

            ui.separator();

            // Loop toggle
            ui.checkbox(&mut self.looped, "🔁 Loop");

            // Grid toggle
            ui.checkbox(&mut self.show_grid, "📐 Grid");
        });
    }

    fn render_timeline(&mut self, ui: &mut egui::Ui) {
        let total_height = self.tracks.len() as f32 * self.track_height + 30.0; // +30 for time ruler
        let total_width = (self.duration * self.zoom).max(ui.available_width());

        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(total_width, total_height),
            egui::Sense::click_and_drag(),
        );

        let painter = ui.painter_at(rect);

        // Background
        painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(25, 25, 30));

        // Draw time ruler
        self.render_time_ruler(&painter, rect);

        // Draw grid if enabled
        if self.show_grid {
            self.render_grid(&painter, rect);
        }

        // Draw tracks
        self.render_tracks(&painter, rect);

        // Draw playhead
        self.render_playhead(&painter, rect);

        // Handle interaction
        self.handle_interaction(&response, rect);
    }

    fn render_time_ruler(&self, painter: &egui::Painter, rect: egui::Rect) {
        let ruler_height = 30.0;
        let ruler_rect =
            egui::Rect::from_min_max(rect.min, egui::pos2(rect.max.x, rect.min.y + ruler_height));

        painter.rect_filled(ruler_rect, 0.0, egui::Color32::from_rgb(35, 35, 40));

        // Draw time markers
        let start_time = self.scroll_offset;
        let end_time = self.scroll_offset + rect.width() / self.zoom;

        let mut time = start_time;
        while time <= end_time {
            let x = rect.min.x + (time - self.scroll_offset) * self.zoom;

            // Major marker every second
            painter.line_segment(
                [
                    egui::pos2(x, ruler_rect.max.y - 8.0),
                    egui::pos2(x, ruler_rect.max.y),
                ],
                egui::Stroke::new(1.0, egui::Color32::LIGHT_GRAY),
            );

            painter.text(
                egui::pos2(x + 2.0, ruler_rect.min.y + 2.0),
                egui::Align2::LEFT_TOP,
                &format!("{:.1}", time),
                egui::FontId::monospace(10.0),
                egui::Color32::LIGHT_GRAY,
            );

            time += self.grid_spacing;
        }
    }

    fn render_grid(&self, painter: &egui::Painter, rect: egui::Rect) {
        let ruler_height = 30.0;
        let start_time = self.scroll_offset;
        let end_time = self.scroll_offset + rect.width() / self.zoom;

        let mut time = start_time;
        while time <= end_time {
            let x = rect.min.x + (time - self.scroll_offset) * self.zoom;

            painter.line_segment(
                [
                    egui::pos2(x, rect.min.y + ruler_height),
                    egui::pos2(x, rect.max.y),
                ],
                egui::Stroke::new(0.5, egui::Color32::from_gray(50)),
            );

            time += self.grid_spacing;
        }
    }

    fn render_tracks(&self, painter: &egui::Painter, rect: egui::Rect) {
        let ruler_height = 30.0;

        for (track_idx, track) in self.tracks.iter().enumerate() {
            if !track.visible {
                continue;
            }

            let y = rect.min.y + ruler_height + track_idx as f32 * self.track_height;
            let track_rect = egui::Rect::from_min_max(
                egui::pos2(rect.min.x, y),
                egui::pos2(rect.max.x, y + self.track_height),
            );

            // Track background (alternating colors)
            let bg_color = if track_idx % 2 == 0 {
                egui::Color32::from_rgb(30, 30, 35)
            } else {
                egui::Color32::from_rgb(28, 28, 33)
            };
            painter.rect_filled(track_rect, 0.0, bg_color);

            // Track label
            painter.text(
                egui::pos2(rect.min.x + 4.0, y + 4.0),
                egui::Align2::LEFT_TOP,
                &track.name,
                egui::FontId::monospace(11.0),
                track.color,
            );

            // Draw keyframe diamonds
            for (kf_idx, &t) in track.keyframe_times.iter().enumerate() {
                let x = rect.min.x + (t - self.scroll_offset) * self.zoom;
                if x >= rect.min.x && x <= rect.max.x {
                    let center = egui::pos2(x, y + self.track_height * 0.6);
                    let size = 6.0;

                    let diamond = vec![
                        egui::pos2(center.x, center.y - size),
                        egui::pos2(center.x + size, center.y),
                        egui::pos2(center.x, center.y + size),
                        egui::pos2(center.x - size, center.y),
                    ];

                    let is_selected = self.selected_keyframe == Some((track_idx, kf_idx));
                    let color = if is_selected {
                        egui::Color32::YELLOW
                    } else {
                        track.color
                    };

                    painter.add(egui::Shape::convex_polygon(
                        diamond,
                        color,
                        egui::Stroke::new(1.5, egui::Color32::WHITE),
                    ));

                    // Draw value text below keyframe
                    let value = track.keyframe_values[kf_idx];
                    painter.text(
                        egui::pos2(center.x, center.y + size + 2.0),
                        egui::Align2::CENTER_TOP,
                        &format!("{:.1}", value),
                        egui::FontId::monospace(9.0),
                        egui::Color32::LIGHT_GRAY,
                    );
                }
            }

            // Draw interpolation curves (simplified)
            if track.keyframe_times.len() > 1 {
                self.render_interpolation_curve(painter, track_rect, track);
            }
        }
    }

    fn render_interpolation_curve(
        &self,
        painter: &egui::Painter,
        track_rect: egui::Rect,
        track: &TimelineTrack,
    ) {
        let y_center = track_rect.center().y;
        let value_range = 10.0; // Adjust based on expected value range
        let min_val = track
            .keyframe_values
            .iter()
            .cloned()
            .fold(f32::INFINITY, f32::min);
        let max_val = track
            .keyframe_values
            .iter()
            .cloned()
            .fold(f32::NEG_INFINITY, f32::max);
        let actual_range = (max_val - min_val).max(0.001);

        let mut points = Vec::new();
        let start_time = self.scroll_offset;
        let end_time = self.scroll_offset + track_rect.width() / self.zoom;
        let steps = 50;

        for i in 0..=steps {
            let t = start_time + (end_time - start_time) * (i as f32 / steps as f32);
            if let Some(value) = track.sample(t) {
                let x = track_rect.min.x + (t - self.scroll_offset) * self.zoom;
                let normalized_value = (value - min_val) / actual_range;
                let y = y_center + (0.5 - normalized_value) * self.track_height * 0.4;
                points.push(egui::pos2(x, y));
            }
        }

        if points.len() > 1 {
            painter.add(egui::Shape::line(
                points,
                egui::Stroke::new(1.5, track.color.linear_multiply(0.5)),
            ));
        }
    }

    fn render_playhead(&self, painter: &egui::Painter, rect: egui::Rect) {
        let ruler_height = 30.0;
        let scrub_x = rect.min.x + (self.scrub_time - self.scroll_offset) * self.zoom;

        if scrub_x >= rect.min.x && scrub_x <= rect.max.x {
            // Playhead line
            painter.line_segment(
                [
                    egui::pos2(scrub_x, rect.min.y + ruler_height),
                    egui::pos2(scrub_x, rect.max.y),
                ],
                egui::Stroke::new(2.0, egui::Color32::from_rgb(80, 180, 255)),
            );

            // Playhead handle at top
            let handle_rect = egui::Rect::from_center_size(
                egui::pos2(scrub_x, rect.min.y + ruler_height * 0.5),
                egui::vec2(12.0, ruler_height * 0.8),
            );
            painter.rect_filled(handle_rect, 2.0, egui::Color32::from_rgb(80, 180, 255));

            // Time text on handle
            painter.text(
                handle_rect.center(),
                egui::Align2::CENTER_CENTER,
                &format!("{:.2}", self.scrub_time),
                egui::FontId::monospace(10.0),
                egui::Color32::WHITE,
            );
        }
    }

    fn handle_interaction(&mut self, response: &egui::Response, rect: egui::Rect) {
        if let Some(pos) = response.interact_pointer_pos() {
            // Click to scrub
            if response.clicked() {
                let new_time = self.scroll_offset + (pos.x - rect.min.x) / self.zoom;
                self.scrub_time = new_time.clamp(0.0, self.duration);
                self.selected_keyframe = None;
            }

            // Find and select/drag keyframes
            if response.drag_started() {
                let mut best: Option<(usize, usize, f32)> = None;
                let ruler_height = 30.0;

                for (track_idx, track) in self.tracks.iter().enumerate() {
                    if !track.visible {
                        continue;
                    }

                    let cy = rect.min.y
                        + ruler_height
                        + track_idx as f32 * self.track_height
                        + self.track_height * 0.6;

                    for (kf_idx, &t) in track.keyframe_times.iter().enumerate() {
                        let kx = rect.min.x + (t - self.scroll_offset) * self.zoom;
                        let dist = ((pos.x - kx).powi(2) + (pos.y - cy).powi(2)).sqrt();

                        if dist < 8.0
                            && (best.is_none() || dist < best.as_ref().map_or(f32::MAX, |b| b.2))
                        {
                            best = Some((track_idx, kf_idx, dist));
                        }
                    }
                }

                if let Some((track_idx, kf_idx, _)) = best {
                    self.selected_keyframe = Some((track_idx, kf_idx));
                    self.dragging_keyframe = Some((track_idx, kf_idx));
                }
            }

            // Drag to retime keyframe
            if response.dragged() {
                if let Some((track_idx, kf_idx)) = self.dragging_keyframe {
                    if track_idx < self.tracks.len() {
                        let new_time = self.scroll_offset + (pos.x - rect.min.x) / self.zoom;
                        self.tracks[track_idx].update_keyframe_time(kf_idx, new_time);
                    }
                }
            }
        }

        if response.drag_stopped() {
            self.dragging_keyframe = None;
        }

        // Scroll to pan
        if response.hovered() {
            ui::ctx_from_response(response).input(|i| {
                if i.raw_scroll_delta.y.abs() > 0.0 {
                    // This will be called from the UI thread
                }
            });
        }
    }

    /// Get the currently selected keyframe's data.
    pub fn get_selected_keyframe_data(&self) -> Option<(f32, f32, KeyframeInterpolation)> {
        if let Some((track_idx, kf_idx)) = self.selected_keyframe {
            if track_idx < self.tracks.len() {
                let track = &self.tracks[track_idx];
                if kf_idx < track.keyframe_times.len() {
                    return Some((
                        track.keyframe_times[kf_idx],
                        track.keyframe_values[kf_idx],
                        track.interp_modes[kf_idx],
                    ));
                }
            }
        }
        None
    }

    /// Update the selected keyframe's properties.
    pub fn update_selected_keyframe(
        &mut self,
        time: Option<f32>,
        value: Option<f32>,
        interp: Option<KeyframeInterpolation>,
    ) {
        if let Some((track_idx, kf_idx)) = self.selected_keyframe {
            if track_idx < self.tracks.len() && kf_idx < self.tracks[track_idx].keyframe_times.len()
            {
                if let Some(t) = time {
                    self.tracks[track_idx].update_keyframe_time(kf_idx, t);
                }
                if let Some(v) = value {
                    self.tracks[track_idx].update_keyframe_value(kf_idx, v);
                }
                if let Some(i) = interp {
                    self.tracks[track_idx].update_keyframe_interp(kf_idx, i);
                }
            }
        }
    }
}

impl Default for TimelineWidget {
    fn default() -> Self {
        Self::new()
    }
}

// Helper function to get egui context from response
fn ctx_from_response(response: &egui::Response) -> egui::Context {
    // This is a workaround since we can't access ctx directly from response
    // In practice, the UI code will have access to ctx
    response.ctx.clone()
}

mod ui {
    pub fn ctx_from_response(response: &egui::Response) -> egui::Context {
        response.ctx.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeline_track_new() {
        let track = TimelineTrack::new("Test Track");
        assert_eq!(track.name, "Test Track");
        assert!(track.keyframe_times.is_empty());
        assert!(track.visible);
    }

    #[test]
    fn test_timeline_track_add_keyframe() {
        let mut track = TimelineTrack::new("Test");
        track.add_keyframe(1.0, 10.0, KeyframeInterpolation::Linear);
        track.add_keyframe(0.5, 5.0, KeyframeInterpolation::Step);

        assert_eq!(track.keyframe_times.len(), 2);
        assert_eq!(track.keyframe_times[0], 0.5); // Should be sorted
        assert_eq!(track.keyframe_times[1], 1.0);
    }

    #[test]
    fn test_timeline_track_sample_linear() {
        let mut track = TimelineTrack::new("Test");
        track.add_keyframe(0.0, 0.0, KeyframeInterpolation::Linear);
        track.add_keyframe(1.0, 10.0, KeyframeInterpolation::Linear);

        let value = track.sample(0.5).unwrap();
        assert!((value - 5.0).abs() < 0.01);
    }

    #[test]
    fn test_timeline_track_sample_step() {
        let mut track = TimelineTrack::new("Test");
        track.add_keyframe(0.0, 0.0, KeyframeInterpolation::Step);
        track.add_keyframe(1.0, 10.0, KeyframeInterpolation::Step);

        let value = track.sample(0.5).unwrap();
        assert!((value - 0.0).abs() < 0.01); // Should hold first value
    }

    #[test]
    fn test_timeline_track_sample_cubic() {
        let mut track = TimelineTrack::new("Test");
        track.add_keyframe(0.0, 0.0, KeyframeInterpolation::CubicSpline);
        track.add_keyframe(1.0, 10.0, KeyframeInterpolation::CubicSpline);

        let value = track.sample(0.5).unwrap();
        // Cubic spline with 2 points should give similar result to linear
        assert!((value - 5.0).abs() < 1.0);
    }

    #[test]
    fn test_timeline_widget_new() {
        let widget = TimelineWidget::new();
        assert_eq!(widget.scrub_time, 0.0);
        assert_eq!(widget.zoom, 100.0);
        assert!(!widget.playing);
        assert!(widget.looped);
    }

    #[test]
    fn test_timeline_update() {
        let mut widget = TimelineWidget::new();
        widget.playing = true;
        widget.playback_speed = 1.0;
        widget.duration = 10.0;

        widget.update(0.5);
        assert!((widget.scrub_time - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_timeline_update_loop() {
        let mut widget = TimelineWidget::new();
        widget.playing = true;
        widget.playback_speed = 1.0;
        widget.duration = 1.0;
        widget.scrub_time = 0.8;

        widget.update(0.5);
        // Should have wrapped around
        assert!(widget.scrub_time < 0.5);
    }

    #[test]
    fn test_timeline_init_from_keyframes() {
        let mut widget = TimelineWidget::new();
        let keyframes = vec![
            TransformKeyframe {
                time: 0.0,
                position: quasar_math::Vec3::ZERO,
                rotation: quasar_math::Quat::IDENTITY,
                scale: quasar_math::Vec3::ONE,
                interpolation: KeyframeInterpolation::Linear,
            },
            TransformKeyframe {
                time: 1.0,
                position: quasar_math::Vec3::new(10.0, 0.0, 0.0),
                rotation: quasar_math::Quat::IDENTITY,
                scale: quasar_math::Vec3::ONE,
                interpolation: KeyframeInterpolation::Linear,
            },
        ];

        widget.init_from_keyframes(&keyframes);
        assert_eq!(widget.tracks.len(), 9); // 3 properties × 3 axes
        assert_eq!(widget.duration, 1.0);
    }

    #[test]
    fn test_timeline_get_selected_keyframe_data() {
        let mut widget = TimelineWidget::new();
        let mut track = TimelineTrack::new("Test");
        track.add_keyframe(1.0, 5.0, KeyframeInterpolation::Linear);
        widget.tracks.push(track);

        widget.selected_keyframe = Some((0, 0));
        let data = widget.get_selected_keyframe_data().unwrap();
        assert_eq!(data.0, 1.0);
        assert_eq!(data.1, 5.0);
        assert_eq!(data.2, KeyframeInterpolation::Linear);
    }

    #[test]
    fn test_timeline_update_selected_keyframe() {
        let mut widget = TimelineWidget::new();
        let mut track = TimelineTrack::new("Test");
        track.add_keyframe(1.0, 5.0, KeyframeInterpolation::Linear);
        widget.tracks.push(track);
        widget.selected_keyframe = Some((0, 0));

        widget.update_selected_keyframe(Some(2.0), Some(10.0), Some(KeyframeInterpolation::Step));

        let data = widget.get_selected_keyframe_data().unwrap();
        assert_eq!(data.0, 2.0);
        assert_eq!(data.1, 10.0);
        assert_eq!(data.2, KeyframeInterpolation::Step);
    }
}
