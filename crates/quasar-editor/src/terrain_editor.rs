//! Terrain editor panel — integrates heightmap editing, texture splatting,
//! foliage painting, and material configuration into a unified egui UI.
//!
//! Provides:
//! - Toolbar with brush type selection
//! - Heightmap editor with brush preview
//! - Texture splatting editor
//! - Foliage painting system
//! - Terrain creation and save/load
//! - Real-time settings panel

use egui::{Color32, Sense, Ui, Vec2};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::brush_tools::{
    apply_brush_heightmap, apply_brush_splatmap, BrushSettings, BrushStroke, BrushType,
    FalloffType, HeightmapSnapshot, SplatmapSnapshot,
};
use crate::foliage_editor::{foliage_editor_ui, FoliageEditorState};
use crate::splat_editor::BlendMode as EditorBlendMode;
use crate::splat_editor::{
    splat_editor_ui, SplatEditorState, SplatmapData, TerrainMaterial as EditorMaterial,
};

// Re-export terrain types from quasar-render
use quasar_render::terrain::{
    TerrainBlendMode as RenderMaterialBlendMode, TerrainConfig, TerrainData,
    TerrainMaterial as RenderMaterial,
};

// ---------------------------------------------------------------------------
// Editor Mode
// ---------------------------------------------------------------------------

/// Which sub-editor is currently active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TerrainEditorMode {
    Heightmap,
    Splatting,
    Foliage,
    Materials,
    Settings,
}

impl TerrainEditorMode {
    pub fn display_name(&self) -> &'static str {
        match self {
            TerrainEditorMode::Heightmap => "Heightmap",
            TerrainEditorMode::Splatting => "Splatting",
            TerrainEditorMode::Foliage => "Foliage",
            TerrainEditorMode::Materials => "Materials",
            TerrainEditorMode::Settings => "Settings",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            TerrainEditorMode::Heightmap => "⛰️",
            TerrainEditorMode::Splatting => "🎨",
            TerrainEditorMode::Foliage => "🌲",
            TerrainEditorMode::Materials => "🧱",
            TerrainEditorMode::Settings => "⚙️",
        }
    }
}

// ---------------------------------------------------------------------------
// Brush Tool Selection
// ---------------------------------------------------------------------------

/// Quick-select brush tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickBrush {
    Raise,
    Lower,
    Smooth,
    Flatten,
    Paint,
    Foliage,
    EraseFoliage,
}

impl QuickBrush {
    pub fn icon(&self) -> &'static str {
        match self {
            QuickBrush::Raise => "⬆️",
            QuickBrush::Lower => "⬇️",
            QuickBrush::Smooth => "〰️",
            QuickBrush::Flatten => "📏",
            QuickBrush::Paint => "🖌️",
            QuickBrush::Foliage => "🌱",
            QuickBrush::EraseFoliage => "🧹",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            QuickBrush::Raise => "Raise",
            QuickBrush::Lower => "Lower",
            QuickBrush::Smooth => "Smooth",
            QuickBrush::Flatten => "Flatten",
            QuickBrush::Paint => "Paint",
            QuickBrush::Foliage => "Foliage",
            QuickBrush::EraseFoliage => "Erase",
        }
    }
}

// ---------------------------------------------------------------------------
// Terrain Editor State
// ---------------------------------------------------------------------------

/// Main terrain editor state.
pub struct TerrainEditor {
    /// Current editor mode (sub-panel).
    pub mode: TerrainEditorMode,
    /// Currently active quick brush.
    pub active_brush: QuickBrush,
    /// Global brush settings.
    pub brush_settings: BrushSettings,
    /// Splat editor state.
    pub splat_editor: SplatEditorState,
    /// Foliage editor state.
    pub foliage_editor: FoliageEditorState,
    /// Undo history for brush strokes.
    pub undo_stack: Vec<BrushStroke>,
    /// Redo stack.
    pub redo_stack: Vec<BrushStroke>,
    /// Maximum undo history size.
    pub max_undo: usize,
    /// Whether the brush is currently active (mouse down).
    pub brush_active: bool,
    /// Current brush grid position (x, z).
    pub brush_grid_x: f32,
    pub brush_grid_z: f32,
    /// Show brush preview radius.
    pub show_brush_preview: bool,
    /// Flatten target height.
    pub flatten_height: f32,
    /// Terrain file path for save/load.
    pub current_file: Option<PathBuf>,
    /// Whether the terrain has unsaved changes.
    pub dirty: bool,
    /// Status message.
    pub status_message: String,
    /// New terrain creation settings.
    pub new_terrain_resolution: u32,
    pub new_terrain_width: f32,
    pub new_terrain_depth: f32,
    pub new_terrain_max_height: f32,
    pub new_terrain_name: String,
}

impl TerrainEditor {
    pub fn new() -> Self {
        Self {
            mode: TerrainEditorMode::Heightmap,
            active_brush: QuickBrush::Raise,
            brush_settings: BrushSettings::raise(0.5, 5.0),
            splat_editor: SplatEditorState::new(),
            foliage_editor: FoliageEditorState::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_undo: 50,
            brush_active: false,
            brush_grid_x: 0.0,
            brush_grid_z: 0.0,
            show_brush_preview: true,
            flatten_height: 0.0,
            current_file: None,
            dirty: false,
            status_message: String::from("Ready"),
            new_terrain_resolution: 256,
            new_terrain_width: 500.0,
            new_terrain_depth: 500.0,
            new_terrain_max_height: 100.0,
            new_terrain_name: String::from("New Terrain"),
        }
    }

    // -----------------------------------------------------------------------
    // Terrain Data Management
    // -----------------------------------------------------------------------

    /// Create a new terrain and replace the current one.
    pub fn create_new_terrain(&mut self) -> TerrainData {
        let data = TerrainData::new(
            &self.new_terrain_name,
            self.new_terrain_resolution,
            self.new_terrain_width,
            self.new_terrain_depth,
            self.new_terrain_max_height,
        );
        self.current_file = None;
        self.dirty = false;
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.status_message = format!(
            "Created new terrain: {} ({}x{}, {}m height)",
            data.name, data.resolution, data.resolution, data.max_height
        );
        data
    }

    /// Save terrain to the current file or prompt for a path.
    pub fn save_terrain(
        &mut self,
        data: &TerrainData,
        path: Option<&PathBuf>,
    ) -> Result<(), String> {
        let save_path = path
            .or(self.current_file.as_ref())
            .cloned()
            .ok_or("No file path specified")?;

        data.save_json(&save_path)?;
        self.current_file = Some(save_path.clone());
        self.dirty = false;
        self.status_message = format!("Saved terrain to {:?}", save_path);
        Ok(())
    }

    /// Load terrain from a file.
    pub fn load_terrain(&mut self, path: &PathBuf) -> Result<TerrainData, String> {
        let data = TerrainData::load_json(path)?;
        self.current_file = Some(path.clone());
        self.dirty = false;
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.status_message = format!("Loaded terrain from {:?}", path);
        Ok(data)
    }

    /// Export heightmap as raw binary.
    pub fn export_heightmap(&self, data: &TerrainData, path: &PathBuf) -> Result<(), String> {
        data.save_heightmap_raw(path)
    }

    // -----------------------------------------------------------------------
    // Brush Application
    // -----------------------------------------------------------------------

    /// Apply the current brush to the terrain heightmap.
    pub fn apply_brush_to_terrain(&mut self, data: &mut TerrainData, grid_x: f32, grid_z: f32) {
        // Save snapshot for undo
        let stroke = BrushStroke::new(
            self.brush_settings.clone(),
            grid_x,
            grid_z,
            Some(&data.heightmap),
            Some(&data.splatmap),
        );

        match &self.active_brush {
            QuickBrush::Raise | QuickBrush::Lower | QuickBrush::Smooth | QuickBrush::Flatten => {
                let mut settings = self.brush_settings.clone();
                // Sync brush type with quick brush selection
                settings.brush_type = match self.active_brush {
                    QuickBrush::Raise => BrushType::Raise {
                        strength: settings.strength,
                    },
                    QuickBrush::Lower => BrushType::Lower {
                        strength: settings.strength,
                    },
                    QuickBrush::Smooth => BrushType::Smooth {
                        strength: settings.strength,
                    },
                    QuickBrush::Flatten => BrushType::Flatten {
                        height: self.flatten_height,
                    },
                    _ => settings.brush_type,
                };

                apply_brush_heightmap(
                    &mut data.heightmap,
                    data.resolution,
                    grid_x,
                    grid_z,
                    &settings,
                );
            }
            QuickBrush::Paint => {
                apply_brush_splatmap(
                    &mut data.splatmap,
                    data.resolution,
                    grid_x,
                    grid_z,
                    &self.brush_settings,
                );
                data.normalize_splatmap();
            }
            QuickBrush::Foliage => {
                // Foliage is handled by the foliage editor
                self.foliage_editor.paint_foliage(
                    &data.heightmap,
                    data.resolution,
                    data.width,
                    data.depth,
                    grid_x as u32,
                    grid_z as u32,
                );
            }
            QuickBrush::EraseFoliage => {
                self.foliage_editor.erase_foliage(
                    &data.heightmap,
                    data.resolution,
                    data.width,
                    data.depth,
                    grid_x as u32,
                    grid_z as u32,
                    self.brush_settings.radius,
                );
            }
        }

        // Push undo state
        if self.undo_stack.len() >= self.max_undo {
            self.undo_stack.remove(0);
        }
        self.undo_stack.push(stroke);
        self.redo_stack.clear();
        data.touch();
        self.dirty = true;
    }

    /// Undo the last brush stroke.
    pub fn undo(&mut self, data: &mut TerrainData) {
        if let Some(stroke) = self.undo_stack.pop() {
            // Restore previous state
            if let Some(snapshot) = &stroke.previous_heightmap {
                data.heightmap = snapshot.data.clone();
            }
            if let Some(snapshot) = &stroke.previous_splatmap {
                data.splatmap = snapshot.data.clone();
                data.normalize_splatmap();
            }

            self.redo_stack.push(stroke);
            self.dirty = true;
            self.status_message = "Undid last stroke".to_string();
        }
    }

    /// Redo the last undone brush stroke.
    pub fn redo(&mut self, data: &mut TerrainData) {
        if let Some(stroke) = self.redo_stack.pop() {
            // Re-apply the brush
            match &stroke.settings.brush_type {
                BrushType::Raise { .. }
                | BrushType::Lower { .. }
                | BrushType::Smooth { .. }
                | BrushType::Flatten { .. } => {
                    // Save current state for re-undo
                    let redo_snapshot = BrushStroke::new(
                        stroke.settings.clone(),
                        stroke.center_x,
                        stroke.center_z,
                        Some(&data.heightmap),
                        Some(&data.splatmap),
                    );

                    apply_brush_heightmap(
                        &mut data.heightmap,
                        data.resolution,
                        stroke.center_x,
                        stroke.center_z,
                        &stroke.settings,
                    );

                    self.undo_stack.push(redo_snapshot);
                }
                BrushType::Paint { .. } => {
                    let redo_snapshot = BrushStroke::new(
                        stroke.settings.clone(),
                        stroke.center_x,
                        stroke.center_z,
                        Some(&data.heightmap),
                        Some(&data.splatmap),
                    );

                    apply_brush_splatmap(
                        &mut data.splatmap,
                        data.resolution,
                        stroke.center_x,
                        stroke.center_z,
                        &stroke.settings,
                    );
                    data.normalize_splatmap();

                    self.undo_stack.push(redo_snapshot);
                }
                _ => {}
            }

            self.dirty = true;
            self.status_message = "Redid last stroke".to_string();
        }
    }

    /// Check if undo is available.
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Check if redo is available.
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Clear all undo/redo history.
    pub fn clear_history(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    // -----------------------------------------------------------------------
    // UI
    // -----------------------------------------------------------------------

    /// Render the full terrain editor panel.
    pub fn ui(&mut self, ui: &mut Ui, data: &mut TerrainData) {
        ui.heading("Terrain Editor");
        ui.separator();

        // Quick info bar
        ui.horizontal(|ui| {
            ui.label(format!("{}x{}", data.resolution, data.resolution));
            ui.label("|");
            ui.label(format!("{}m x {}m", data.width, data.depth));
            ui.label("|");
            ui.label(format!("Foliage: {}", self.foliage_editor.instance_count()));
            if self.dirty {
                ui.label(egui::RichText::new("*").color(Color32::YELLOW));
            }
        });

        ui.separator();

        // Mode tabs
        ui.horizontal(|ui| {
            for mode in &[
                TerrainEditorMode::Heightmap,
                TerrainEditorMode::Splatting,
                TerrainEditorMode::Foliage,
                TerrainEditorMode::Materials,
                TerrainEditorMode::Settings,
            ] {
                let selected = self.mode == *mode;
                let label = format!("{} {}", mode.icon(), mode.display_name());
                if ui.selectable_label(selected, label).clicked() {
                    self.mode = *mode;
                }
            }
        });

        ui.separator();

        // Sub-panel based on mode
        match self.mode {
            TerrainEditorMode::Heightmap => {
                self.heightmap_editor_ui(ui, data);
            }
            TerrainEditorMode::Splatting => {
                splat_editor_ui(ui, &mut self.splat_editor, None);
                // Sync splat editor materials with terrain data
                self.sync_splat_materials(data);
            }
            TerrainEditorMode::Foliage => {
                foliage_editor_ui(ui, &mut self.foliage_editor);
            }
            TerrainEditorMode::Materials => {
                self.materials_editor_ui(ui, data);
            }
            TerrainEditorMode::Settings => {
                self.settings_ui(ui, data);
            }
        }

        ui.separator();

        // Status bar
        ui.small(&self.status_message);
    }

    /// Heightmap editing sub-panel.
    fn heightmap_editor_ui(&mut self, ui: &mut Ui, data: &TerrainData) {
        ui.label("Heightmap Editing");
        ui.separator();

        // Brush selection
        ui.label("Brush:");
        ui.horizontal(|ui| {
            for brush in &[
                QuickBrush::Raise,
                QuickBrush::Lower,
                QuickBrush::Smooth,
                QuickBrush::Flatten,
            ] {
                let selected = self.active_brush == *brush;
                let label = format!("{} {}", brush.icon(), brush.label());
                if ui.selectable_label(selected, label).clicked() {
                    self.active_brush = *brush;
                    self.update_brush_type();
                }
            }
        });

        ui.separator();

        // Brush parameters
        ui.horizontal(|ui| {
            ui.label("Radius:");
            ui.add(
                egui::Slider::new(&mut self.brush_settings.radius, 1.0..=50.0).text("Brush Radius"),
            );
        });

        ui.horizontal(|ui| {
            ui.label("Strength:");
            ui.add(
                egui::Slider::new(&mut self.brush_settings.strength, 0.01..=1.0)
                    .text("Brush Strength"),
            );
        });

        if self.active_brush == QuickBrush::Flatten {
            ui.horizontal(|ui| {
                ui.label("Target Height:");
                ui.add(
                    egui::DragValue::new(&mut self.flatten_height)
                        .speed(0.5)
                        .range(0.0..=data.max_height),
                );
            });
        }

        ui.horizontal(|ui| {
            ui.label("Falloff:");
            let mut falloff_name = format!("{:?}", self.brush_settings.falloff);
            egui::ComboBox::from_id_salt("terrain_falloff")
                .selected_text(&falloff_name)
                .show_ui(ui, |ui| {
                    for falloff in [
                        FalloffType::Linear,
                        FalloffType::Smooth,
                        FalloffType::Sharp,
                        FalloffType::Gaussian,
                    ] {
                        let name = format!("{:?}", falloff);
                        if ui.selectable_label(falloff_name == name, &name).clicked() {
                            self.brush_settings.falloff = falloff;
                            falloff_name = name;
                        }
                    }
                });
        });

        ui.checkbox(&mut self.show_brush_preview, "Show Brush Preview");

        ui.separator();

        // Undo/Redo
        ui.horizontal(|ui| {
            if ui
                .add_enabled(self.can_undo(), egui::Button::new("↶ Undo"))
                .clicked()
            {
                // undo called externally
            }
            if ui
                .add_enabled(self.can_redo(), egui::Button::new("↷ Redo"))
                .clicked()
            {
                // redo called externally
            }
            ui.label(format!(
                "History: {} undo, {} redo",
                self.undo_stack.len(),
                self.redo_stack.len()
            ));
        });

        ui.separator();

        // Heightmap statistics
        let min_h = data.heightmap.iter().cloned().fold(f32::INFINITY, f32::min);
        let max_h = data
            .heightmap
            .iter()
            .cloned()
            .fold(f32::NEG_INFINITY, f32::max);
        let avg_h = if !data.heightmap.is_empty() {
            data.heightmap.iter().sum::<f32>() / data.heightmap.len() as f32
        } else {
            0.0
        };

        ui.label("Heightmap Statistics:");
        ui.label(format!(
            "  Min: {:.2} ({:.2}m)",
            min_h,
            min_h * data.max_height
        ));
        ui.label(format!(
            "  Max: {:.2} ({:.2}m)",
            max_h,
            max_h * data.max_height
        ));
        ui.label(format!(
            "  Avg: {:.2} ({:.2}m)",
            avg_h,
            avg_h * data.max_height
        ));
    }

    /// Materials editor sub-panel.
    fn materials_editor_ui(&mut self, ui: &mut Ui, data: &mut TerrainData) {
        ui.label("Terrain Materials");
        ui.separator();

        // Sync editor materials with terrain data
        while self.splat_editor.materials.len() < data.materials.len() {
            if let Some(m) = data.materials.get(self.splat_editor.materials.len()) {
                self.splat_editor.materials.push(EditorMaterial {
                    name: m.name.clone(),
                    texture_albedo: m.texture_albedo.clone(),
                    texture_normal: m.texture_normal.clone(),
                    texture_roughness: m.texture_roughness.clone(),
                    texture_height: m.texture_height.clone(),
                    tiling: [m.tiling.x, m.tiling.y],
                    blend_mode: crate::splat_editor::BlendMode::Linear,
                    editor_color: [
                        (self.splat_editor.materials.len() as f32 * 0.25).fract(),
                        0.5,
                        0.5,
                        1.0,
                    ],
                });
            }
        }

        splat_editor_ui(ui, &mut self.splat_editor, None);
    }

    /// Settings sub-panel.
    fn settings_ui(&mut self, ui: &mut Ui, data: &mut TerrainData) {
        ui.label("Terrain Settings");
        ui.separator();

        ui.label("Terrain Info:");
        ui.label(format!("  Name: {}", data.name));
        ui.label(format!(
            "  Resolution: {}x{}",
            data.resolution, data.resolution
        ));
        ui.label(format!("  Size: {}m x {}m", data.width, data.depth));
        ui.label(format!("  Max Height: {}m", data.max_height));
        ui.label(format!("  Created: {}", data.created_at));
        ui.label(format!("  Modified: {}", data.modified_at));

        ui.separator();

        // Save/Load
        ui.label("File Operations:");
        ui.horizontal(|ui| {
            if ui.button("Save").clicked() {
                if let Some(path) = &self.current_file {
                    if let Err(e) = self.save_terrain(data, None) {
                        self.status_message = format!("Save failed: {}", e);
                    }
                } else {
                    self.status_message = "Use 'Save As' to specify a file".to_string();
                }
            }
            if ui.button("Save As...").clicked() {
                // In a real editor this would open a file dialog
                self.status_message = "File dialog not available in headless mode".to_string();
            }
        });

        ui.horizontal(|ui| {
            if ui.button("Load...").clicked() {
                self.status_message = "Use file dialog to load a terrain".to_string();
            }
            if ui.button("Export Heightmap").clicked() {
                self.status_message = "Export requires a file path".to_string();
            }
        });

        ui.separator();

        // New terrain
        ui.label("New Terrain:");
        ui.horizontal(|ui| {
            ui.label("Name:");
            ui.text_edit_singleline(&mut self.new_terrain_name);
        });
        ui.horizontal(|ui| {
            ui.label("Resolution:");
            egui::ComboBox::from_id_salt("new_res")
                .selected_text(format!("{}", self.new_terrain_resolution))
                .show_ui(ui, |ui| {
                    for &res in &[64, 128, 256, 512, 1024, 2048] {
                        if ui
                            .selectable_label(
                                self.new_terrain_resolution == res,
                                format!("{}", res),
                            )
                            .clicked()
                        {
                            self.new_terrain_resolution = res;
                        }
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label("Width:");
            ui.add(
                egui::DragValue::new(&mut self.new_terrain_width)
                    .speed(10.0)
                    .range(10.0..=10000.0),
            );
            ui.label("m");
        });
        ui.horizontal(|ui| {
            ui.label("Depth:");
            ui.add(
                egui::DragValue::new(&mut self.new_terrain_depth)
                    .speed(10.0)
                    .range(10.0..=10000.0),
            );
            ui.label("m");
        });
        ui.horizontal(|ui| {
            ui.label("Max Height:");
            ui.add(
                egui::DragValue::new(&mut self.new_terrain_max_height)
                    .speed(5.0)
                    .range(1.0..=1000.0),
            );
            ui.label("m");
        });

        if ui.button("Create New Terrain").clicked() {
            let _new_data = self.create_new_terrain();
            // In practice, the caller would replace the active terrain
        }

        ui.separator();

        // Undo history settings
        ui.horizontal(|ui| {
            ui.label("Max Undo History:");
            ui.add(
                egui::DragValue::new(&mut self.max_undo)
                    .speed(1)
                    .range(5..=200),
            );
        });

        if ui.button("Clear History").clicked() {
            self.clear_history();
            self.status_message = "Undo history cleared".to_string();
        }
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Sync the active brush type with the current quick brush selection.
    fn update_brush_type(&mut self) {
        let strength = self.brush_settings.strength;
        self.brush_settings.brush_type = match self.active_brush {
            QuickBrush::Raise => BrushType::Raise { strength },
            QuickBrush::Lower => BrushType::Lower { strength },
            QuickBrush::Smooth => BrushType::Smooth { strength },
            QuickBrush::Flatten => BrushType::Flatten {
                height: self.flatten_height,
            },
            QuickBrush::Paint => BrushType::Paint {
                texture_index: self.splat_editor.selected_material as u32,
                strength,
            },
            QuickBrush::Foliage => BrushType::Foliage {
                foliage_type: self.foliage_editor.selected_type as u32,
                density: strength,
            },
            QuickBrush::EraseFoliage => BrushType::EraseFoliage {
                radius: self.brush_settings.radius,
            },
        };
    }

    /// Sync splat editor materials from terrain data.
    fn sync_splat_materials(&mut self, data: &TerrainData) {
        // Only sync if counts differ
        if self.splat_editor.materials.len() != data.materials.len() {
            self.splat_editor.materials = data
                .materials
                .iter()
                .map(|m| EditorMaterial {
                    name: m.name.clone(),
                    texture_albedo: m.texture_albedo.clone(),
                    texture_normal: m.texture_normal.clone(),
                    texture_roughness: m.texture_roughness.clone(),
                    texture_height: m.texture_height.clone(),
                    tiling: [m.tiling.x, m.tiling.y],
                    blend_mode: match m.blend_mode {
                        RenderMaterialBlendMode::Linear => EditorBlendMode::Linear,
                        RenderMaterialBlendMode::HeightBased => EditorBlendMode::HeightBased,
                        RenderMaterialBlendMode::Triplanar => EditorBlendMode::Triplanar,
                        RenderMaterialBlendMode::NormalBased => EditorBlendMode::NormalBased,
                    },
                    editor_color: [0.5, 0.5, 0.5, 1.0],
                })
                .collect();
        }
    }
}

impl Default for TerrainEditor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_creation() {
        let editor = TerrainEditor::new();
        assert_eq!(editor.mode, TerrainEditorMode::Heightmap);
        assert_eq!(editor.active_brush, QuickBrush::Raise);
        assert_eq!(editor.undo_stack.len(), 0);
        assert!(!editor.dirty);
    }

    #[test]
    fn create_new_terrain() {
        let mut editor = TerrainEditor::new();
        editor.new_terrain_name = "Test".to_string();
        editor.new_terrain_resolution = 64;
        editor.new_terrain_width = 100.0;
        editor.new_terrain_depth = 100.0;
        editor.new_terrain_max_height = 50.0;

        let data = editor.create_new_terrain();
        assert_eq!(data.name, "Test");
        assert_eq!(data.resolution, 64);
        assert_eq!(data.heightmap.len(), 64 * 64);
        assert_eq!(data.splatmap.len(), 64 * 64);
    }

    #[test]
    fn brush_apply_creates_undo() {
        let mut editor = TerrainEditor::new();
        let mut data = TerrainData::new("Test", 32, 100.0, 100.0, 50.0);
        editor.active_brush = QuickBrush::Raise;
        editor.brush_settings.strength = 0.1;
        editor.brush_settings.radius = 3.0;

        editor.apply_brush_to_terrain(&mut data, 16.0, 16.0);

        assert!(editor.can_undo());
        assert!(editor.dirty);
        assert_eq!(editor.undo_stack.len(), 1);
    }

    #[test]
    fn undo_restores_heightmap() {
        let mut editor = TerrainEditor::new();
        let mut data = TerrainData::new("Test", 32, 100.0, 100.0, 50.0);
        editor.active_brush = QuickBrush::Raise;
        editor.brush_settings.strength = 0.5;
        editor.brush_settings.radius = 3.0;

        let original = data.heightmap.clone();
        editor.apply_brush_to_terrain(&mut data, 16.0, 16.0);
        assert_ne!(data.heightmap, original);

        editor.undo(&mut data);
        assert_eq!(data.heightmap, original);
    }

    #[test]
    fn undo_redo_stack_flow() {
        let mut editor = TerrainEditor::new();
        let mut data = TerrainData::new("Test", 32, 100.0, 100.0, 50.0);
        editor.active_brush = QuickBrush::Raise;
        editor.brush_settings.strength = 0.1;
        editor.brush_settings.radius = 2.0;

        editor.apply_brush_to_terrain(&mut data, 10.0, 10.0);
        editor.apply_brush_to_terrain(&mut data, 20.0, 20.0);
        assert_eq!(editor.undo_stack.len(), 2);

        editor.undo(&mut data);
        assert_eq!(editor.undo_stack.len(), 1);
        assert!(editor.can_redo());

        editor.undo(&mut data);
        assert_eq!(editor.undo_stack.len(), 0);
        assert!(editor.can_redo());
    }

    #[test]
    fn max_undo_limit() {
        let mut editor = TerrainEditor::new();
        editor.max_undo = 3;
        let mut data = TerrainData::new("Test", 32, 100.0, 100.0, 50.0);
        editor.active_brush = QuickBrush::Raise;
        editor.brush_settings.strength = 0.01;
        editor.brush_settings.radius = 1.0;

        for i in 0..5 {
            editor.apply_brush_to_terrain(&mut data, i as f32, i as f32);
        }

        assert!(editor.undo_stack.len() <= 3);
    }
}
