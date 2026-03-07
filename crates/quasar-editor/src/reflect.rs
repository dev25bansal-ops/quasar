//! Runtime reflection system for the inspector panel.
//!
//! Provides the [`Inspect`] trait and a [`ReflectionRegistry`] that maps
//! `TypeId` → inspector UI function. Components that implement [`Inspect`]
//! render their own egui widgets automatically.

use std::any::{Any, TypeId};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Inspect trait
// ---------------------------------------------------------------------------

/// Trait implemented by components that want to be editable in the inspector.
///
/// In the simplest case, implement this manually. A `#[derive(Inspect)]`
/// proc-macro can be added later to auto-generate the implementations.
pub trait Inspect: Any {
    /// Draw egui widgets for this component. Returns `true` if any value changed.
    fn inspect_ui(&mut self, ui: &mut egui::Ui) -> bool;
    /// Human-readable type name for the collapsing header.
    fn type_label(&self) -> &str;
}

// ---------------------------------------------------------------------------
// Field descriptor (for future derive macro support)
// ---------------------------------------------------------------------------

/// Describes a single reflected field.
#[derive(Debug, Clone)]
pub struct FieldDescriptor {
    pub name: &'static str,
    pub type_name: &'static str,
    pub offset: usize,
}

/// Metadata for how a field should be displayed in the inspector.
#[derive(Debug, Clone)]
pub struct FieldMeta {
    pub label: String,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub speed: f64,
    pub read_only: bool,
}

impl Default for FieldMeta {
    fn default() -> Self {
        Self {
            label: String::new(),
            min: None,
            max: None,
            speed: 0.05,
            read_only: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Widget helpers — type-directed UI generation
// ---------------------------------------------------------------------------

/// Draw a drag-value widget for an `f32`. Returns `true` if changed.
pub fn widget_f32(ui: &mut egui::Ui, label: &str, value: &mut f32, meta: &FieldMeta) -> bool {
    let mut dv = egui::DragValue::new(value).speed(meta.speed as f32).prefix(format!("{label} "));
    if let Some(min) = meta.min {
        if let Some(max) = meta.max {
            dv = dv.range(min as f32..=max as f32);
        }
    }
    ui.add(dv).changed()
}

/// Draw a drag-value widget for an `f64`. Returns `true` if changed.
pub fn widget_f64(ui: &mut egui::Ui, label: &str, value: &mut f64, meta: &FieldMeta) -> bool {
    let mut dv = egui::DragValue::new(value).speed(meta.speed).prefix(format!("{label} "));
    if let Some(min) = meta.min {
        if let Some(max) = meta.max {
            dv = dv.range(min..=max);
        }
    }
    ui.add(dv).changed()
}

/// Draw a drag-value widget for a `u32`.
pub fn widget_u32(ui: &mut egui::Ui, label: &str, value: &mut u32, meta: &FieldMeta) -> bool {
    let mut v = *value as f64;
    let changed = widget_f64(ui, label, &mut v, meta);
    if changed {
        *value = v.max(0.0) as u32;
    }
    changed
}

/// Draw a drag-value widget for an `i32`.
pub fn widget_i32(ui: &mut egui::Ui, label: &str, value: &mut i32, meta: &FieldMeta) -> bool {
    let mut v = *value as f64;
    let changed = widget_f64(ui, label, &mut v, meta);
    if changed {
        *value = v as i32;
    }
    changed
}

/// Draw a checkbox for a `bool`.
pub fn widget_bool(ui: &mut egui::Ui, label: &str, value: &mut bool) -> bool {
    ui.checkbox(value, label).changed()
}

/// Draw a text edit for a `String`.
pub fn widget_string(ui: &mut egui::Ui, label: &str, value: &mut String) -> bool {
    ui.label(label);
    ui.text_edit_singleline(value).changed()
}

/// Draw an RGB color editor for a `[f32; 3]`.
pub fn widget_color3(ui: &mut egui::Ui, label: &str, value: &mut [f32; 3]) -> bool {
    ui.label(label);
    ui.color_edit_button_rgb(value).changed()
}

/// Draw an RGBA color editor for a `[f32; 4]`.
pub fn widget_color4(ui: &mut egui::Ui, label: &str, value: &mut [f32; 4]) -> bool {
    ui.label(label);
    ui.color_edit_button_rgba_unmultiplied(value).changed()
}

/// Draw a Vec3-like XYZ editor for a `[f32; 3]`.
pub fn widget_vec3(ui: &mut egui::Ui, label: &str, value: &mut [f32; 3], speed: f32) -> bool {
    ui.label(label);
    ui.horizontal(|ui| {
        let mut c = false;
        c |= ui.add(egui::DragValue::new(&mut value[0]).speed(speed).prefix("X ")).changed();
        c |= ui.add(egui::DragValue::new(&mut value[1]).speed(speed).prefix("Y ")).changed();
        c |= ui.add(egui::DragValue::new(&mut value[2]).speed(speed).prefix("Z ")).changed();
        c
    }).inner
}

// ---------------------------------------------------------------------------
// Reflection registry
// ---------------------------------------------------------------------------

/// Type-erased inspector function: receives `&mut dyn Any` + `&mut egui::Ui`,
/// returns `true` if any field was modified.
pub type InspectFn = Box<dyn Fn(&mut dyn Any, &mut egui::Ui) -> bool + Send + Sync>;

/// Registry mapping `TypeId` to inspector rendering functions.
///
/// Game code registers component types at startup:
/// ```ignore
/// registry.register::<MyComponent>(|comp, ui| {
///     comp.inspect_ui(ui)
/// });
/// ```
pub struct ReflectionRegistry {
    entries: HashMap<TypeId, RegistryEntry>,
}

struct RegistryEntry {
    label: String,
    inspect_fn: InspectFn,
}

impl ReflectionRegistry {
    pub fn new() -> Self {
        Self { entries: HashMap::new() }
    }

    /// Register a type that implements [`Inspect`].
    pub fn register<T: Inspect + 'static>(&mut self, label: impl Into<String>) {
        let label = label.into();
        self.entries.insert(
            TypeId::of::<T>(),
            RegistryEntry {
                label,
                inspect_fn: Box::new(|any, ui| {
                    if let Some(comp) = any.downcast_mut::<T>() {
                        comp.inspect_ui(ui)
                    } else {
                        false
                    }
                }),
            },
        );
    }

    /// Register a custom inspect function for a type (no trait required).
    pub fn register_fn<T: 'static>(
        &mut self,
        label: impl Into<String>,
        f: impl Fn(&mut T, &mut egui::Ui) -> bool + Send + Sync + 'static,
    ) {
        let label = label.into();
        self.entries.insert(
            TypeId::of::<T>(),
            RegistryEntry {
                label,
                inspect_fn: Box::new(move |any, ui| {
                    if let Some(comp) = any.downcast_mut::<T>() {
                        f(comp, ui)
                    } else {
                        false
                    }
                }),
            },
        );
    }

    /// Check if a type is registered.
    pub fn has(&self, type_id: TypeId) -> bool {
        self.entries.contains_key(&type_id)
    }

    /// Get the label for a registered type.
    pub fn label(&self, type_id: TypeId) -> Option<&str> {
        self.entries.get(&type_id).map(|e| e.label.as_str())
    }

    /// Invoke the inspector UI for a type-erased component.
    /// Returns `true` if any field was modified.
    pub fn inspect(&self, type_id: TypeId, component: &mut dyn Any, ui: &mut egui::Ui) -> bool {
        if let Some(entry) = self.entries.get(&type_id) {
            egui::CollapsingHeader::new(&entry.label)
                .default_open(true)
                .show(ui, |ui| (entry.inspect_fn)(component, ui))
                .body_returned
                .unwrap_or(false)
        } else {
            false
        }
    }
}

impl Default for ReflectionRegistry {
    fn default() -> Self {
        Self::new()
    }
}
