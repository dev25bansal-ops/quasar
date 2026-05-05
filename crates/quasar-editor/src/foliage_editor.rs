//! Foliage painting system with density controls.
//!
//! Provides:
//! - Foliage type definitions (grass, trees, bushes, flowers)
//! - Density-based foliage painting
//! - Foliage instance management
//! - Erase and scatter tools
//! - LOD configuration per foliage type

use egui::{Ui, Vec2};
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};

use crate::brush_tools::{BrushSettings, BrushType, FalloffType};

// ---------------------------------------------------------------------------
// Foliage Types
// ---------------------------------------------------------------------------

/// Type of foliage instance.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum FoliageKind {
    /// Low grass and ground cover.
    Grass,
    /// Small flowering plants.
    Flowers,
    /// Bushes and shrubs.
    Bush,
    /// Small trees.
    SmallTree,
    /// Large trees.
    LargeTree,
    /// Rocks and stones.
    Rock,
}

impl FoliageKind {
    pub fn display_name(&self) -> &'static str {
        match self {
            FoliageKind::Grass => "Grass",
            FoliageKind::Flowers => "Flowers",
            FoliageKind::Bush => "Bush",
            FoliageKind::SmallTree => "Small Tree",
            FoliageKind::LargeTree => "Large Tree",
            FoliageKind::Rock => "Rock",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            FoliageKind::Grass => "🌿",
            FoliageKind::Flowers => "🌸",
            FoliageKind::Bush => "🌳",
            FoliageKind::SmallTree => "🌲",
            FoliageKind::LargeTree => "🌴",
            FoliageKind::Rock => "🪨",
        }
    }

    pub fn all() -> &'static [FoliageKind] {
        &[
            FoliageKind::Grass,
            FoliageKind::Flowers,
            FoliageKind::Bush,
            FoliageKind::SmallTree,
            FoliageKind::LargeTree,
            FoliageKind::Rock,
        ]
    }
}

// ---------------------------------------------------------------------------
// Foliage Type Definition
// ---------------------------------------------------------------------------

/// Configuration for a specific foliage type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FoliageTypeDef {
    /// The kind of foliage.
    pub kind: FoliageKind,
    /// Display name.
    pub name: String,
    /// Path to the mesh asset.
    pub mesh_path: String,
    /// Path to the material asset.
    pub material_path: String,
    /// Minimum scale factor.
    pub scale_min: f32,
    /// Maximum scale factor.
    pub scale_max: f32,
    /// Random rotation range (degrees).
    pub random_rotation: f32,
    /// Color tint variation (RGBA range).
    pub color_variation: [f32; 4],
    /// Minimum slope angle (degrees) this foliage can be placed on.
    pub min_slope: f32,
    /// Maximum slope angle (degrees) this foliage can be placed on.
    pub max_slope: f32,
    /// Minimum height (normalized) for placement.
    pub min_height: f32,
    /// Maximum height (normalized) for placement.
    pub max_height: f32,
    /// LOD distances (near, mid, far).
    pub lod_distances: [f32; 3],
    /// Use billboarding at distance.
    pub billboard: bool,
    /// Wind animation weight.
    pub wind_weight: f32,
}

impl FoliageTypeDef {
    /// Create a grass foliage definition.
    pub fn grass() -> Self {
        Self {
            kind: FoliageKind::Grass,
            name: "Grass".to_string(),
            mesh_path: "meshes/foliage/grass_patch.glb".to_string(),
            material_path: "materials/grass.material".to_string(),
            scale_min: 0.8,
            scale_max: 1.5,
            random_rotation: 360.0,
            color_variation: [0.1, 0.15, 0.0, 0.0],
            min_slope: 0.0,
            max_slope: 35.0,
            min_height: 0.1,
            max_height: 0.8,
            lod_distances: [30.0, 80.0, 200.0],
            billboard: true,
            wind_weight: 1.0,
        }
    }

    /// Create a flowers foliage definition.
    pub fn flowers() -> Self {
        Self {
            kind: FoliageKind::Flowers,
            name: "Wildflowers".to_string(),
            mesh_path: "meshes/foliage/flowers.glb".to_string(),
            material_path: "materials/flowers.material".to_string(),
            scale_min: 0.6,
            scale_max: 1.2,
            random_rotation: 360.0,
            color_variation: [0.2, 0.2, 0.2, 0.0],
            min_slope: 0.0,
            max_slope: 25.0,
            min_height: 0.15,
            max_height: 0.7,
            lod_distances: [25.0, 60.0, 150.0],
            billboard: true,
            wind_weight: 0.8,
        }
    }

    /// Create a bush foliage definition.
    pub fn bush() -> Self {
        Self {
            kind: FoliageKind::Bush,
            name: "Bush".to_string(),
            mesh_path: "meshes/foliage/bush.glb".to_string(),
            material_path: "materials/bush.material".to_string(),
            scale_min: 0.7,
            scale_max: 1.8,
            random_rotation: 360.0,
            color_variation: [0.1, 0.1, 0.05, 0.0],
            min_slope: 0.0,
            max_slope: 40.0,
            min_height: 0.1,
            max_height: 0.75,
            lod_distances: [40.0, 100.0, 250.0],
            billboard: false,
            wind_weight: 0.5,
        }
    }

    /// Create a small tree foliage definition.
    pub fn small_tree() -> Self {
        Self {
            kind: FoliageKind::SmallTree,
            name: "Small Tree".to_string(),
            mesh_path: "meshes/foliage/small_tree.glb".to_string(),
            material_path: "materials/small_tree.material".to_string(),
            scale_min: 0.8,
            scale_max: 1.3,
            random_rotation: 360.0,
            color_variation: [0.08, 0.08, 0.05, 0.0],
            min_slope: 0.0,
            max_slope: 30.0,
            min_height: 0.15,
            max_height: 0.65,
            lod_distances: [50.0, 120.0, 300.0],
            billboard: true,
            wind_weight: 0.6,
        }
    }

    /// Create a large tree foliage definition.
    pub fn large_tree() -> Self {
        Self {
            kind: FoliageKind::LargeTree,
            name: "Large Tree".to_string(),
            mesh_path: "meshes/foliage/large_tree.glb".to_string(),
            material_path: "materials/large_tree.material".to_string(),
            scale_min: 0.9,
            scale_max: 1.4,
            random_rotation: 360.0,
            color_variation: [0.06, 0.06, 0.04, 0.0],
            min_slope: 0.0,
            max_slope: 25.0,
            min_height: 0.1,
            max_height: 0.55,
            lod_distances: [60.0, 150.0, 400.0],
            billboard: true,
            wind_weight: 0.4,
        }
    }

    /// Create a rock foliage definition.
    pub fn rock() -> Self {
        Self {
            kind: FoliageKind::Rock,
            name: "Rock".to_string(),
            mesh_path: "meshes/foliage/rock.glb".to_string(),
            material_path: "materials/rock.material".to_string(),
            scale_min: 0.5,
            scale_max: 2.0,
            random_rotation: 360.0,
            color_variation: [0.05, 0.05, 0.05, 0.0],
            min_slope: 0.0,
            max_slope: 60.0,
            min_height: 0.05,
            max_height: 0.9,
            lod_distances: [40.0, 100.0, 250.0],
            billboard: false,
            wind_weight: 0.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Foliage Instances
// ---------------------------------------------------------------------------

/// A single foliage instance placed on the terrain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FoliageInstance {
    /// World X position.
    pub x: f32,
    /// World Y position (height).
    pub y: f32,
    /// World Z position.
    pub z: f32,
    /// Rotation in degrees around Y axis.
    pub rotation_deg: f32,
    /// Uniform scale factor.
    pub scale: f32,
    /// Which foliage type this instance uses.
    pub foliage_type: u32,
    /// Color tint (RGBA, additive to base material).
    pub color_tint: [f32; 4],
    /// LOD level override (0 = force highest, -1 = auto).
    pub lod_override: i32,
}

impl FoliageInstance {
    /// Get position as [x, y, z].
    pub fn position(&self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }
}

// ---------------------------------------------------------------------------
// Foliage Density Map
// ---------------------------------------------------------------------------

/// A density map for placing foliage. Each cell holds a density value [0, 1].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FoliageDensityMap {
    /// Resolution of the density grid (square).
    pub resolution: u32,
    /// Density values per cell.
    pub densities: Vec<f32>,
}

impl FoliageDensityMap {
    /// Create a new empty density map.
    pub fn new(resolution: u32) -> Self {
        Self {
            resolution,
            densities: vec![0.0f32; (resolution * resolution) as usize],
        }
    }

    /// Get density at grid coordinates.
    pub fn get(&self, x: u32, z: u32) -> f32 {
        if x < self.resolution && z < self.resolution {
            self.densities[(z * self.resolution + x) as usize]
        } else {
            0.0
        }
    }

    /// Set density at grid coordinates.
    pub fn set(&mut self, x: u32, z: u32, value: f32) {
        if x < self.resolution && z < self.resolution {
            self.densities[(z * self.resolution + x) as usize] = value.clamp(0.0, 1.0);
        }
    }

    /// Apply a brush to the density map.
    pub fn apply_brush(&mut self, center_x: f32, center_z: f32, settings: &BrushSettings) {
        let radius_cells = settings.radius;
        let radius_sq = radius_cells * radius_cells;

        let min_x = (center_x - radius_cells).max(0.0).floor() as u32;
        let max_x = (center_x + radius_cells)
            .min(self.resolution as f32 - 1.0)
            .ceil() as u32;
        let min_z = (center_z - radius_cells).max(0.0).floor() as u32;
        let max_z = (center_z + radius_cells)
            .min(self.resolution as f32 - 1.0)
            .ceil() as u32;

        for z in min_z..=max_z {
            for x in min_x..=max_x {
                let dx = x as f32 - center_x;
                let dz = z as f32 - center_z;
                let dist_sq = dx * dx + dz * dz;
                if dist_sq > radius_sq {
                    continue;
                }
                let dist = dist_sq.sqrt();
                let t = dist / radius_sq.sqrt();
                let influence = settings.falloff.evaluate(t) * settings.strength;

                let idx = (z * self.resolution + x) as usize;
                if idx < self.densities.len() {
                    // For foliage brush, increase density
                    if let BrushType::Foliage { density, .. } = &settings.brush_type {
                        self.densities[idx] = (self.densities[idx] + density * influence).min(1.0);
                    }
                    // For erase, decrease density
                    else if let BrushType::EraseFoliage { .. } = &settings.brush_type {
                        self.densities[idx] = (self.densities[idx] - influence).max(0.0);
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Foliage Editor State
// ---------------------------------------------------------------------------

/// State for the foliage editor panel.
pub struct FoliageEditorState {
    /// Available foliage type definitions.
    pub foliage_types: Vec<FoliageTypeDef>,
    /// Currently selected foliage type index.
    pub selected_type: usize,
    /// Brush settings for foliage painting.
    pub brush_settings: BrushSettings,
    /// Density maps per foliage type.
    pub density_maps: Vec<FoliageDensityMap>,
    /// Actual foliage instances (generated from density maps).
    pub instances: Vec<FoliageInstance>,
    /// Maximum number of instances.
    pub max_instances: u32,
    /// Whether to show instances in the viewport.
    pub show_instances: bool,
    /// Whether to show the density map overlay.
    pub show_density_overlay: bool,
    /// Density overlay opacity.
    pub density_overlay_opacity: f32,
    /// Which density map channel to visualize.
    pub visualize_density_type: usize,
    /// Auto-regenerate instances on density change.
    pub auto_regenerate: bool,
    /// Random seed for instance generation.
    pub generation_seed: u64,
}

impl FoliageEditorState {
    pub fn new() -> Self {
        let foliage_types = vec![
            FoliageTypeDef::grass(),
            FoliageTypeDef::flowers(),
            FoliageTypeDef::bush(),
            FoliageTypeDef::small_tree(),
            FoliageTypeDef::large_tree(),
            FoliageTypeDef::rock(),
        ];

        let density_maps = vec![FoliageDensityMap::new(128); foliage_types.len()];

        Self {
            foliage_types,
            selected_type: 0,
            brush_settings: BrushSettings::foliage(0, 0.5, 5.0),
            density_maps,
            instances: Vec::new(),
            max_instances: 100_000,
            show_instances: true,
            show_density_overlay: false,
            density_overlay_opacity: 0.3,
            visualize_density_type: 0,
            auto_regenerate: true,
            generation_seed: 42,
        }
    }

    /// Paint foliage using the current brush.
    pub fn paint_foliage(
        &mut self,
        heightmap: &[f32],
        terrain_resolution: u32,
        terrain_width: f32,
        terrain_depth: f32,
        center_grid_x: u32,
        center_grid_z: u32,
    ) {
        // Apply brush to density map
        if self.selected_type < self.density_maps.len() {
            let grid_x = center_grid_x as f32
                * self.density_maps[self.selected_type].resolution as f32
                / terrain_resolution as f32;
            let grid_z = center_grid_z as f32
                * self.density_maps[self.selected_type].resolution as f32
                / terrain_resolution as f32;

            self.density_maps[self.selected_type].apply_brush(grid_x, grid_z, &self.brush_settings);
        }

        // Regenerate instances
        if self.auto_regenerate {
            self.regenerate_instances(heightmap, terrain_resolution, terrain_width, terrain_depth);
        }
    }

    /// Erase foliage using the erase brush.
    pub fn erase_foliage(
        &mut self,
        heightmap: &[f32],
        terrain_resolution: u32,
        terrain_width: f32,
        terrain_depth: f32,
        center_grid_x: u32,
        center_grid_z: u32,
        radius: f32,
    ) {
        let world_cx = center_grid_x as f32 / terrain_resolution as f32 * terrain_width;
        let world_cz = center_grid_z as f32 / terrain_resolution as f32 * terrain_depth;
        let world_radius = radius / terrain_resolution as f32 * terrain_width;
        let world_radius_sq = world_radius * world_radius;

        // Remove instances within radius
        self.instances.retain(|inst| {
            let dx = inst.x - world_cx;
            let dz = inst.z - world_cz;
            dx * dx + dz * dz > world_radius_sq
        });

        // Also reduce density
        for dm in self.density_maps.iter_mut() {
            let grid_x = center_grid_x as f32 * dm.resolution as f32 / terrain_resolution as f32;
            let grid_z = center_grid_z as f32 * dm.resolution as f32 / terrain_resolution as f32;

            let erase_settings = BrushSettings {
                brush_type: BrushType::EraseFoliage { radius },
                radius,
                strength: 1.0,
                falloff: FalloffType::Smooth,
            };
            dm.apply_brush(grid_x, grid_z, &erase_settings);
        }
    }

    /// Regenerate all instances from density maps.
    pub fn regenerate_instances(
        &mut self,
        heightmap: &[f32],
        terrain_resolution: u32,
        terrain_width: f32,
        terrain_depth: f32,
    ) {
        self.instances.clear();

        let mut rng = rand::rngs::StdRng::seed_from_u64(self.generation_seed);
        let half_w = terrain_width * 0.5;
        let half_d = terrain_depth * 0.5;

        for (type_idx, density_map) in self.density_maps.iter().enumerate() {
            if self.instances.len() >= self.max_instances as usize {
                break;
            }

            let foliage_def = &self.foliage_types[type_idx];

            for z in 0..density_map.resolution {
                for x in 0..density_map.resolution {
                    let density = density_map.get(x, z);
                    if density < 0.01 {
                        continue;
                    }

                    // Probability of placing an instance based on density
                    if rng.gen::<f32>() > density {
                        continue;
                    }

                    if self.instances.len() >= self.max_instances as usize {
                        break;
                    }

                    // Convert grid coords to world coords
                    let world_x =
                        (x as f32 / density_map.resolution as f32) * terrain_width - half_w;
                    let world_z =
                        (z as f32 / density_map.resolution as f32) * terrain_depth - half_d;

                    // Sample height from terrain
                    let terrain_u = x as f32 / density_map.resolution as f32;
                    let terrain_v = z as f32 / density_map.resolution as f32;
                    let world_y =
                        sample_terrain_height(heightmap, terrain_resolution, terrain_u, terrain_v);

                    let scale = rng.gen_range(foliage_def.scale_min..=foliage_def.scale_max);
                    let rotation = rng.gen_range(0.0..=foliage_def.random_rotation);

                    let color_tint = [
                        rng.gen_range(
                            -foliage_def.color_variation[0]..=foliage_def.color_variation[0],
                        ),
                        rng.gen_range(
                            -foliage_def.color_variation[1]..=foliage_def.color_variation[1],
                        ),
                        rng.gen_range(
                            -foliage_def.color_variation[2]..=foliage_def.color_variation[2],
                        ),
                        0.0,
                    ];

                    self.instances.push(FoliageInstance {
                        x: world_x,
                        y: world_y,
                        z: world_z,
                        rotation_deg: rotation,
                        scale,
                        foliage_type: type_idx as u32,
                        color_tint,
                        lod_override: -1,
                    });
                }
            }
        }
    }

    /// Update brush settings to match selected foliage type.
    pub fn update_brush_for_selection(&mut self) {
        self.brush_settings.brush_type = BrushType::Foliage {
            foliage_type: self.selected_type as u32,
            density: self.brush_settings.strength,
        };
    }

    /// Add a custom foliage type.
    pub fn add_foliage_type(&mut self, name: &str) {
        self.foliage_types.push(FoliageTypeDef {
            kind: FoliageKind::Grass,
            name: name.to_string(),
            mesh_path: String::new(),
            material_path: String::new(),
            scale_min: 0.5,
            scale_max: 1.5,
            random_rotation: 360.0,
            color_variation: [0.1, 0.1, 0.1, 0.0],
            min_slope: 0.0,
            max_slope: 45.0,
            min_height: 0.0,
            max_height: 1.0,
            lod_distances: [50.0, 100.0, 200.0],
            billboard: false,
            wind_weight: 0.5,
        });
        self.density_maps.push(FoliageDensityMap::new(128));
    }

    /// Get instance count.
    pub fn instance_count(&self) -> usize {
        self.instances.len()
    }
}

impl Default for FoliageEditorState {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Helper: sample terrain height
// ---------------------------------------------------------------------------

fn sample_terrain_height(heightmap: &[f32], resolution: u32, u: f32, v: f32) -> f32 {
    let u = u.clamp(0.0, 1.0);
    let v = v.clamp(0.0, 1.0);
    let fx = u * (resolution - 1) as f32;
    let fz = v * (resolution - 1) as f32;
    let ix = (fx as u32).min(resolution - 2);
    let iz = (fz as u32).min(resolution - 2);
    let tx = fx - ix as f32;
    let tz = fz - iz as f32;

    let h00 = heightmap[(iz * resolution + ix) as usize];
    let h10 = heightmap[(iz * resolution + ix + 1) as usize];
    let h01 = heightmap[((iz + 1) * resolution + ix) as usize];
    let h11 = heightmap[((iz + 1) * resolution + ix + 1) as usize];

    h00 * (1.0 - tx) * (1.0 - tz) + h10 * tx * (1.0 - tz) + h01 * (1.0 - tx) * tz + h11 * tx * tz
}

// ---------------------------------------------------------------------------
// Foliage Editor UI
// ---------------------------------------------------------------------------

/// Render the foliage editor panel.
pub fn foliage_editor_ui(ui: &mut Ui, state: &mut FoliageEditorState) {
    ui.heading("Foliage Painting");
    ui.separator();

    // Stats
    ui.horizontal(|ui| {
        ui.label(format!(
            "Instances: {} / {}",
            state.instance_count(),
            state.max_instances
        ));
    });

    ui.separator();

    // Foliage type selection
    ui.label("Foliage Types:");
    let mut update_brush = false;
    let mut new_selection = None;
    egui::ScrollArea::vertical()
        .max_height(180.0)
        .show(ui, |ui| {
            for (i, ftype) in state.foliage_types.iter().enumerate() {
                let is_selected = i == state.selected_type;
                ui.horizontal(|ui| {
                    let btn = if is_selected {
                        ui.selectable_label(true, format!("{} {}", ftype.kind.icon(), ftype.name))
                    } else {
                        ui.selectable_label(false, format!("{} {}", ftype.kind.icon(), ftype.name))
                    };
                    if btn.clicked() {
                        new_selection = Some(i);
                        update_brush = true;
                    }
                });
            }
        });

    if let Some(sel) = new_selection {
        state.selected_type = sel;
    }
    if update_brush {
        state.update_brush_for_selection();
    }

    ui.horizontal(|ui| {
        if ui.button("+ Add Type").clicked() {
            state.add_foliage_type(&format!("Custom {}", state.foliage_types.len()));
        }
    });

    ui.separator();

    // Selected type properties
    if let Some(ftype) = state.foliage_types.get_mut(state.selected_type) {
        ui.label(format!("Properties: {}", ftype.name));

        ui.horizontal(|ui| {
            ui.label("Scale Min:");
            ui.add(
                egui::DragValue::new(&mut ftype.scale_min)
                    .speed(0.05)
                    .range(0.01..=10.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Scale Max:");
            ui.add(
                egui::DragValue::new(&mut ftype.scale_max)
                    .speed(0.05)
                    .range(0.01..=10.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Wind Weight:");
            ui.add(egui::Slider::new(&mut ftype.wind_weight, 0.0..=1.0));
        });
        ui.horizontal(|ui| {
            ui.label("Max Slope:");
            ui.add(
                egui::DragValue::new(&mut ftype.max_slope)
                    .speed(1.0)
                    .range(0.0..=90.0),
            );
        });

        ui.horizontal(|ui| {
            ui.label("Mesh:");
            ui.text_edit_singleline(&mut ftype.mesh_path);
        });
        ui.horizontal(|ui| {
            ui.label("Material:");
            ui.text_edit_singleline(&mut ftype.material_path);
        });
    }

    ui.separator();

    // Brush settings
    ui.label("Foliage Brush:");
    ui.horizontal(|ui| {
        ui.label("Radius:");
        ui.add(
            egui::Slider::new(&mut state.brush_settings.radius, 1.0..=50.0).text("Brush Radius"),
        );
    });
    ui.horizontal(|ui| {
        ui.label("Density:");
        ui.add(egui::Slider::new(&mut state.brush_settings.strength, 0.01..=1.0).text("Density"));
    });

    ui.horizontal(|ui| {
        ui.label("Falloff:");
        let mut falloff_name = format!("{:?}", state.brush_settings.falloff);
        egui::ComboBox::from_id_salt("foliage_falloff")
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

    ui.separator();

    // Display options
    ui.checkbox(&mut state.show_instances, "Show Instances");
    ui.checkbox(&mut state.show_density_overlay, "Show Density Overlay");
    if state.show_density_overlay {
        ui.horizontal(|ui| {
            ui.label("Opacity:");
            ui.add(egui::Slider::new(
                &mut state.density_overlay_opacity,
                0.0..=1.0,
            ));
        });
        ui.horizontal(|ui| {
            ui.label("Visualize Type:");
            egui::ComboBox::from_id_salt("density_viz_type")
                .selected_text(
                    state
                        .foliage_types
                        .get(state.visualize_density_type)
                        .map(|t| &t.name)
                        .unwrap_or(&"None".to_string()),
                )
                .show_ui(ui, |ui| {
                    for (i, ftype) in state.foliage_types.iter().enumerate() {
                        if ui
                            .selectable_label(state.visualize_density_type == i, &ftype.name)
                            .clicked()
                        {
                            state.visualize_density_type = i;
                        }
                    }
                });
        });
    }

    ui.separator();

    // Generation settings
    ui.checkbox(&mut state.auto_regenerate, "Auto-regenerate Instances");
    ui.horizontal(|ui| {
        ui.label("Max Instances:");
        ui.add(
            egui::DragValue::new(&mut state.max_instances)
                .speed(100)
                .range(100..=1_000_000),
        );
    });
    ui.horizontal(|ui| {
        ui.label("Seed:");
        ui.add(egui::DragValue::new(&mut state.generation_seed).speed(1.0));
    });

    if ui.button("Regenerate All").clicked() {
        state.generation_seed = state.generation_seed.wrapping_add(1);
        // Note: actual regeneration requires terrain data, done externally
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foliage_type_presets() {
        let grass = FoliageTypeDef::grass();
        assert_eq!(grass.kind, FoliageKind::Grass);
        assert!(grass.wind_weight > 0.0);

        let rock = FoliageTypeDef::rock();
        assert_eq!(rock.kind, FoliageKind::Rock);
        assert!((rock.wind_weight - 0.0).abs() < 0.001);
    }

    #[test]
    fn density_map_brush() {
        let mut dm = FoliageDensityMap::new(64);
        let settings = BrushSettings::foliage(0, 0.5, 5.0);
        dm.apply_brush(32.0, 32.0, &settings);

        // Center should have some density
        assert!(dm.get(32, 32) > 0.0);
        // Edge should be zero
        assert!((dm.get(0, 0) - 0.0).abs() < 0.001);
    }

    #[test]
    fn density_map_erase() {
        let mut dm = FoliageDensityMap::new(64);
        // First paint
        let paint = BrushSettings::foliage(0, 1.0, 10.0);
        dm.apply_brush(32.0, 32.0, &paint);
        let painted = dm.get(32, 32);
        assert!(painted > 0.0);

        // Then erase
        let erase = BrushSettings::erase_foliage(10.0);
        dm.apply_brush(32.0, 32.0, &erase);
        let erased = dm.get(32, 32);
        assert!(erased < painted);
    }

    #[test]
    fn sample_terrain_bilinear() {
        let heightmap = vec![0.0, 0.0, 0.0, 0.0];
        let h = sample_terrain_height(&heightmap, 2, 0.5, 0.5);
        assert!((h - 0.0).abs() < 0.001);
    }

    #[test]
    fn foliage_editor_state_init() {
        let state = FoliageEditorState::new();
        assert_eq!(state.foliage_types.len(), 6);
        assert_eq!(state.density_maps.len(), 6);
        assert_eq!(state.instances.len(), 0);
    }

    #[test]
    fn foliage_instance_position() {
        let inst = FoliageInstance {
            x: 10.0,
            y: 5.0,
            z: -3.0,
            rotation_deg: 45.0,
            scale: 1.5,
            foliage_type: 0,
            color_tint: [0.0; 4],
            lod_override: -1,
        };
        let pos = inst.position();
        assert!((pos[0] - 10.0).abs() < 0.001);
        assert!((pos[1] - 5.0).abs() < 0.001);
        assert!((pos[2] - (-3.0)).abs() < 0.001);
    }
}
