//! Particle Editor - Complete particle effect editor with VFX graph integration.
//!
//! Provides:
//! - Particle editor panel integrated into egui-based editor
//! - Real-time particle preview in viewport
//! - Emitter properties editor (rate, lifetime, velocity, color, size)
//! - Force fields (gravity, wind, turbulence, vortex)
//! - Collision with geometry
//! - Save/load particle systems to JSON format
//! - VFX graph node-based editor integration
//! - Preset particle effects (fire, smoke, explosion, sparks, magic)
//! - Curve editors for animated properties
//! - GPU and CPU simulation modes

use egui::{Color32, RichText, Ui};
use quasar_render::particle::{
    AlignmentDef, BlendModeDef, CpuParticleSimulator, CollisionDef, CollisionType, ColorKeyframe,
    CurveKeyframe, EmitterDef, EmitterShape, ForceDef, ForceType, ModifierDef, ModifierType,
    ParticleRendererDef, ParticleSystemDef, SortingDef, SimulationSpace,
    evaluate_curve, evaluate_color_gradient,
};
use quasar_render::vfx_graph::{VfxGraph, VfxNodeId};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::vfx_graph::VfxGraphEditorState;

// ---------------------------------------------------------------------------
// Simulation Mode
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SimulationMode {
    Cpu,
    GpuCompute,
}

impl Default for SimulationMode {
    fn default() -> Self {
        Self::Cpu
    }
}

// ---------------------------------------------------------------------------
// Particle Editor State
// ---------------------------------------------------------------------------

/// Complete state for the particle editor.
pub struct ParticleEditorState {
    /// Currently edited particle system definition.
    pub system_def: ParticleSystemDef,
    /// Selected emitter index.
    pub selected_emitter: Option<usize>,
    /// Selected force index.
    pub selected_force: Option<usize>,
    /// Selected modifier index.
    pub selected_modifier: Option<usize>,
    /// Selected collision index.
    pub selected_collision: Option<usize>,
    /// Active tab in the editor.
    pub active_tab: EditorTab,
    /// CPU simulator for preview.
    pub simulator: CpuParticleSimulator,
    /// Whether the preview is playing.
    pub is_playing: bool,
    /// Playback speed multiplier.
    pub playback_speed: f32,
    /// Show particle bounds.
    pub show_bounds: bool,
    /// Show grid in preview.
    pub show_grid: bool,
    /// Background color for preview.
    pub background_color: [f32; 4],
    /// Particle size multiplier for preview.
    pub preview_particle_size: f32,
    /// VFX graph editor state.
    pub vfx_graph_editor: VfxGraphEditorState,
    /// Use VFX graph mode (true) or direct definition mode (false).
    pub use_vfx_graph: bool,
    /// File path for save/load.
    pub file_path: Option<PathBuf>,
    /// Status message.
    pub status_message: String,
    /// Show preset browser.
    pub show_presets: bool,
    /// Search filter for presets.
    pub preset_search: String,
    /// Undo stack for particle system changes.
    pub undo_stack: Vec<ParticleSystemDef>,
    /// Redo stack.
    pub redo_stack: Vec<ParticleSystemDef>,
}

/// Editor tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorTab {
    Emitters,
    Forces,
    Modifiers,
    Collisions,
    Renderer,
    VfxGraph,
    Preview,
}

impl Default for ParticleEditorState {
    fn default() -> Self {
        let system_def = ParticleSystemDef::default();
        let simulator = CpuParticleSimulator::new(system_def.clone());
        Self {
            system_def,
            selected_emitter: Some(0),
            selected_force: None,
            selected_modifier: None,
            selected_collision: None,
            active_tab: EditorTab::Emitters,
            simulator,
            is_playing: true,
            playback_speed: 1.0,
            show_bounds: true,
            show_grid: true,
            background_color: [0.05, 0.05, 0.1, 1.0],
            preview_particle_size: 1.0,
            vfx_graph_editor: VfxGraphEditorState::new(),
            use_vfx_graph: false,
            file_path: None,
            status_message: String::new(),
            show_presets: false,
            preset_search: String::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }
}

impl ParticleEditorState {
    /// Create a new particle editor with default state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create from an existing particle system definition.
    pub fn from_system(system_def: ParticleSystemDef) -> Self {
        let simulator = CpuParticleSimulator::new(system_def.clone());
        Self {
            system_def,
            simulator,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            ..Self::default()
        }
    }

    /// Reset the simulation.
    pub fn reset_simulation(&mut self) {
        self.simulator.reset();
    }

    /// Update the simulation.
    pub fn update_simulation(&mut self, dt: f32) {
        if self.is_playing {
            self.simulator.update(dt);
        }
    }

    /// Push current state to undo stack.
    fn push_undo(&mut self) {
        self.undo_stack.push(self.system_def.clone());
        if self.undo_stack.len() > 50 {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    /// Undo last change.
    pub fn undo(&mut self) {
        if let Some(prev) = self.undo_stack.pop() {
            self.redo_stack.push(self.system_def.clone());
            self.system_def = prev;
            self.simulator = CpuParticleSimulator::new(self.system_def.clone());
            self.status_message = "Undo".to_string();
        }
    }

    /// Redo last change.
    pub fn redo(&mut self) {
        if let Some(next) = self.redo_stack.pop() {
            self.undo_stack.push(self.system_def.clone());
            self.system_def = next;
            self.simulator = CpuParticleSimulator::new(self.system_def.clone());
            self.status_message = "Redo".to_string();
        }
    }

    /// Save particle system to JSON file.
    pub fn save_to_file(&mut self, path: &PathBuf) -> Result<(), String> {
        self.push_undo();
        let json = serde_json::to_string_pretty(&self.system_def)
            .map_err(|e| format!("Failed to serialize: {}", e))?;
        std::fs::write(path, json).map_err(|e| format!("Failed to write file: {}", e))?;
        self.file_path = Some(path.clone());
        self.status_message = format!("Saved to {}", path.display());
        Ok(())
    }

    /// Load particle system from JSON file.
    pub fn load_from_file(&mut self, path: &PathBuf) -> Result<(), String> {
        let json = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read file: {}", e))?;
        let system_def: ParticleSystemDef = serde_json::from_str(&json)
            .map_err(|e| format!("Failed to parse JSON: {}", e))?;
        self.push_undo();
        self.system_def = system_def;
        self.simulator = CpuParticleSimulator::new(self.system_def.clone());
        self.file_path = Some(path.clone());
        self.selected_emitter = if self.system_def.emitters.is_empty() {
            None
        } else {
            Some(0)
        };
        self.status_message = format!("Loaded from {}", path.display());
        Ok(())
    }

    /// Add a new emitter.
    pub fn add_emitter(&mut self) {
        self.push_undo();
        let idx = self.system_def.emitters.len();
        self.system_def.emitters.push(EmitterDef::default());
        self.selected_emitter = Some(idx);
        self.active_tab = EditorTab::Emitters;
    }

    /// Remove the selected emitter.
    pub fn remove_selected_emitter(&mut self) {
        if let Some(idx) = self.selected_emitter.take() {
            self.push_undo();
            self.system_def.emitters.remove(idx);
            if self.system_def.emitters.is_empty() {
                self.system_def.emitters.push(EmitterDef::default());
                self.selected_emitter = Some(0);
            } else {
                self.selected_emitter = Some(idx.min(self.system_def.emitters.len() - 1));
            }
        }
    }

    /// Duplicate the selected emitter.
    pub fn duplicate_selected_emitter(&mut self) {
        if let Some(idx) = self.selected_emitter {
            self.push_undo();
            let mut new_emitter = self.system_def.emitters[idx].clone();
            new_emitter.name = format!("{}_copy", new_emitter.name);
            self.system_def.emitters.push(new_emitter);
            self.selected_emitter = Some(self.system_def.emitters.len() - 1);
        }
    }

    /// Add a new force field.
    pub fn add_force(&mut self, force_type: ForceType) {
        self.push_undo();
        let idx = self.system_def.forces.len();
        self.system_def.forces.push(ForceDef {
            name: format!("{:?}", force_type),
            force_type,
            enabled: true,
        });
        self.selected_force = Some(idx);
        self.active_tab = EditorTab::Forces;
    }

    /// Remove the selected force.
    pub fn remove_selected_force(&mut self) {
        if let Some(idx) = self.selected_force.take() {
            self.push_undo();
            self.system_def.forces.remove(idx);
            self.selected_force = None;
        }
    }

    /// Add a new modifier.
    pub fn add_modifier(&mut self, modifier_type: ModifierType) {
        self.push_undo();
        let idx = self.system_def.modifiers.len();
        self.system_def.modifiers.push(ModifierDef {
            name: format!("{:?}", modifier_type),
            modifier_type,
            enabled: true,
        });
        self.selected_modifier = Some(idx);
        self.active_tab = EditorTab::Modifiers;
    }

    /// Remove the selected modifier.
    pub fn remove_selected_modifier(&mut self) {
        if let Some(idx) = self.selected_modifier.take() {
            self.push_undo();
            self.system_def.modifiers.remove(idx);
            self.selected_modifier = None;
        }
    }

    /// Add a new collision.
    pub fn add_collision(&mut self, collision_type: CollisionType) {
        self.push_undo();
        let idx = self.system_def.collisions.len();
        self.system_def.collisions.push(CollisionDef {
            name: format!("Collision_{}", idx),
            collision_type,
            bounce_factor: 0.5,
            kill_on_collision: false,
            enabled: true,
        });
        self.selected_collision = Some(idx);
        self.active_tab = EditorTab::Collisions;
    }

    /// Remove the selected collision.
    pub fn remove_selected_collision(&mut self) {
        if let Some(idx) = self.selected_collision.take() {
            self.push_undo();
            self.system_def.collisions.remove(idx);
            self.selected_collision = None;
        }
    }

    /// Load a preset particle system.
    pub fn load_preset(&mut self, preset: &str) {
        self.push_undo();
        self.system_def = match preset {
            "fire" => presets::fire(),
            "smoke" => presets::smoke(),
            "explosion" => presets::explosion(),
            "sparks" => presets::sparks(),
            "magic" => presets::magic(),
            "rain" => presets::rain(),
            "snow" => presets::snow(),
            "fountain" => presets::fountain(),
            _ => ParticleSystemDef::default(),
        };
        self.simulator = CpuParticleSimulator::new(self.system_def.clone());
        self.selected_emitter = if self.system_def.emitters.is_empty() {
            None
        } else {
            Some(0)
        };
        self.status_message = format!("Loaded preset: {}", preset);
    }
}

// ---------------------------------------------------------------------------
// Particle Editor UI
// ---------------------------------------------------------------------------

impl ParticleEditorState {
    /// Render the particle editor panel.
    pub fn ui(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("particle_editor")
            .default_width(400.0)
            .resizable(true)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    // Toolbar
                    self.render_toolbar(ui);
                    ui.separator();

                    // Tab bar
                    self.render_tabs(ui);
                    ui.separator();

                    // Tab content
                    match self.active_tab {
                        EditorTab::Emitters => self.render_emitters_tab(ui),
                        EditorTab::Forces => self.render_forces_tab(ui),
                        EditorTab::Modifiers => self.render_modifiers_tab(ui),
                        EditorTab::Collisions => self.render_collisions_tab(ui),
                        EditorTab::Renderer => self.render_renderer_tab(ui),
                        EditorTab::VfxGraph => self.render_vfx_graph_tab(ui),
                        EditorTab::Preview => self.render_preview_tab(ui),
                    }
                });
            });

        // Preset browser window
        if self.show_presets {
            self.render_preset_browser(ctx);
        }

        // Status bar
        if !self.status_message.is_empty() {
            // Show briefly then clear
        }
    }

    fn render_toolbar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label("\u{2728} Particle Editor");
            ui.separator();

            // Play/Pause/Reset
            if ui
                .button(if self.is_playing { "\u{23F8} Pause" } else { "\u{25B6} Play" })
                .clicked()
            {
                self.is_playing = !self.is_playing;
            }

            if ui.button("\u{23F9} Reset").clicked() {
                self.reset_simulation();
            }

            ui.separator();

            // Undo/Redo
            if ui.add_enabled(!self.undo_stack.is_empty(), egui::Button::new("\u{21A9} Undo")).clicked()
            {
                self.undo();
            }
            if ui.add_enabled(!self.redo_stack.is_empty(), egui::Button::new("\u{21AA} Redo")).clicked()
            {
                self.redo();
            }

            ui.separator();

            // Save/Load
            if ui.button("\u{1F4BE} Save").clicked() {
                let path_clone = self.file_path.clone();
                if let Some(path) = path_clone {
                    if let Err(e) = self.save_to_file(&path) {
                        self.status_message = format!("Save error: {}", e);
                    }
                }
            }

            if ui.button("\u{1F4C2} Load").clicked() {
                // Would open file dialog
            }

            if ui.button("\u{1F4E6} Presets").clicked() {
                self.show_presets = !self.show_presets;
            }
        });
    }

    fn render_tabs(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let tabs = [
                (EditorTab::Emitters, "\u{1F525} Emitters"),
                (EditorTab::Forces, "\u{1F300} Forces"),
                (EditorTab::Modifiers, "\u{2699} Modifiers"),
                (EditorTab::Collisions, "\u{1F4A5} Collisions"),
                (EditorTab::Renderer, "\u{1F3A8} Renderer"),
                (EditorTab::VfxGraph, "\u{1F50C} VFX Graph"),
                (EditorTab::Preview, "\u{1F441} Preview"),
            ];

            for (tab, label) in &tabs {
                let selected = self.active_tab == *tab;
                if ui.selectable_label(selected, *label).clicked() {
                    self.active_tab = *tab;
                }
            }
        });
    }

    fn render_emitters_tab(&mut self, ui: &mut Ui) {
        ui.heading("Emitters");

        // Emitter list
        ui.horizontal(|ui| {
            if ui.button("\u{2795} Add").clicked() {
                self.add_emitter();
            }
            if ui.button("\u{1F4CB} Duplicate").clicked() {
                self.duplicate_selected_emitter();
            }
            if ui.button("\u{1F5D1} Remove").clicked() {
                self.remove_selected_emitter();
            }
        });

        ui.separator();

        // Emitter list selector
        for (i, emitter) in self.system_def.emitters.iter().enumerate() {
            let selected = self.selected_emitter == Some(i);
            if ui.selectable_label(selected, &emitter.name).clicked() {
                self.selected_emitter = Some(i);
            }
        }

        ui.separator();

        // Emitter properties
        if let Some(idx) = self.selected_emitter {
            let mut emitter = self.system_def.emitters[idx].clone();

            ui.text_edit_singleline(&mut emitter.name);
            ui.checkbox(&mut emitter.enabled, "Enabled");

            ui.collapsing("Shape", |ui| {
                self.render_emitter_shape_ui(ui, &mut emitter);
            });

            ui.collapsing("Emission", |ui| {
                ui.horizontal(|ui| {
                    ui.label("Rate:");
                    ui.add(egui::DragValue::new(&mut emitter.rate).speed(1.0).range(0.0..=10000.0));
                });
                ui.horizontal(|ui| {
                    ui.label("Burst Count:");
                    ui.add(egui::DragValue::new(&mut emitter.burst_count).speed(1.0).range(0..=1000));
                });
                ui.horizontal(|ui| {
                    ui.label("Burst Interval:");
                    ui.add(egui::DragValue::new(&mut emitter.burst_interval).speed(0.1).range(0.1..=60.0));
                });
            });

            ui.collapsing("Lifetime", |ui| {
                let mut min = *emitter.lifetime.start();
                let mut max = *emitter.lifetime.end();
                ui.horizontal(|ui| {
                    ui.label("Min:");
                    ui.add(egui::DragValue::new(&mut min).speed(0.1).range(0.01..=60.0));
                    ui.label("Max:");
                    ui.add(egui::DragValue::new(&mut max).speed(0.1).range(0.01..=60.0));
                });
                emitter.lifetime = min..=max;
            });

            ui.collapsing("Velocity", |ui| {
                let mut min = *emitter.velocity.start();
                let mut max = *emitter.velocity.end();
                ui.horizontal(|ui| {
                    ui.label("Min:");
                    ui.add(egui::DragValue::new(&mut min).speed(0.1).range(0.0..=100.0));
                    ui.label("Max:");
                    ui.add(egui::DragValue::new(&mut max).speed(0.1).range(0.0..=100.0));
                });
                emitter.velocity = min..=max;
                ui.horizontal(|ui| {
                    ui.label("Spread Angle:");
                    ui.add(egui::DragValue::new(&mut emitter.spread_angle).speed(1.0).range(0.0..=180.0).suffix("°"));
                });
            });

            ui.collapsing("Size", |ui| {
                let mut min = *emitter.size.start();
                let mut max = *emitter.size.end();
                ui.horizontal(|ui| {
                    ui.label("Min:");
                    ui.add(egui::DragValue::new(&mut min).speed(0.01).range(0.001..=10.0));
                    ui.label("Max:");
                    ui.add(egui::DragValue::new(&mut max).speed(0.01).range(0.001..=10.0));
                });
                emitter.size = min..=max;
            });

            ui.collapsing("Color", |ui| {
                ui.label("Start Color:");
                self.render_color_picker(ui, &mut emitter.color_start);
                ui.label("End Color:");
                self.render_color_picker(ui, &mut emitter.color_end);
            });
            ui.collapsing("Position", |ui| {
                ui.horizontal(|ui| {
                    ui.label("X:");
                    ui.add(egui::DragValue::new(&mut emitter.position[0]).speed(0.1));
                    ui.label("Y:");
                    ui.add(egui::DragValue::new(&mut emitter.position[1]).speed(0.1));
                    ui.label("Z:");
                    ui.add(egui::DragValue::new(&mut emitter.position[2]).speed(0.1));
                });
            });

            ui.collapsing("Max Particles", |ui| {
                ui.add(egui::DragValue::new(&mut emitter.max_particles).speed(10.0).range(1..=100000));
            });

            self.system_def.emitters[idx] = emitter;
        } else {
            ui.label("No emitter selected");
        }
    }

    fn render_emitter_shape_ui(&mut self, ui: &mut Ui, emitter: &mut EmitterDef) {
        let current_shape = match &emitter.shape {
            EmitterShape::Point => "Point",
            EmitterShape::Box { .. } => "Box",
            EmitterShape::Sphere { .. } => "Sphere",
            EmitterShape::Cone { .. } => "Cone",
            EmitterShape::Circle { .. } => "Circle",
            EmitterShape::Hemisphere { .. } => "Hemisphere",
        };

        egui::ComboBox::from_label("Shape")
            .selected_text(current_shape)
            .show_ui(ui, |ui| {
                if ui.selectable_label(current_shape == "Point", "Point").clicked() {
                    emitter.shape = EmitterShape::Point;
                }
                if ui.selectable_label(current_shape == "Box", "Box").clicked() {
                    emitter.shape = EmitterShape::Box { size: [1.0, 1.0, 1.0] };
                }
                if ui.selectable_label(current_shape == "Sphere", "Sphere").clicked() {
                    emitter.shape = EmitterShape::Sphere { radius: 1.0 };
                }
                if ui.selectable_label(current_shape == "Cone", "Cone").clicked() {
                    emitter.shape = EmitterShape::Cone { angle: 30.0, length: 2.0 };
                }
                if ui.selectable_label(current_shape == "Circle", "Circle").clicked() {
                    emitter.shape = EmitterShape::Circle { radius: 1.0 };
                }
                if ui.selectable_label(current_shape == "Hemisphere", "Hemisphere").clicked() {
                    emitter.shape = EmitterShape::Hemisphere { radius: 1.0 };
                }
            });

        // Shape-specific parameters
        match &mut emitter.shape {
            EmitterShape::Box { size } => {
                ui.horizontal(|ui| {
                    ui.label("Size:");
                    ui.add(egui::DragValue::new(&mut size[0]).speed(0.1));
                    ui.add(egui::DragValue::new(&mut size[1]).speed(0.1));
                    ui.add(egui::DragValue::new(&mut size[2]).speed(0.1));
                });
            }
            EmitterShape::Sphere { radius } | EmitterShape::Circle { radius } | EmitterShape::Hemisphere { radius } => {
                ui.horizontal(|ui| {
                    ui.label("Radius:");
                    ui.add(egui::DragValue::new(radius).speed(0.1).range(0.01..=100.0));
                });
            }
            EmitterShape::Cone { angle, length } => {
                ui.horizontal(|ui| {
                    ui.label("Angle:");
                    ui.add(egui::DragValue::new(angle).speed(1.0).range(1.0..=179.0));
                    ui.label("Length:");
                    ui.add(egui::DragValue::new(length).speed(0.1).range(0.1..=50.0));
                });
            }
            _ => {}
        }
    }

    fn render_forces_tab(&mut self, ui: &mut Ui) {
        ui.heading("Force Fields");

        // Add force buttons
        ui.horizontal_wrapped(|ui| {
            if ui.button("\u{1F30D} Gravity").clicked() {
                self.add_force(ForceType::Gravity { strength: 9.81 });
            }
            if ui.button("\u{1F4A8} Wind").clicked() {
                self.add_force(ForceType::Wind { direction: [1.0, 0.0, 0.0], strength: 1.0 });
            }
            if ui.button("\u{1F300} Turbulence").clicked() {
                self.add_force(ForceType::Turbulence { strength: 1.0, frequency: 1.0, speed: 1.0, seed: 0 });
            }
            if ui.button("\u{1F300} Vortex").clicked() {
                self.add_force(ForceType::Vortex { center: [0.0, 0.0, 0.0], axis: [0.0, 1.0, 0.0], strength: 1.0, radius: 5.0 });
            }
            if ui.button("\u{1F9F2} Attractor").clicked() {
                self.add_force(ForceType::Attractor { position: [0.0, 0.0, 0.0], strength: 1.0, range: 10.0 });
            }
            if ui.button("\u{1F9F2} Repeller").clicked() {
                self.add_force(ForceType::Repeller { position: [0.0, 0.0, 0.0], strength: 1.0, range: 10.0 });
            }
        });

        ui.separator();

        // Force list
        for (i, force) in self.system_def.forces.iter().enumerate() {
            let selected = self.selected_force == Some(i);
            if ui.selectable_label(selected, &force.name).clicked() {
                self.selected_force = Some(i);
            }
        }

        ui.separator();

        // Force properties
        if let Some(idx) = self.selected_force {
            if let Some(force) = self.system_def.forces.get_mut(idx) {
                ui.text_edit_singleline(&mut force.name);
                ui.checkbox(&mut force.enabled, "Enabled");

                match &mut force.force_type {
                    ForceType::Gravity { strength } => {
                        ui.horizontal(|ui| {
                            ui.label("Strength:");
                            ui.add(egui::DragValue::new(strength).speed(0.1).range(0.0..=50.0));
                        });
                    }
                    ForceType::Wind { direction, strength } => {
                        ui.horizontal(|ui| {
                            ui.label("Direction:");
                            ui.add(egui::DragValue::new(&mut direction[0]).speed(0.1));
                            ui.add(egui::DragValue::new(&mut direction[1]).speed(0.1));
                            ui.add(egui::DragValue::new(&mut direction[2]).speed(0.1));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Strength:");
                            ui.add(egui::DragValue::new(strength).speed(0.1));
                        });
                    }
                    ForceType::Turbulence { strength, frequency, speed, seed } => {
                        ui.horizontal(|ui| {
                            ui.label("Strength:");
                            ui.add(egui::DragValue::new(strength).speed(0.1));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Frequency:");
                            ui.add(egui::DragValue::new(frequency).speed(0.1));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Speed:");
                            ui.add(egui::DragValue::new(speed).speed(0.1));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Seed:");
                            ui.add(egui::DragValue::new(seed).speed(1.0));
                        });
                    }
                    ForceType::Vortex { center, axis, strength, radius } => {
                        ui.horizontal(|ui| {
                            ui.label("Center:");
                            ui.add(egui::DragValue::new(&mut center[0]).speed(0.1));
                            ui.add(egui::DragValue::new(&mut center[1]).speed(0.1));
                            ui.add(egui::DragValue::new(&mut center[2]).speed(0.1));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Axis:");
                            ui.add(egui::DragValue::new(&mut axis[0]).speed(0.1));
                            ui.add(egui::DragValue::new(&mut axis[1]).speed(0.1));
                            ui.add(egui::DragValue::new(&mut axis[2]).speed(0.1));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Strength:");
                            ui.add(egui::DragValue::new(strength).speed(0.1));
                            ui.label("Radius:");
                            ui.add(egui::DragValue::new(radius).speed(0.1));
                        });
                    }
                    ForceType::Attractor { position, strength, range }
                    | ForceType::Repeller { position, strength, range } => {
                        ui.horizontal(|ui| {
                            ui.label("Position:");
                            ui.add(egui::DragValue::new(&mut position[0]).speed(0.1));
                            ui.add(egui::DragValue::new(&mut position[1]).speed(0.1));
                            ui.add(egui::DragValue::new(&mut position[2]).speed(0.1));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Strength:");
                            ui.add(egui::DragValue::new(strength).speed(0.1));
                            ui.label("Range:");
                            ui.add(egui::DragValue::new(range).speed(0.1));
                        });
                    }
                    ForceType::Drag { coefficient } => {
                        ui.horizontal(|ui| {
                            ui.label("Coefficient:");
                            ui.add(egui::DragValue::new(coefficient).speed(0.01).range(0.0..=1.0));
                        });
                    }
                    ForceType::Noise { strength, scale } => {
                        ui.horizontal(|ui| {
                            ui.label("Strength:");
                            ui.add(egui::DragValue::new(strength).speed(0.1));
                            ui.label("Scale:");
                            ui.add(egui::DragValue::new(scale).speed(0.1));
                        });
                    }
                }

                ui.separator();
                if ui.button("\u{1F5D1} Remove Force").clicked() {
                    self.remove_selected_force();
                }
            }
        } else {
            ui.label("No force field selected");
        }
    }

    fn render_modifiers_tab(&mut self, ui: &mut Ui) {
        ui.heading("Modifiers");

        ui.horizontal_wrapped(|ui| {
            if ui.button("\u{1F3A8} Color Over Lifetime").clicked() {
                self.add_modifier(ModifierType::ColorOverLifetime {
                    gradient: vec![
                        ColorKeyframe { time: 0.0, color: [1.0, 1.0, 1.0, 1.0] },
                        ColorKeyframe { time: 1.0, color: [1.0, 1.0, 1.0, 0.0] },
                    ],
                });
            }
            if ui.button("\u{1F4D0} Size Over Lifetime").clicked() {
                self.add_modifier(ModifierType::SizeOverLifetime {
                    curve: vec![
                        CurveKeyframe { time: 0.0, value: 1.0, in_tangent: 0.0, out_tangent: 0.0 },
                        CurveKeyframe { time: 1.0, value: 1.0, in_tangent: 0.0, out_tangent: 0.0 },
                    ],
                });
            }
            if ui.button("\u{26A1} Limit Velocity").clicked() {
                self.add_modifier(ModifierType::LimitVelocity { max_speed: 10.0 });
            }
            if ui.button("\u{1F4CF} Clamp Size").clicked() {
                self.add_modifier(ModifierType::ClampSize { min: 0.01, max: 10.0 });
            }
        });

        ui.separator();

        for (i, modifier) in self.system_def.modifiers.iter().enumerate() {
            let selected = self.selected_modifier == Some(i);
            if ui.selectable_label(selected, &modifier.name).clicked() {
                self.selected_modifier = Some(i);
            }
        }

        ui.separator();

        if let Some(idx) = self.selected_modifier {
            let mut modifier = self.system_def.modifiers[idx].clone();

            ui.text_edit_singleline(&mut modifier.name);
            ui.checkbox(&mut modifier.enabled, "Enabled");

            match &mut modifier.modifier_type {
                ModifierType::ColorOverLifetime { gradient } => {
                    ui.label("Color Gradient:");
                    self.render_color_gradient_ui(ui, gradient);
                }
                ModifierType::SizeOverLifetime { curve } => {
                    ui.label("Size Curve:");
                    self.render_curve_ui(ui, curve);
                }
                ModifierType::LimitVelocity { max_speed } => {
                    ui.horizontal(|ui| {
                        ui.label("Max Speed:");
                        ui.add(egui::DragValue::new(max_speed).speed(0.1).range(0.1..=100.0));
                    });
                }
                ModifierType::ClampSize { min, max } => {
                    ui.horizontal(|ui| {
                        ui.label("Min:");
                        ui.add(egui::DragValue::new(min).speed(0.01).range(0.001..=10.0));
                        ui.label("Max:");
                        ui.add(egui::DragValue::new(max).speed(0.01).range(0.001..=10.0));
                    });
                }
                _ => {}
            }

            ui.separator();
            let mut remove = false;
            if ui.button("\u{1F5D1} Remove Modifier").clicked() {
                remove = true;
            }

            self.system_def.modifiers[idx] = modifier;
            
            if remove {
                self.remove_selected_modifier();
            }
        } else {
            ui.label("No modifier selected");
        }
    }

    fn render_collisions_tab(&mut self, ui: &mut Ui) {
        ui.heading("Collisions");

        ui.horizontal_wrapped(|ui| {
            if ui.button("\u{1F4F0} Plane").clicked() {
                self.add_collision(CollisionType::Plane {
                    normal: [0.0, 1.0, 0.0],
                    distance: 0.0,
                });
            }
            if ui.button("\u{1F4E6} Box").clicked() {
                self.add_collision(CollisionType::Box {
                    min: [-5.0, 0.0, -5.0],
                    max: [5.0, 10.0, 5.0],
                });
            }
            if ui.button("\u{26AA} Sphere").clicked() {
                self.add_collision(CollisionType::Sphere {
                    center: [0.0, 0.0, 0.0],
                    radius: 1.0,
                });
            }
        });

        ui.separator();

        for (i, collision) in self.system_def.collisions.iter().enumerate() {
            let selected = self.selected_collision == Some(i);
            if ui.selectable_label(selected, &collision.name).clicked() {
                self.selected_collision = Some(i);
            }
        }

        ui.separator();

        if let Some(idx) = self.selected_collision {
            if let Some(collision) = self.system_def.collisions.get_mut(idx) {
                ui.text_edit_singleline(&mut collision.name);
                ui.checkbox(&mut collision.enabled, "Enabled");

                ui.horizontal(|ui| {
                    ui.label("Bounce Factor:");
                    ui.add(egui::DragValue::new(&mut collision.bounce_factor).speed(0.01).range(0.0..=1.0));
                });
                ui.checkbox(&mut collision.kill_on_collision, "Kill on Collision");

                ui.separator();
                if ui.button("\u{1F5D1} Remove Collision").clicked() {
                    self.remove_selected_collision();
                }
            }
        } else {
            ui.label("No collision selected");
        }
    }

    fn render_renderer_tab(&mut self, ui: &mut Ui) {
        ui.heading("Renderer Settings");

        let renderer = &mut self.system_def.renderer;

        ui.collapsing("Blend Mode", |ui| {
            let current = match renderer.blend_mode {
                BlendModeDef::Opaque => "Opaque",
                BlendModeDef::Alpha => "Alpha",
                BlendModeDef::Additive => "Additive",
                BlendModeDef::Multiply => "Multiply",
                BlendModeDef::Subtractive => "Subtractive",
            };

            egui::ComboBox::from_label("Blend Mode")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for (mode, name) in [
                        (BlendModeDef::Opaque, "Opaque"),
                        (BlendModeDef::Alpha, "Alpha"),
                        (BlendModeDef::Additive, "Additive"),
                        (BlendModeDef::Multiply, "Multiply"),
                        (BlendModeDef::Subtractive, "Subtractive"),
                    ] {
                        if ui.selectable_label(current == name, name).clicked() {
                            renderer.blend_mode = mode;
                        }
                    }
                });
        });

        ui.collapsing("Alignment", |ui| {
            let current = match renderer.alignment {
                AlignmentDef::ViewFacing => "View Facing",
                AlignmentDef::WorldY => "World Y",
                AlignmentDef::VelocityAligned => "Velocity Aligned",
                AlignmentDef::Fixed => "Fixed",
            };

            egui::ComboBox::from_label("Alignment")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for (align, name) in [
                        (AlignmentDef::ViewFacing, "View Facing"),
                        (AlignmentDef::WorldY, "World Y"),
                        (AlignmentDef::VelocityAligned, "Velocity Aligned"),
                        (AlignmentDef::Fixed, "Fixed"),
                    ] {
                        if ui.selectable_label(current == name, name).clicked() {
                            renderer.alignment = align;
                        }
                    }
                });
        });

        ui.collapsing("Sorting", |ui| {
            let current = match renderer.sorting {
                SortingDef::None => "None",
                SortingDef::Distance => "Distance",
                SortingDef::Age => "Age",
            };

            egui::ComboBox::from_label("Sorting")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for (sort, name) in [
                        (SortingDef::None, "None"),
                        (SortingDef::Distance, "Distance"),
                        (SortingDef::Age, "Age"),
                    ] {
                        if ui.selectable_label(current == name, name).clicked() {
                            renderer.sorting = sort;
                        }
                    }
                });
        });

        ui.checkbox(&mut renderer.cast_shadows, "Cast Shadows");

        if let Some(tex) = &mut renderer.texture {
            ui.horizontal(|ui| {
                ui.label("Texture:");
                ui.text_edit_singleline(tex);
            });
        } else {
            if ui.button("Set Texture").clicked() {
                renderer.texture = Some(String::new());
            }
        }
    }

    fn render_vfx_graph_tab(&mut self, ui: &mut Ui) {
        ui.heading("VFX Graph Editor");
        ui.label("Visual node-based effects editing");
        ui.separator();

        // Switch to VFX graph mode
        ui.checkbox(&mut self.use_vfx_graph, "Use VFX Graph Mode");

        if self.use_vfx_graph {
            self.vfx_graph_editor.ui(ui.ctx());
            self.vfx_graph_editor.node_palette_ui(ui.ctx());

            ui.separator();
            if ui.button("\u{1F504} Convert Graph to System").clicked() {
                self.system_def = crate::vfx_graph::graph_to_particle_system(&self.vfx_graph_editor.graph);
                self.simulator = CpuParticleSimulator::new(self.system_def.clone());
                self.status_message = "Converted VFX graph to particle system".to_string();
            }
        } else {
            ui.label("Enable VFX Graph mode to use node-based editing");
            if ui.button("\u{2795} Create VFX Graph from System").clicked() {
                // Convert current system to graph (future enhancement)
                self.use_vfx_graph = true;
            }
        }
    }

    fn render_preview_tab(&mut self, ui: &mut Ui) {
        ui.heading("Preview Settings");

        ui.horizontal(|ui| {
            ui.label("Playback Speed:");
            ui.add(egui::DragValue::new(&mut self.playback_speed).speed(0.1).range(0.1..=5.0));
        });

        ui.checkbox(&mut self.show_grid, "Show Grid");
        ui.checkbox(&mut self.show_bounds, "Show Bounds");

        ui.horizontal(|ui| {
            ui.label("Preview Size:");
            ui.add(egui::DragValue::new(&mut self.preview_particle_size).speed(0.1).range(0.1..=5.0));
        });

        ui.label("Background Color:");
        let mut bg_color = self.background_color;
        self.render_color_picker(ui, &mut bg_color);
        self.background_color = bg_color;

        ui.separator();
        ui.label("Simulation Stats:");
        ui.label(format!("Alive particles: {}", self.simulator.alive_count()));
        ui.label(format!("Time: {:.2}s", self.simulator.time));
    }

    // -----------------------------------------------------------------------
    // Helper UI rendering functions
    // -----------------------------------------------------------------------

    fn render_color_picker(&mut self, ui: &mut Ui, color: &mut [f32; 4]) {
        let mut egui_color = egui::Color32::from_rgba_premultiplied(
            (color[0] * 255.0) as u8,
            (color[1] * 255.0) as u8,
            (color[2] * 255.0) as u8,
            (color[3] * 255.0) as u8,
        );
        ui.color_edit_button_srgba(&mut egui_color);
        let srgba = egui_color.to_srgba_unmultiplied();
        color[0] = srgba[0] as f32 / 255.0;
        color[1] = srgba[1] as f32 / 255.0;
        color[2] = srgba[2] as f32 / 255.0;
        color[3] = srgba[3] as f32 / 255.0;
    }

    fn render_color_gradient_ui(&mut self, ui: &mut Ui, gradient: &mut Vec<ColorKeyframe>) {
        // Simple gradient visualization
        let gradient_height = 30.0;
        let width = ui.available_width() - 40.0;

        let (rect, response) = ui.allocate_exact_size(egui::vec2(width, gradient_height), egui::Sense::click());
        let painter = ui.painter_at(rect);

        // Draw gradient
        let steps = 50;
        for i in 0..steps {
            let t = i as f32 / steps as f32;
            let color = evaluate_color_gradient(gradient, t);
            let x = rect.min.x + t * width;
            let bar_rect = egui::Rect::from_min_max(
                egui::pos2(x, rect.min.y),
                egui::pos2(x + width / steps as f32 + 1.0, rect.max.y),
            );
            let c = egui::Color32::from_rgba_premultiplied(
                (color[0] * 255.0) as u8,
                (color[1] * 255.0) as u8,
                (color[2] * 255.0) as u8,
                (color[3] * 255.0) as u8,
            );
            painter.rect_filled(bar_rect, 0.0, c);
        }

        // Draw keyframe markers
        for kf in gradient.iter() {
            let x = rect.min.x + kf.time * width;
            painter.line_segment(
                [egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)],
                egui::Stroke::new(2.0, egui::Color32::WHITE),
            );
        }
    }

    fn render_curve_ui(&mut self, ui: &mut Ui, curve: &mut Vec<CurveKeyframe>) {
        let graph_height = 80.0;
        let width = ui.available_width() - 40.0;

        let (rect, response) = ui.allocate_exact_size(egui::vec2(width, graph_height), egui::Sense::click());
        let painter = ui.painter_at(rect);

        // Draw grid
        painter.rect_filled(rect, 0.0, egui::Color32::from_gray(25));
        for i in 0..=4 {
            let x = rect.min.x + (i as f32 / 4.0) * width;
            painter.line_segment(
                [egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)],
                egui::Stroke::new(0.5, egui::Color32::from_gray(50)),
            );
        }

        // Draw curve
        let steps = 100;
        for i in 0..steps {
            let t = i as f32 / steps as f32;
            let val = evaluate_curve(curve, t);
            let x = rect.min.x + t * width;
            let y = rect.max.y - val * graph_height;
            if i == 0 {
                painter.line_segment(
                    [egui::pos2(x, y), egui::pos2(x + 1.0, y)],
                    egui::Stroke::new(2.0, egui::Color32::from_rgb(100, 200, 255)),
                );
            } else {
                let prev_t = (i - 1) as f32 / steps as f32;
                let prev_val = evaluate_curve(curve, prev_t);
                let prev_x = rect.min.x + prev_t * width;
                let prev_y = rect.max.y - prev_val * graph_height;
                painter.line_segment(
                    [egui::pos2(prev_x, prev_y), egui::pos2(x, y)],
                    egui::Stroke::new(2.0, egui::Color32::from_rgb(100, 200, 255)),
                );
            }
        }

        // Draw keyframes
        for kf in curve.iter() {
            let x = rect.min.x + kf.time * width;
            let y = rect.max.y - kf.value * graph_height;
            painter.circle_filled(egui::pos2(x, y), 5.0, egui::Color32::from_rgb(255, 200, 50));
            painter.circle_stroke(egui::pos2(x, y), 5.0, egui::Stroke::new(1.0, egui::Color32::WHITE));
        }
    }

    fn render_preset_browser(&mut self, ui: &egui::Context) {
        egui::Window::new("Particle Effect Presets")
            .default_size(egui::vec2(300.0, 400.0))
            .resizable(true)
            .show(ui, |ui| {
                ui.text_edit_singleline(&mut self.preset_search);
                ui.separator();

                let presets = [
                    ("fire", "\u{1F525} Fire"),
                    ("smoke", "\u{1F4A8} Smoke"),
                    ("explosion", "\u{1F4A5} Explosion"),
                    ("sparks", "\u{26A1} Sparks"),
                    ("magic", "\u{2728} Magic"),
                    ("rain", "\u{1F327} Rain"),
                    ("snow", "\u{2744} Snow"),
                    ("fountain", "\u{26F2} Fountain"),
                ];

                for (id, label) in &presets {
                    if !self.preset_search.is_empty()
                        && !label.to_lowercase().contains(&self.preset_search.to_lowercase())
                    {
                        continue;
                    }

                    if ui.button(*label).clicked() {
                        self.load_preset(id);
                        self.show_presets = false;
                    }
                }
            });
    }
}

// ---------------------------------------------------------------------------
// Preset particle effects
// ---------------------------------------------------------------------------

pub mod presets {
    use super::*;

    pub fn fire() -> ParticleSystemDef {
        let mut def = ParticleSystemDef::default();
        def.name = "Fire".to_string();
        def.emitters[0].name = "FireEmitter".to_string();
        def.emitters[0].shape = EmitterShape::Cone { angle: 40.0, length: 0.5 };
        def.emitters[0].rate = 50.0;
        def.emitters[0].lifetime = 0.5..=1.5;
        def.emitters[0].velocity = 2.0..=5.0;
        def.emitters[0].spread_angle = 20.0;
        def.emitters[0].size = 0.2..=0.5;
        def.emitters[0].color_start = [1.0, 0.8, 0.2, 1.0];
        def.emitters[0].color_end = [0.8, 0.2, 0.0, 0.0];
        def.forces.push(ForceDef {
            name: "Updraft".to_string(),
            force_type: ForceType::Wind { direction: [0.0, 1.0, 0.0], strength: 2.0 },
            enabled: true,
        });
        def.renderer.blend_mode = BlendModeDef::Additive;
        def
    }

    pub fn smoke() -> ParticleSystemDef {
        let mut def = ParticleSystemDef::default();
        def.name = "Smoke".to_string();
        def.emitters[0].name = "SmokeEmitter".to_string();
        def.emitters[0].shape = EmitterShape::Point;
        def.emitters[0].rate = 10.0;
        def.emitters[0].lifetime = 2.0..=4.0;
        def.emitters[0].velocity = 0.5..=1.5;
        def.emitters[0].spread_angle = 45.0;
        def.emitters[0].size = 0.5..=2.0;
        def.emitters[0].color_start = [0.4, 0.4, 0.4, 0.6];
        def.emitters[0].color_end = [0.6, 0.6, 0.6, 0.0];
        def.forces.push(ForceDef {
            name: "Rise".to_string(),
            force_type: ForceType::Wind { direction: [0.0, 0.5, 0.0], strength: 0.5 },
            enabled: true,
        });
        def.forces.push(ForceDef {
            name: "Turbulence".to_string(),
            force_type: ForceType::Turbulence { strength: 0.5, frequency: 2.0, speed: 1.0, seed: 0 },
            enabled: true,
        });
        def.renderer.blend_mode = BlendModeDef::Alpha;
        def
    }

    pub fn explosion() -> ParticleSystemDef {
        let mut def = ParticleSystemDef::default();
        def.name = "Explosion".to_string();
        def.emitters[0].name = "ExplosionCore".to_string();
        def.emitters[0].shape = EmitterShape::Sphere { radius: 0.1 };
        def.emitters[0].rate = 0.0;
        def.emitters[0].burst_count = 200;
        def.emitters[0].burst_interval = 0.1;
        def.emitters[0].lifetime = 0.3..=1.0;
        def.emitters[0].velocity = 5.0..=15.0;
        def.emitters[0].spread_angle = 360.0;
        def.emitters[0].size = 0.1..=0.5;
        def.emitters[0].color_start = [1.0, 1.0, 0.5, 1.0];
        def.emitters[0].color_end = [1.0, 0.3, 0.0, 0.0];
        def.forces.push(ForceDef {
            name: "Drag".to_string(),
            force_type: ForceType::Drag { coefficient: 0.1 },
            enabled: true,
        });
        def.renderer.blend_mode = BlendModeDef::Additive;
        def
    }

    pub fn sparks() -> ParticleSystemDef {
        let mut def = ParticleSystemDef::default();
        def.name = "Sparks".to_string();
        def.emitters[0].name = "SparksEmitter".to_string();
        def.emitters[0].shape = EmitterShape::Point;
        def.emitters[0].rate = 100.0;
        def.emitters[0].lifetime = 0.2..=0.8;
        def.emitters[0].velocity = 3.0..=8.0;
        def.emitters[0].spread_angle = 60.0;
        def.emitters[0].size = 0.02..=0.05;
        def.emitters[0].color_start = [1.0, 0.9, 0.4, 1.0];
        def.emitters[0].color_end = [1.0, 0.5, 0.0, 0.0];
        def.forces.push(ForceDef {
            name: "Gravity".to_string(),
            force_type: ForceType::Gravity { strength: 15.0 },
            enabled: true,
        });
        def.renderer.blend_mode = BlendModeDef::Additive;
        def
    }

    pub fn magic() -> ParticleSystemDef {
        let mut def = ParticleSystemDef::default();
        def.name = "Magic".to_string();
        def.emitters[0].name = "MagicEmitter".to_string();
        def.emitters[0].shape = EmitterShape::Sphere { radius: 0.5 };
        def.emitters[0].rate = 20.0;
        def.emitters[0].lifetime = 1.0..=3.0;
        def.emitters[0].velocity = 0.5..=2.0;
        def.emitters[0].spread_angle = 180.0;
        def.emitters[0].size = 0.05..=0.15;
        def.emitters[0].color_start = [0.5, 0.3, 1.0, 1.0];
        def.emitters[0].color_end = [0.8, 0.5, 1.0, 0.0];
        def.forces.push(ForceDef {
            name: "Vortex".to_string(),
            force_type: ForceType::Vortex {
                center: [0.0, 0.0, 0.0],
                axis: [0.0, 1.0, 0.0],
                strength: 2.0,
                radius: 3.0,
            },
            enabled: true,
        });
        def.renderer.blend_mode = BlendModeDef::Additive;
        def
    }

    pub fn rain() -> ParticleSystemDef {
        let mut def = ParticleSystemDef::default();
        def.name = "Rain".to_string();
        def.emitters[0].name = "RainEmitter".to_string();
        def.emitters[0].shape = EmitterShape::Box { size: [20.0, 0.1, 20.0] };
        def.emitters[0].position = [0.0, 20.0, 0.0];
        def.emitters[0].rate = 500.0;
        def.emitters[0].lifetime = 1.5..=2.5;
        def.emitters[0].velocity = 15.0..=20.0;
        def.emitters[0].spread_angle = 5.0;
        def.emitters[0].size = 0.02..=0.04;
        def.emitters[0].color_start = [0.6, 0.7, 0.9, 0.7];
        def.emitters[0].color_end = [0.6, 0.7, 0.9, 0.3];
        def.forces.push(ForceDef {
            name: "Gravity".to_string(),
            force_type: ForceType::Gravity { strength: 20.0 },
            enabled: true,
        });
        def.collisions.push(CollisionDef {
            name: "Ground".to_string(),
            collision_type: CollisionType::Plane { normal: [0.0, 1.0, 0.0], distance: 0.0 },
            bounce_factor: 0.0,
            kill_on_collision: true,
            enabled: true,
        });
        def.renderer.blend_mode = BlendModeDef::Alpha;
        def
    }

    pub fn snow() -> ParticleSystemDef {
        let mut def = ParticleSystemDef::default();
        def.name = "Snow".to_string();
        def.emitters[0].name = "SnowEmitter".to_string();
        def.emitters[0].shape = EmitterShape::Box { size: [20.0, 0.1, 20.0] };
        def.emitters[0].position = [0.0, 15.0, 0.0];
        def.emitters[0].rate = 100.0;
        def.emitters[0].lifetime = 5.0..=8.0;
        def.emitters[0].velocity = 0.5..=1.5;
        def.emitters[0].spread_angle = 180.0;
        def.emitters[0].size = 0.05..=0.15;
        def.emitters[0].color_start = [1.0, 1.0, 1.0, 0.9];
        def.emitters[0].color_end = [1.0, 1.0, 1.0, 0.0];
        def.forces.push(ForceDef {
            name: "Gravity".to_string(),
            force_type: ForceType::Gravity { strength: 2.0 },
            enabled: true,
        });
        def.forces.push(ForceDef {
            name: "Wind".to_string(),
            force_type: ForceType::Wind { direction: [1.0, 0.0, 0.5], strength: 0.5 },
            enabled: true,
        });
        def.forces.push(ForceDef {
            name: "Turbulence".to_string(),
            force_type: ForceType::Turbulence { strength: 0.3, frequency: 0.5, speed: 0.5, seed: 42 },
            enabled: true,
        });
        def.collisions.push(CollisionDef {
            name: "Ground".to_string(),
            collision_type: CollisionType::Plane { normal: [0.0, 1.0, 0.0], distance: 0.0 },
            bounce_factor: 0.0,
            kill_on_collision: true,
            enabled: true,
        });
        def.renderer.blend_mode = BlendModeDef::Alpha;
        def
    }

    pub fn fountain() -> ParticleSystemDef {
        let mut def = ParticleSystemDef::default();
        def.name = "Fountain".to_string();
        def.emitters[0].name = "FountainJet".to_string();
        def.emitters[0].shape = EmitterShape::Circle { radius: 0.2 };
        def.emitters[0].rate = 200.0;
        def.emitters[0].lifetime = 1.0..=2.0;
        def.emitters[0].velocity = 8.0..=12.0;
        def.emitters[0].spread_angle = 15.0;
        def.emitters[0].size = 0.05..=0.1;
        def.emitters[0].color_start = [0.3, 0.6, 1.0, 0.8];
        def.emitters[0].color_end = [0.5, 0.8, 1.0, 0.2];
        def.forces.push(ForceDef {
            name: "Gravity".to_string(),
            force_type: ForceType::Gravity { strength: 9.81 },
            enabled: true,
        });
        def.collisions.push(CollisionDef {
            name: "WaterSurface".to_string(),
            collision_type: CollisionType::Plane { normal: [0.0, 1.0, 0.0], distance: 0.0 },
            bounce_factor: 0.2,
            kill_on_collision: false,
            enabled: true,
        });
        def.renderer.blend_mode = BlendModeDef::Alpha;
        def
    }
}
