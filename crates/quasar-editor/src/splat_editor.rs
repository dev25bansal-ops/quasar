//! Texture splatting editor for terrain materials.
//!
//! Provides:
//! - Multi-layer texture splatmap editing
//! - Material blending configuration
//! - Splatmap visualization in the editor
//! - Save/load splatmap data

use egui::{Color32, Image, Response, Sense, TextureHandle, TextureOptions, Ui, Vec2};
use serde::{Deserialize, Serialize};

use crate::brush_tools::{apply_brush_splatmap, BrushSettings, FalloffType};

// ---------------------------------------------------------------------------
// Blend Modes
// ---------------------------------------------------------------------------

/// Blend mode for terrain material layers.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum BlendMode {
    /// Linear interpolation based on splatmap weights.
    #[default]
    Linear,
    /// Height-based blending using texture height maps.
    HeightBased,
    /// Triplanar projection for seamless tiling.
    Triplanar,
    /// Normal-based blending for micro-detail variation.
    NormalBased,
}

impl BlendMode {
    pub fn display_name(&self) -> &'static str {
        match self {
            BlendMode::Linear => "Linear",
            BlendMode::HeightBased => "Height Based",
            BlendMode::Triplanar => "Triplanar",
            BlendMode::NormalBased => "Normal Based",
        }
    }

    pub fn all() -> &'static [BlendMode] {
        &[
            BlendMode::Linear,
            BlendMode::HeightBased,
            BlendMode::Triplanar,
            BlendMode::NormalBased,
        ]
    }
}

// ---------------------------------------------------------------------------
// Terrain Material
// ---------------------------------------------------------------------------

/// A single terrain material layer descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerrainMaterial {
    /// Human-readable name (e.g., "Grass", "Rock", "Sand").
    pub name: String,
    /// Path to the albedo/diffuse texture.
    pub texture_albedo: String,
    /// Path to the normal map texture.
    pub texture_normal: String,
    /// Path to the roughness texture.
    pub texture_roughness: String,
    /// Path to the height/displacement texture.
    pub texture_height: String,
    /// Texture tiling scale (u, v).
    pub tiling: [f32; 2],
    /// How this layer blends with others.
    pub blend_mode: BlendMode,
    /// Display color for the editor UI.
    pub editor_color: [f32; 4],
}

impl TerrainMaterial {
    /// Create a new material with default empty paths.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            texture_albedo: String::new(),
            texture_normal: String::new(),
            texture_roughness: String::new(),
            texture_height: String::new(),
            tiling: [1.0, 1.0],
            blend_mode: BlendMode::default(),
            editor_color: [1.0, 1.0, 1.0, 1.0],
        }
    }

    /// Create a grass material preset.
    pub fn grass() -> Self {
        Self {
            name: "Grass".to_string(),
            texture_albedo: "textures/terrain/grass_albedo.png".to_string(),
            texture_normal: "textures/terrain/grass_normal.png".to_string(),
            texture_roughness: "textures/terrain/grass_roughness.png".to_string(),
            texture_height: "textures/terrain/grass_height.png".to_string(),
            tiling: [10.0, 10.0],
            blend_mode: BlendMode::HeightBased,
            editor_color: [0.2, 0.6, 0.1, 1.0],
        }
    }

    /// Create a rock material preset.
    pub fn rock() -> Self {
        Self {
            name: "Rock".to_string(),
            texture_albedo: "textures/terrain/rock_albedo.png".to_string(),
            texture_normal: "textures/terrain/rock_normal.png".to_string(),
            texture_roughness: "textures/terrain/rock_roughness.png".to_string(),
            texture_height: "textures/terrain/rock_height.png".to_string(),
            tiling: [5.0, 5.0],
            blend_mode: BlendMode::HeightBased,
            editor_color: [0.4, 0.4, 0.4, 1.0],
        }
    }

    /// Create a sand material preset.
    pub fn sand() -> Self {
        Self {
            name: "Sand".to_string(),
            texture_albedo: "textures/terrain/sand_albedo.png".to_string(),
            texture_normal: "textures/terrain/sand_normal.png".to_string(),
            texture_roughness: "textures/terrain/sand_roughness.png".to_string(),
            texture_height: "textures/terrain/sand_height.png".to_string(),
            tiling: [8.0, 8.0],
            blend_mode: BlendMode::Linear,
            editor_color: [0.76, 0.7, 0.5, 1.0],
        }
    }

    /// Create a snow material preset.
    pub fn snow() -> Self {
        Self {
            name: "Snow".to_string(),
            texture_albedo: "textures/terrain/snow_albedo.png".to_string(),
            texture_normal: "textures/terrain/snow_normal.png".to_string(),
            texture_roughness: "textures/terrain/snow_roughness.png".to_string(),
            texture_height: "textures/terrain/snow_height.png".to_string(),
            tiling: [6.0, 6.0],
            blend_mode: BlendMode::HeightBased,
            editor_color: [0.95, 0.95, 0.98, 1.0],
        }
    }
}

// ---------------------------------------------------------------------------
// Splatmap Data
// ---------------------------------------------------------------------------

/// A splatmap layer holding texture weights for a terrain region.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplatmapData {
    /// Resolution of the splatmap (square).
    pub resolution: u32,
    /// RGBA weight data, one per vertex. Each channel corresponds to
    /// a material layer. Weights should sum to 1.0.
    pub weights: Vec<[f32; 4]>,
}

impl SplatmapData {
    /// Create a new uniform splatmap with all weight on layer 0.
    pub fn new(resolution: u32, num_layers: usize) -> Self {
        let total = (resolution * resolution) as usize;
        let mut weights = vec![[0.0f32; 4]; total];

        // Put all weight on the first available layer
        let first_layer = num_layers.min(4).saturating_sub(1);
        for w in weights.iter_mut() {
            w[first_layer] = 1.0;
        }

        Self {
            resolution,
            weights,
        }
    }

    /// Create a uniform splatmap with all weight on layer 0.
    pub fn uniform(resolution: u32) -> Self {
        Self {
            resolution,
            weights: vec![[1.0, 0.0, 0.0, 0.0]; (resolution * resolution) as usize],
        }
    }

    /// Get the weight for a specific vertex.
    pub fn get_weight(&self, x: u32, z: u32, channel: usize) -> f32 {
        if x < self.resolution && z < self.resolution && channel < 4 {
            let idx = (z * self.resolution + x) as usize;
            self.weights[idx][channel]
        } else {
            0.0
        }
    }

    /// Set the weight for a specific vertex.
    pub fn set_weight(&mut self, x: u32, z: u32, channel: usize, value: f32) {
        if x < self.resolution && z < self.resolution && channel < 4 {
            let idx = (z * self.resolution + x) as usize;
            self.weights[idx][channel] = value.clamp(0.0, 1.0);
        }
    }

    /// Normalize all weights so each vertex's channels sum to 1.0.
    pub fn normalize(&mut self) {
        for w in self.weights.iter_mut() {
            let sum: f32 = w.iter().sum();
            if sum > 0.0001 {
                for c in w.iter_mut() {
                    *c /= sum;
                }
            } else {
                w[0] = 1.0;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Splat Editor State
// ---------------------------------------------------------------------------

/// State for the texture splatting editor.
pub struct SplatEditorState {
    /// Available terrain materials.
    pub materials: Vec<TerrainMaterial>,
    /// Currently selected material index for painting.
    pub selected_material: usize,
    /// Brush settings for splat painting.
    pub brush_settings: BrushSettings,
    /// Whether to show the splatmap preview overlay.
    pub show_splatmap_preview: bool,
    /// Which channel to visualize in the preview (0-3).
    pub preview_channel: usize,
    /// Auto-normalize weights after each stroke.
    pub auto_normalize: bool,
}

impl SplatEditorState {
    pub fn new() -> Self {
        Self {
            materials: vec![
                TerrainMaterial::grass(),
                TerrainMaterial::rock(),
                TerrainMaterial::sand(),
                TerrainMaterial::snow(),
            ],
            selected_material: 0,
            brush_settings: BrushSettings {
                brush_type: crate::brush_tools::BrushType::Paint {
                    texture_index: 0,
                    strength: 0.5,
                },
                radius: 5.0,
                strength: 0.5,
                falloff: FalloffType::Smooth,
            },
            show_splatmap_preview: false,
            preview_channel: 0,
            auto_normalize: true,
        }
    }

    /// Apply the current brush to the splatmap.
    pub fn apply_brush(&self, splatmap: &mut SplatmapData, center_x: f32, center_z: f32) {
        apply_brush_splatmap(
            &mut splatmap.weights,
            splatmap.resolution,
            center_x,
            center_z,
            &self.brush_settings,
        );

        if self.auto_normalize {
            splatmap.normalize();
        }
    }

    /// Add a new material to the list.
    pub fn add_material(&mut self, name: &str) {
        self.materials.push(TerrainMaterial::new(name));
    }

    /// Remove a material by index.
    pub fn remove_material(&mut self, index: usize) {
        if index < self.materials.len() {
            self.materials.remove(index);
            if self.selected_material >= self.materials.len() {
                self.selected_material = self.materials.len().saturating_sub(1);
            }
        }
    }

    /// Update the selected material index on the brush.
    pub fn update_brush_channel(&mut self) {
        if let crate::brush_tools::BrushType::Paint {
            ref mut texture_index,
            strength,
        } = self.brush_settings.brush_type
        {
            *texture_index = self.selected_material as u32;
            self.brush_settings.strength = strength;
        }
    }
}

impl Default for SplatEditorState {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Splat Editor UI
// ---------------------------------------------------------------------------

/// Render the splat editor panel.
pub fn splat_editor_ui(ui: &mut Ui, state: &mut SplatEditorState, splatmap: Option<&SplatmapData>) {
    ui.heading("Texture Splatting");
    ui.separator();

    // Material list
    ui.label("Materials:");
    egui::ScrollArea::vertical()
        .max_height(200.0)
        .show(ui, |ui| {
            let mut action = None;
            for (i, mat) in state.materials.iter().enumerate() {
                let is_selected = i == state.selected_material;
                let color = Color32::from_rgba_premultiplied(
                    (mat.editor_color[0] * 255.0) as u8,
                    (mat.editor_color[1] * 255.0) as u8,
                    (mat.editor_color[2] * 255.0) as u8,
                    (mat.editor_color[3] * 255.0) as u8,
                );

                ui.horizontal(|ui| {
                    // Color swatch
                    let (rect, resp) =
                        ui.allocate_exact_size(Vec2::new(20.0, 20.0), Sense::click());
                    ui.painter().rect_filled(rect, 2.0, color);
                    if resp.clicked() {
                        action = Some(("select", i));
                    }

                    // Material name (selectable)
                    let btn = if is_selected {
                        ui.selectable_label(true, &mat.name)
                    } else {
                        ui.selectable_label(false, &mat.name)
                    };
                    if btn.clicked() {
                        action = Some(("select", i));
                    }

                    // Delete button (not for first material)
                    if i > 0 && ui.small_button("-").clicked() {
                        action = Some(("remove", i));
                    }
                });
            }
            if let Some((act, i)) = action {
                if act == "select" {
                    state.selected_material = i;
                    state.update_brush_channel();
                } else if act == "remove" {
                    state.remove_material(i);
                }
            }
        });

    ui.horizontal(|ui| {
        if ui.button("+ Add Material").clicked() {
            let idx = state.materials.len() + 1;
            state.add_material(&format!("Layer {}", idx));
        }
    });

    ui.separator();

    // Selected material properties
    if let Some(mat) = state.materials.get_mut(state.selected_material) {
        ui.label(format!("Editing: {}", mat.name));

        ui.horizontal(|ui| {
            ui.label("Blend Mode:");
            let mut blend_name = mat.blend_mode.display_name();
            egui::ComboBox::from_id_salt("blend_mode")
                .selected_text(blend_name)
                .show_ui(ui, |ui| {
                    for &mode in BlendMode::all() {
                        let name = mode.display_name();
                        if ui.selectable_label(blend_name == name, name).clicked() {
                            mat.blend_mode = mode;
                            blend_name = name;
                        }
                    }
                });
        });

        ui.horizontal(|ui| {
            ui.label("Tiling U:");
            ui.add(
                egui::DragValue::new(&mut mat.tiling[0])
                    .speed(0.5)
                    .range(0.1..=100.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Tiling V:");
            ui.add(
                egui::DragValue::new(&mut mat.tiling[1])
                    .speed(0.5)
                    .range(0.1..=100.0),
            );
        });

        ui.separator();
        ui.label("Texture Paths:");

        ui.horizontal(|ui| {
            ui.label("Albedo:");
            ui.text_edit_singleline(&mut mat.texture_albedo);
        });
        ui.horizontal(|ui| {
            ui.label("Normal:");
            ui.text_edit_singleline(&mut mat.texture_normal);
        });
        ui.horizontal(|ui| {
            ui.label("Roughness:");
            ui.text_edit_singleline(&mut mat.texture_roughness);
        });
        ui.horizontal(|ui| {
            ui.label("Height:");
            ui.text_edit_singleline(&mut mat.texture_height);
        });

        ui.separator();
        ui.label("Editor Color:");
        ui.color_edit_button_rgba_unmultiplied(&mut mat.editor_color);
    }

    ui.separator();

    // Brush settings
    ui.label("Paint Brush Settings:");
    ui.horizontal(|ui| {
        ui.label("Radius:");
        ui.add(
            egui::Slider::new(&mut state.brush_settings.radius, 1.0..=50.0).text("Brush Radius"),
        );
    });
    ui.horizontal(|ui| {
        ui.label("Strength:");
        ui.add(
            egui::Slider::new(&mut state.brush_settings.strength, 0.01..=1.0)
                .text("Paint Strength"),
        );
    });

    ui.horizontal(|ui| {
        ui.label("Falloff:");
        let mut falloff_name = format!("{:?}", state.brush_settings.falloff);
        egui::ComboBox::from_id_salt("splat_falloff")
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
                        state.brush_settings.falloff = falloff;
                        falloff_name = name;
                    }
                }
            });
    });

    ui.checkbox(&mut state.auto_normalize, "Auto-normalize weights");

    ui.separator();

    // Preview options
    ui.checkbox(&mut state.show_splatmap_preview, "Show Splatmap Preview");
    if state.show_splatmap_preview {
        ui.horizontal(|ui| {
            ui.label("Preview Channel:");
            if ui.radio_value(&mut state.preview_channel, 0, "R").clicked() {}
            if ui.radio_value(&mut state.preview_channel, 1, "G").clicked() {}
            if ui.radio_value(&mut state.preview_channel, 2, "B").clicked() {}
            if ui.radio_value(&mut state.preview_channel, 3, "A").clicked() {}
        });
    }

    // Splatmap preview render
    if state.show_splatmap_preview {
        if let Some(splatmap) = splatmap {
            render_splatmap_preview(ui, splatmap, state.preview_channel);
        }
    }
}

/// Render a small preview of the splatmap channel.
fn render_splatmap_preview(ui: &mut Ui, splatmap: &SplatmapData, channel: usize) {
    // Downsample for display
    let display_res = 128u32;
    let mut pixels = vec![0u8; (display_res * display_res * 4) as usize];

    let scale_x = splatmap.resolution as f32 / display_res as f32;
    let scale_z = splatmap.resolution as f32 / display_res as f32;

    for dz in 0..display_res {
        for dx in 0..display_res {
            let sx = (dx as f32 * scale_x) as u32;
            let sz = (dz as f32 * scale_z) as u32;
            let weight = splatmap.get_weight(sx, sz, channel);
            let idx = ((dz * display_res + dx) * 4) as usize;

            let intensity = (weight * 255.0) as u8;
            pixels[idx] = intensity;
            pixels[idx + 1] = intensity;
            pixels[idx + 2] = intensity;
            pixels[idx + 3] = 255;
        }
    }

    let image = egui::ColorImage::from_rgba_unmultiplied(
        [display_res as usize, display_res as usize],
        &pixels,
    );

    let texture_handle: TextureHandle =
        ui.ctx()
            .load_texture("splat_preview", image, TextureOptions::default());

    ui.image(&texture_handle);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splatmap_uniform() {
        let sm = SplatmapData::uniform(32);
        assert_eq!(sm.weights.len(), 32 * 32);
        assert!((sm.weights[0][0] - 1.0).abs() < 0.001);
    }

    #[test]
    fn splatmap_normalize() {
        let mut sm = SplatmapData {
            resolution: 4,
            weights: vec![[2.0, 3.0, 0.0, 0.0]; 16],
        };
        sm.normalize();
        assert!((sm.weights[0][0] - 0.4).abs() < 0.001);
        assert!((sm.weights[0][1] - 0.6).abs() < 0.001);
    }

    #[test]
    fn material_presets() {
        let grass = TerrainMaterial::grass();
        assert_eq!(grass.name, "Grass");
        assert!(!grass.texture_albedo.is_empty());

        let rock = TerrainMaterial::rock();
        assert_eq!(rock.name, "Rock");

        let sand = TerrainMaterial::sand();
        assert_eq!(sand.name, "Sand");

        let snow = TerrainMaterial::snow();
        assert_eq!(snow.name, "Snow");
    }

    #[test]
    fn add_remove_materials() {
        let mut state = SplatEditorState::new();
        let initial_count = state.materials.len();
        state.add_material("Mud");
        assert_eq!(state.materials.len(), initial_count + 1);
        state.remove_material(state.materials.len() - 1);
        assert_eq!(state.materials.len(), initial_count);
    }

    #[test]
    fn brush_channel_update() {
        let mut state = SplatEditorState::new();
        state.selected_material = 2;
        state.update_brush_channel();

        if let crate::brush_tools::BrushType::Paint { texture_index, .. } =
            &state.brush_settings.brush_type
        {
            assert_eq!(*texture_index, 2);
        } else {
            panic!("Expected Paint brush type");
        }
    }
}
