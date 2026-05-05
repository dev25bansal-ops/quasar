//! Animation Editor Panel
//!
//! Visual editor for creating and editing animation clips with keyframe timeline.
//! Provides UI for:
//! - Animation clip management (create, rename, delete, duplicate)
//! - Keyframe editing (add, remove, modify)
//! - Interpolation preview (Step, Linear, CubicSpline)
//! - Integration with existing AnimationClip and AnimationPlayer systems

use quasar_core::animation::{
    AnimationClip, AnimationPlayer, AnimationResource, AnimationState,
    KeyframeInterpolation, TransformKeyframe,
};
use quasar_math::{Quat, Transform, Vec3};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents an editable animation clip in the editor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorAnimationClip {
    /// Unique identifier for the clip.
    pub id: u64,
    /// The underlying animation clip.
    pub clip: AnimationClip,
    /// Whether this clip is currently selected.
    pub selected: bool,
}

impl EditorAnimationClip {
    pub fn new(id: u64, name: impl Into<String>) -> Self {
        Self {
            id,
            clip: AnimationClip::new(name),
            selected: false,
        }
    }

    pub fn from_clip(id: u64, clip: AnimationClip) -> Self {
        Self {
            id,
            clip,
            selected: false,
        }
    }
}

/// State for the animation editor panel.
pub struct AnimationEditorPanel {
    /// All animation clips in the editor.
    pub clips: Vec<EditorAnimationClip>,
    /// ID counter for generating unique clip IDs.
    pub next_clip_id: u64,
    /// Currently selected clip ID.
    pub selected_clip_id: Option<u64>,
    /// Currently selected keyframe index in the selected clip.
    pub selected_keyframe_index: Option<usize>,
    /// Whether the keyframe property editor is visible.
    pub show_keyframe_editor: bool,
    /// Path for saving/loading animations.
    pub animations_dir: std::path::PathBuf,
    /// Name input buffer for new clips.
    pub new_clip_name: String,
    /// Error message display.
    pub error_message: Option<String>,
    /// Success message display.
    pub success_message: Option<String>,
    /// Preview animation time.
    pub preview_time: f32,
    /// Whether preview is playing.
    pub preview_playing: bool,
    /// Preview speed multiplier.
    pub preview_speed: f32,
}

impl AnimationEditorPanel {
    pub fn new() -> Self {
        Self {
            clips: Vec::new(),
            next_clip_id: 1,
            selected_clip_id: None,
            selected_keyframe_index: None,
            show_keyframe_editor: false,
            animations_dir: std::path::PathBuf::from("assets/animations"),
            new_clip_name: String::from("NewAnimation"),
            error_message: None,
            success_message: None,
            preview_time: 0.0,
            preview_playing: false,
            preview_speed: 1.0,
        }
    }

    /// Set the animations directory path.
    pub fn with_animations_dir(mut self, path: std::path::PathBuf) -> Self {
        self.animations_dir = path;
        self
    }

    /// Load clips from the animations directory.
    pub fn load_clips_from_directory(&mut self) {
        if !self.animations_dir.exists() {
            if let Err(e) = std::fs::create_dir_all(&self.animations_dir) {
                self.error_message = Some(format!("Failed to create animations dir: {}", e));
                return;
            }
        }

        self.clips.clear();
        self.next_clip_id = 1;

        if let Ok(entries) = std::fs::read_dir(&self.animations_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("json") {
                    if let Ok(clip) = AnimationClip::load_from_json(&path) {
                        let id = self.next_clip_id;
                        self.next_clip_id += 1;
                        self.clips.push(EditorAnimationClip::from_clip(id, clip));
                    }
                }
            }
        }

        if !self.clips.is_empty() {
            self.selected_clip_id = Some(self.clips[0].id);
        }
    }

    /// Get the currently selected clip.
    pub fn get_selected_clip(&self) -> Option<&EditorAnimationClip> {
        self.clips.iter().find(|c| Some(c.id) == self.selected_clip_id)
    }

    /// Get the currently selected clip mutably.
    pub fn get_selected_clip_mut(&mut self) -> Option<&mut EditorAnimationClip> {
        self.clips
            .iter_mut()
            .find(|c| Some(c.id) == self.selected_clip_id)
    }

    /// Create a new animation clip.
    pub fn create_clip(&mut self) {
        let name = if self.new_clip_name.is_empty() {
            format!("Animation_{}", self.next_clip_id)
        } else {
            self.new_clip_name.clone()
        };

        // Check for duplicate names
        if self.clips.iter().any(|c| c.clip.name == name) {
            self.error_message = Some(format!("Clip with name '{}' already exists", name));
            return;
        }

        let id = self.next_clip_id;
        self.next_clip_id += 1;
        let clip = EditorAnimationClip::new(id, &name);
        self.clips.push(clip);
        self.selected_clip_id = Some(id);
        self.selected_keyframe_index = None;
        self.success_message = Some(format!("Created clip '{}'", name));
    }

    /// Delete the selected clip.
    pub fn delete_selected_clip(&mut self) {
        if let Some(id) = self.selected_clip_id {
            if let Some(clip) = self.clips.iter().find(|c| c.id == id) {
                let name = clip.clip.name.clone();
                self.clips.retain(|c| c.id != id);
                self.selected_clip_id = self.clips.first().map(|c| c.id);
                self.selected_keyframe_index = None;
                self.success_message = Some(format!("Deleted clip '{}'", name));
            }
        }
    }

    /// Duplicate the selected clip.
    pub fn duplicate_selected_clip(&mut self) {
        if let Some(clip) = self.get_selected_clip() {
            let new_id = self.next_clip_id;
            let clip_name = clip.clip.name.clone();
            let mut new_clip = clip.clone();
            self.next_clip_id += 1;
            new_clip.id = new_id;
            new_clip.clip.name = format!("{}_Copy", clip_name);
            new_clip.selected = false;
            self.clips.push(new_clip);
            self.selected_clip_id = Some(new_id);
            self.success_message = Some(format!("Duplicated clip '{}'", clip_name));
        }
    }

    /// Rename the selected clip.
    pub fn rename_selected_clip(&mut self, new_name: &str) {
        if let Some(clip) = self.get_selected_clip_mut() {
            let old_name = clip.clip.name.clone();
            clip.clip.name = new_name.to_string();
            self.success_message = Some(format!("Renamed '{}' to '{}'", old_name, new_name));
        }
    }

    /// Add a keyframe to the selected clip at the specified time.
    pub fn add_keyframe(&mut self, time: f32, transform: Transform) {
        if let Some(clip) = self.get_selected_clip_mut() {
            let keyframe = TransformKeyframe {
                time,
                position: transform.position,
                rotation: transform.rotation,
                scale: transform.scale,
                interpolation: KeyframeInterpolation::Linear,
            };
            clip.clip = clip.clip.clone().add_keyframe(keyframe);
            self.success_message = Some(format!(
                "Added keyframe at {:.2}s",
                time
            ));
        }
    }

    /// Remove the selected keyframe from the selected clip.
    pub fn remove_selected_keyframe(&mut self) {
        let selected = self.selected_keyframe_index;
        if let Some(clip) = self.get_selected_clip_mut() {
            if let Some(index) = selected {
                if index < clip.clip.keyframes.len() {
                    let time = clip.clip.keyframes[index].time;
                    clip.clip.keyframes.remove(index);
                    self.selected_keyframe_index = None;
                    self.success_message = Some(format!("Removed keyframe at {:.2}s", time));
                }
            }
        }
    }

    /// Update a keyframe's properties.
    pub fn update_keyframe(&mut self, index: usize, keyframe: TransformKeyframe) {
        if let Some(clip) = self.get_selected_clip_mut() {
            if index < clip.clip.keyframes.len() {
                clip.clip.keyframes[index] = keyframe;
                // Re-sort keyframes by time
                clip.clip.keyframes.sort_by(|a, b| {
                    a.time
                        .partial_cmp(&b.time)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                // Update duration
                if let Some(last) = clip.clip.keyframes.last() {
                    clip.clip.duration = clip.clip.duration.max(last.time);
                }
            }
        }
    }

    /// Save the selected clip to JSON.
    pub fn save_selected_clip(&mut self) {
        if let Some(clip) = self.get_selected_clip() {
            let filename = format!("{}.json", clip.clip.name);
            let path = self.animations_dir.join(&filename);
            
            // Ensure directory exists
            if let Err(e) = std::fs::create_dir_all(&self.animations_dir) {
                self.error_message = Some(format!("Failed to create directory: {}", e));
                return;
            }

            if let Err(e) = clip.clip.save_to_json(&path) {
                self.error_message = Some(format!("Failed to save clip: {}", e));
            } else {
                self.success_message = Some(format!("Saved '{}' to {:?}", clip.clip.name, path));
            }
        }
    }

    /// Save all clips to JSON.
    pub fn save_all_clips(&mut self) {
        if let Err(e) = std::fs::create_dir_all(&self.animations_dir) {
            self.error_message = Some(format!("Failed to create directory: {}", e));
            return;
        }

        let mut saved_count = 0;
        for clip in &self.clips {
            let filename = format!("{}.json", clip.clip.name);
            let path = self.animations_dir.join(&filename);
            if clip.clip.save_to_json(&path).is_ok() {
                saved_count += 1;
            }
        }

        if saved_count > 0 {
            self.success_message = Some(format!("Saved {} clips", saved_count));
        }
    }

    /// Load a clip from JSON string.
    pub fn load_clip_from_json_string(&mut self, json: &str) {
        match AnimationClip::from_json_string(json) {
            Ok(clip) => {
                let id = self.next_clip_id;
                self.next_clip_id += 1;
                self.clips.push(EditorAnimationClip::from_clip(id, clip));
                self.selected_clip_id = Some(id);
                self.success_message = Some("Loaded animation from JSON".to_string());
            }
            Err(e) => {
                self.error_message = Some(format!("Failed to load: {}", e));
            }
        }
    }

    /// Update preview animation time.
    pub fn update_preview(&mut self, delta_seconds: f32) {
        if self.preview_playing {
            self.preview_time += delta_seconds * self.preview_speed;
            if let Some(clip) = self.get_selected_clip() {
                if clip.clip.looped {
                    self.preview_time %= clip.clip.duration.max(0.001);
                } else {
                    self.preview_time = self.preview_time.min(clip.clip.duration);
                }
            }
        }
    }

    /// Render the animation editor panel.
    pub fn ui(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("animation_editor")
            .default_width(400.0)
            .resizable(true)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.render_header(ui);
                    ui.separator();
                    self.render_clip_list(ui);
                    ui.separator();
                    self.render_clip_properties(ui);
                    ui.separator();
                    self.render_keyframe_list(ui);
                });
            });

        // Clear messages after a short delay
        if self.error_message.is_some() || self.success_message.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_secs(3));
        }
    }

    fn render_header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("🎬 Animation Editor");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("💾 Save All").clicked() {
                    self.save_all_clips();
                }
                if ui.button("📂 Load").clicked() {
                    self.load_clips_from_directory();
                }
            });
        });

        // Show messages
        if let Some(ref msg) = self.error_message {
            egui::Frame::dark_canvas(ui.style())
                .fill(egui::Color32::DARK_RED)
                .show(ui, |ui| {
                    ui.label(format!("❌ {}", msg));
                });
        }
        if let Some(ref msg) = self.success_message {
            egui::Frame::dark_canvas(ui.style())
                .fill(egui::Color32::DARK_GREEN)
                .show(ui, |ui| {
                    ui.label(format!("✅ {}", msg));
                });
        }
    }

    fn render_clip_list(&mut self, ui: &mut egui::Ui) {
        ui.label("📁 Animation Clips");
        
        // New clip creation
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.new_clip_name)
                    .desired_width(120.0)
                    .hint_text("Clip name"),
            );
            if ui.button("➕ Create").clicked() {
                self.create_clip();
            }
        });

        // Clip list
        egui::Frame::dark_canvas(ui.style()).show(ui, |ui| {
            let mut selected_id = self.selected_clip_id;
            for clip in &mut self.clips {
                let is_selected = Some(clip.id) == selected_id;
                let button = egui::Button::new(if is_selected {
                    format!("▶ {}", clip.clip.name)
                } else {
                    format!("○ {}", clip.clip.name)
                })
                .fill(if is_selected {
                    egui::Color32::DARK_BLUE
                } else {
                    egui::Color32::TRANSPARENT
                });

                if ui.add(button).clicked() {
                    selected_id = Some(clip.id);
                }
            }
            if selected_id != self.selected_clip_id {
                self.selected_clip_id = selected_id;
                self.selected_keyframe_index = None;
            }
        });

        // Clip operations
        if self.get_selected_clip().is_some() {
            ui.horizontal(|ui| {
                if ui.button("🗑️ Delete").clicked() {
                    self.delete_selected_clip();
                }
                if ui.button("📋 Duplicate").clicked() {
                    self.duplicate_selected_clip();
                }
                if ui.button("💾 Save").clicked() {
                    self.save_selected_clip();
                }
            });
        }
    }

    fn render_clip_properties(&mut self, ui: &mut egui::Ui) {
        let mut preview_playing = self.preview_playing;
        let mut preview_time = self.preview_time;
        let mut preview_speed = self.preview_speed;
        let mut selected_keyframe_index = self.selected_keyframe_index;
        let mut show_keyframe_editor = self.show_keyframe_editor;

        if let Some(clip) = self.get_selected_clip_mut() {
            ui.label("📝 Clip Properties");
            
            ui.horizontal(|ui| {
                ui.label("Name:");
                ui.add(egui::TextEdit::singleline(&mut clip.clip.name).desired_width(150.0));
            });

            ui.horizontal(|ui| {
                ui.label("Duration:");
                ui.add(
                    egui::DragValue::new(&mut clip.clip.duration)
                        .speed(0.1)
                        .suffix("s")
                        .clamp_range(0.0..=f32::MAX),
                );
            });

            ui.checkbox(&mut clip.clip.looped, "Looped");

            ui.label(format!("Keyframes: {}", clip.clip.keyframes.len()));

            // Preview controls
            ui.separator();
            ui.label("▶ Preview");
            ui.horizontal(|ui| {
                if ui.button(if preview_playing { "⏸ Pause" } else { "▶ Play" }).clicked() {
                    preview_playing = !preview_playing;
                }
                if ui.button("⏹ Stop").clicked() {
                    preview_playing = false;
                    preview_time = 0.0;
                }
                ui.add(
                    egui::DragValue::new(&mut preview_speed)
                        .speed(0.1)
                        .prefix("Speed: ")
                        .clamp_range(0.1..=5.0),
                );
            });

            ui.horizontal(|ui| {
                ui.label("Time:");
                ui.add(
                    egui::DragValue::new(&mut preview_time)
                        .speed(0.01)
                        .suffix("s")
                        .clamp_range(0.0..=clip.clip.duration.max(0.001)),
                );
            });

            // Sample preview
            if let Some(transform) = clip.clip.sample(preview_time) {
                ui.label("Preview Transform:");
                ui.label(format!("  Position: ({:.2}, {:.2}, {:.2})", 
                    transform.position.x, transform.position.y, transform.position.z));
                ui.label(format!("  Scale: ({:.2}, {:.2}, {:.2})",
                    transform.scale.x, transform.scale.y, transform.scale.z));
            }
        } else {
            ui.label("No clip selected");
        }

        self.preview_playing = preview_playing;
        self.preview_time = preview_time;
        self.preview_speed = preview_speed;
        self.selected_keyframe_index = selected_keyframe_index;
        self.show_keyframe_editor = show_keyframe_editor;
    }

    fn render_keyframe_list(&mut self, ui: &mut egui::Ui) {
        let mut selected_keyframe_index = self.selected_keyframe_index;
        let mut show_keyframe_editor = self.show_keyframe_editor;
        let preview_time = self.preview_time;

        if let Some(clip) = self.get_selected_clip() {
            ui.label("🔑 Keyframes");

            if clip.clip.keyframes.is_empty() {
                ui.label("No keyframes yet. Use the timeline to add keyframes.");
                self.selected_keyframe_index = selected_keyframe_index;
                self.show_keyframe_editor = show_keyframe_editor;
                return;
            }

            egui::ScrollArea::vertical()
                .max_height(200.0)
                .show(ui, |ui| {
                    for (idx, kf) in clip.clip.keyframes.iter().enumerate() {
                        let is_selected = Some(idx) == selected_keyframe_index;
                        
                        egui::Frame::dark_canvas(ui.style())
                            .fill(if is_selected {
                                egui::Color32::DARK_BLUE
                            } else {
                                egui::Color32::TRANSPARENT
                            })
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    if ui.button(format!("K{}", idx)).clicked() {
                                        selected_keyframe_index = if is_selected {
                                            None
                                        } else {
                                            Some(idx)
                                        };
                                        show_keyframe_editor = !is_selected;
                                    }
                                    ui.label(format!("{:.2}s", kf.time));
                                    
                                    // Interpolation indicator
                                    let interp_icon = match kf.interpolation {
                                        KeyframeInterpolation::Step => "▪",
                                        KeyframeInterpolation::Linear => "─",
                                        KeyframeInterpolation::CubicSpline => "◆",
                                    };
                                    ui.small(interp_icon);
                                });
                            });
                    }
                });

            // Keyframe operations
            ui.horizontal(|ui| {
                if ui.button("➕ Add at Current").clicked() {
                    if let Some(clip) = self.get_selected_clip_mut() {
                        let transform = clip.clip.sample(preview_time).unwrap_or(Transform::IDENTITY);
                        let keyframe = TransformKeyframe {
                            time: preview_time,
                            position: transform.position,
                            rotation: transform.rotation,
                            scale: transform.scale,
                            interpolation: KeyframeInterpolation::Linear,
                        };
                        clip.clip = clip.clip.clone().add_keyframe(keyframe);
                    }
                }
                if selected_keyframe_index.is_some() {
                    if ui.button("🗑️ Remove Selected").clicked() {
                        self.remove_selected_keyframe();
                    }
                }
            });
        }

        self.selected_keyframe_index = selected_keyframe_index;
        self.show_keyframe_editor = show_keyframe_editor;

        if self.show_keyframe_editor {
            if let Some(index) = self.selected_keyframe_index {
                let kf_data = self.get_selected_clip().and_then(|clip| {
                    clip.clip.keyframes.get(index).copied()
                });
                if let Some(kf) = kf_data {
                    self.render_keyframe_editor_from_data(ui, kf, index);
                }
            }
        }
    }

    fn render_keyframe_editor(
        &mut self,
        ui: &mut egui::Ui,
        clip: &EditorAnimationClip,
        index: usize,
    ) {
        if index >= clip.clip.keyframes.len() {
            return;
        }

        let kf = clip.clip.keyframes[index];
        self.render_keyframe_editor_from_data(ui, kf, index);
    }

    fn render_keyframe_editor_from_data(
        &mut self,
        ui: &mut egui::Ui,
        kf: TransformKeyframe,
        index: usize,
    ) {
        egui::Frame::dark_canvas(ui.style()).show(ui, |ui| {
            ui.label(format!("Keyframe {} Properties", index));

            let mut time = kf.time;
            ui.horizontal(|ui| {
                ui.label("Time:");
                if ui
                    .add(egui::DragValue::new(&mut time).speed(0.01).suffix("s"))
                    .changed()
                {
                    let mut new_kf = kf;
                    new_kf.time = time;
                    self.update_keyframe(index, new_kf);
                }
            });

            ui.horizontal(|ui| {
                ui.label("Position:");
                let mut pos = kf.position.to_array();
                if ui.add(egui::DragValue::new(&mut pos[0]).speed(0.1)).changed()
                    || ui.add(egui::DragValue::new(&mut pos[1]).speed(0.1)).changed()
                    || ui.add(egui::DragValue::new(&mut pos[2]).speed(0.1)).changed()
                {
                    let mut new_kf = kf;
                    new_kf.position = Vec3::new(pos[0], pos[1], pos[2]);
                    self.update_keyframe(index, new_kf);
                }
            });

            ui.horizontal(|ui| {
                ui.label("Scale:");
                let mut scale = kf.scale.to_array();
                if ui.add(egui::DragValue::new(&mut scale[0]).speed(0.1)).changed()
                    || ui.add(egui::DragValue::new(&mut scale[1]).speed(0.1)).changed()
                    || ui.add(egui::DragValue::new(&mut scale[2]).speed(0.1)).changed()
                {
                    let mut new_kf = kf;
                    new_kf.scale = Vec3::new(scale[0], scale[1], scale[2]);
                    self.update_keyframe(index, new_kf);
                }
            });

            ui.horizontal(|ui| {
                ui.label("Interpolation:");
                let mut interp = kf.interpolation;
                if ui
                    .selectable_label(interp == KeyframeInterpolation::Step, "▪ Step")
                    .clicked()
                {
                    interp = KeyframeInterpolation::Step;
                }
                if ui
                    .selectable_label(interp == KeyframeInterpolation::Linear, "─ Linear")
                    .clicked()
                {
                    interp = KeyframeInterpolation::Linear;
                }
                if ui
                    .selectable_label(interp == KeyframeInterpolation::CubicSpline, "◆ Cubic")
                    .clicked()
                {
                    interp = KeyframeInterpolation::CubicSpline;
                }
                if interp != kf.interpolation {
                    let mut new_kf = kf;
                    new_kf.interpolation = interp;
                    self.update_keyframe(index, new_kf);
                }
            });
        });
    }
}

impl Default for AnimationEditorPanel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_editor_clip() {
        let clip = EditorAnimationClip::new(1, "test");
        assert_eq!(clip.id, 1);
        assert_eq!(clip.clip.name, "test");
        assert!(!clip.selected);
    }

    #[test]
    fn test_animation_editor_panel_new() {
        let panel = AnimationEditorPanel::new();
        assert!(panel.clips.is_empty());
        assert_eq!(panel.next_clip_id, 1);
        assert!(panel.selected_clip_id.is_none());
        assert_eq!(panel.animations_dir, std::path::PathBuf::from("assets/animations"));
    }

    #[test]
    fn test_create_clip() {
        let mut panel = AnimationEditorPanel::new();
        panel.new_clip_name = "TestClip".to_string();
        panel.create_clip();
        
        assert_eq!(panel.clips.len(), 1);
        assert_eq!(panel.clips[0].clip.name, "TestClip");
        assert_eq!(panel.selected_clip_id, Some(1));
        assert_eq!(panel.next_clip_id, 2);
    }

    #[test]
    fn test_create_clip_duplicate_name() {
        let mut panel = AnimationEditorPanel::new();
        panel.new_clip_name = "TestClip".to_string();
        panel.create_clip();
        
        panel.new_clip_name = "TestClip".to_string();
        panel.create_clip();
        
        assert_eq!(panel.clips.len(), 1); // Should not create duplicate
        assert!(panel.error_message.is_some());
    }

    #[test]
    fn test_delete_selected_clip() {
        let mut panel = AnimationEditorPanel::new();
        panel.new_clip_name = "TestClip".to_string();
        panel.create_clip();
        
        panel.delete_selected_clip();
        
        assert!(panel.clips.is_empty());
        assert!(panel.selected_clip_id.is_none());
    }

    #[test]
    fn test_duplicate_selected_clip() {
        let mut panel = AnimationEditorPanel::new();
        panel.new_clip_name = "TestClip".to_string();
        panel.create_clip();
        
        panel.duplicate_selected_clip();
        
        assert_eq!(panel.clips.len(), 2);
        assert_eq!(panel.clips[1].clip.name, "TestClip_Copy");
        assert_eq!(panel.selected_clip_id, Some(2));
    }

    #[test]
    fn test_add_keyframe() {
        let mut panel = AnimationEditorPanel::new();
        panel.new_clip_name = "TestClip".to_string();
        panel.create_clip();
        
        let transform = Transform {
            position: Vec3::new(1.0, 2.0, 3.0),
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        };
        panel.add_keyframe(0.5, transform);
        
        let clip = panel.get_selected_clip().unwrap();
        assert_eq!(clip.clip.keyframes.len(), 1);
        assert_eq!(clip.clip.keyframes[0].time, 0.5);
        assert_eq!(clip.clip.keyframes[0].position, Vec3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn test_remove_selected_keyframe() {
        let mut panel = AnimationEditorPanel::new();
        panel.new_clip_name = "TestClip".to_string();
        panel.create_clip();
        
        panel.add_keyframe(0.5, Transform::IDENTITY);
        panel.selected_keyframe_index = Some(0);
        panel.remove_selected_keyframe();
        
        let clip = panel.get_selected_clip().unwrap();
        assert!(clip.clip.keyframes.is_empty());
        assert!(panel.selected_keyframe_index.is_none());
    }

    #[test]
    fn test_update_keyframe() {
        let mut panel = AnimationEditorPanel::new();
        panel.new_clip_name = "TestClip".to_string();
        panel.create_clip();
        
        panel.add_keyframe(0.5, Transform::IDENTITY);
        
        let new_kf = TransformKeyframe {
            time: 1.0,
            position: Vec3::new(5.0, 0.0, 0.0),
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
            interpolation: KeyframeInterpolation::CubicSpline,
        };
        panel.update_keyframe(0, new_kf);
        
        let clip = panel.get_selected_clip().unwrap();
        assert_eq!(clip.clip.keyframes[0].time, 1.0);
        assert_eq!(clip.clip.keyframes[0].position, Vec3::new(5.0, 0.0, 0.0));
        assert_eq!(clip.clip.keyframes[0].interpolation, KeyframeInterpolation::CubicSpline);
    }

    #[test]
    fn test_clip_json_serialization() {
        let mut panel = AnimationEditorPanel::new();
        panel.new_clip_name = "TestClip".to_string();
        panel.create_clip();
        
        panel.add_keyframe(0.0, Transform::IDENTITY);
        panel.add_keyframe(1.0, Transform {
            position: Vec3::new(10.0, 0.0, 0.0),
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        });
        
        let clip = panel.get_selected_clip().unwrap();
        let json = clip.clip.to_json_string().unwrap();
        assert!(json.contains("TestClip"));
        assert!(json.contains("keyframes"));
        
        let loaded = AnimationClip::from_json_string(&json).unwrap();
        assert_eq!(loaded.name, "TestClip");
        assert_eq!(loaded.keyframes.len(), 2);
    }

    #[test]
    fn test_preview_update() {
        let mut panel = AnimationEditorPanel::new();
        panel.new_clip_name = "TestClip".to_string();
        panel.create_clip();
        panel.add_keyframe(0.0, Transform::IDENTITY);
        panel.add_keyframe(2.0, Transform::IDENTITY);
        
        panel.preview_playing = true;
        panel.preview_speed = 1.0;
        panel.update_preview(0.5);
        
        assert!((panel.preview_time - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_preview_looping() {
        let mut panel = AnimationEditorPanel::new();
        panel.new_clip_name = "TestClip".to_string();
        panel.create_clip();
        panel.add_keyframe(0.0, Transform::IDENTITY);
        panel.add_keyframe(1.0, Transform::IDENTITY);
        
        // Make clip looped
        if let Some(clip) = panel.get_selected_clip_mut() {
            clip.clip.looped = true;
        }
        
        panel.preview_playing = true;
        panel.preview_time = 0.8;
        panel.update_preview(0.5);
        
        // Should have looped back
        assert!(panel.preview_time < 0.5);
    }

    #[test]
    fn test_get_selected_clip_mut() {
        let mut panel = AnimationEditorPanel::new();
        panel.new_clip_name = "TestClip".to_string();
        panel.create_clip();
        
        assert!(panel.get_selected_clip_mut().is_some());
        assert!(panel.get_selected_clip().is_some());
    }

    #[test]
    fn test_rename_selected_clip() {
        let mut panel = AnimationEditorPanel::new();
        panel.new_clip_name = "OldName".to_string();
        panel.create_clip();
        
        panel.rename_selected_clip("NewName");
        
        let clip = panel.get_selected_clip().unwrap();
        assert_eq!(clip.clip.name, "NewName");
    }

    #[test]
    fn test_load_clip_from_json_string() {
        let mut panel = AnimationEditorPanel::new();
        let json = r#"{
            "name": "ImportedClip",
            "duration": 2.0,
            "keyframes": [],
            "looped": true
        }"#;
        
        panel.load_clip_from_json_string(json);
        
        assert_eq!(panel.clips.len(), 1);
        assert_eq!(panel.clips[0].clip.name, "ImportedClip");
    }
}
