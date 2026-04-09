//! Prefab system — serializable entity templates.
//!
//! A `Prefab` is a reusable blueprint for one or more entities.  Prefabs
//! are stored as JSON and can be instantiated at runtime, spawning a fresh
//! set of entities with cloned component data.
//!
//! The prefab format builds on [`crate::scene_serde::EntityData`] and adds
//! optional physics / audio configuration as serde-friendly primitives.

use crate::ecs::{Entity, World};
use crate::scene_serde::EntityData;
use quasar_math::Transform;
use serde::{Deserialize, Serialize};
use std::path::Path;

// ---------------------------------------------------------------------------
// TransformField Enum — single source of truth for Transform field paths
// ---------------------------------------------------------------------------

/// Represents a single scalar field within a [`Transform`] component.
///
/// This enum provides a single source of truth for the 10 transform field
/// paths (`position.x/y/z`, `scale.x/y/z`, `rotation.x/y/z/w`) that are
/// used across prefab override, diffing, and propagation logic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransformField {
    PositionX,
    PositionY,
    PositionZ,
    ScaleX,
    ScaleY,
    ScaleZ,
    RotationX,
    RotationY,
    RotationZ,
    RotationW,
}

impl TransformField {
    /// All 10 transform field variants, in a canonical order.
    pub const ALL: &'static [Self] = &[
        Self::PositionX,
        Self::PositionY,
        Self::PositionZ,
        Self::ScaleX,
        Self::ScaleY,
        Self::ScaleZ,
        Self::RotationX,
        Self::RotationY,
        Self::RotationZ,
        Self::RotationW,
    ];

    /// Parse a dot-separated field path into a [`TransformField`].
    ///
    /// Returns `None` if the path doesn't match any known transform field.
    pub fn from_path(path: &str) -> Option<Self> {
        match path {
            "position.x" => Some(Self::PositionX),
            "position.y" => Some(Self::PositionY),
            "position.z" => Some(Self::PositionZ),
            "scale.x" => Some(Self::ScaleX),
            "scale.y" => Some(Self::ScaleY),
            "scale.z" => Some(Self::ScaleZ),
            "rotation.x" => Some(Self::RotationX),
            "rotation.y" => Some(Self::RotationY),
            "rotation.z" => Some(Self::RotationZ),
            "rotation.w" => Some(Self::RotationW),
            _ => None,
        }
    }

    /// Returns the canonical dot-separated path for this field.
    pub fn path(&self) -> &'static str {
        match self {
            Self::PositionX => "position.x",
            Self::PositionY => "position.y",
            Self::PositionZ => "position.z",
            Self::ScaleX => "scale.x",
            Self::ScaleY => "scale.y",
            Self::ScaleZ => "scale.z",
            Self::RotationX => "rotation.x",
            Self::RotationY => "rotation.y",
            Self::RotationZ => "rotation.z",
            Self::RotationW => "rotation.w",
        }
    }

    /// Read the scalar value from this field of a [`Transform`].
    pub fn get(&self, t: &Transform) -> f32 {
        match self {
            Self::PositionX => t.position.x,
            Self::PositionY => t.position.y,
            Self::PositionZ => t.position.z,
            Self::ScaleX => t.scale.x,
            Self::ScaleY => t.scale.y,
            Self::ScaleZ => t.scale.z,
            Self::RotationX => t.rotation.x,
            Self::RotationY => t.rotation.y,
            Self::RotationZ => t.rotation.z,
            Self::RotationW => t.rotation.w,
        }
    }

    /// Write a scalar value into this field of a [`Transform`].
    pub fn set(&self, t: &mut Transform, value: f32) {
        match self {
            Self::PositionX => t.position.x = value,
            Self::PositionY => t.position.y = value,
            Self::PositionZ => t.position.z = value,
            Self::ScaleX => t.scale.x = value,
            Self::ScaleY => t.scale.y = value,
            Self::ScaleZ => t.scale.z = value,
            Self::RotationX => t.rotation.x = value,
            Self::RotationY => t.rotation.y = value,
            Self::RotationZ => t.rotation.z = value,
            Self::RotationW => t.rotation.w = value,
        }
    }
}

// ---------------------------------------------------------------------------
// Prefab data model
// ---------------------------------------------------------------------------

/// A key-value pair for a generic component property.
/// This is intentionally stringly-typed so that user components can be
/// serialized without requiring a compile-time type registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrefabProperty {
    pub key: String,
    pub value: serde_json::Value,
}

/// A single entity template within a prefab.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrefabEntity {
    /// Human-readable tag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Local transform.
    pub transform: Transform,
    /// Mesh shape tag (e.g. "Cube", "Sphere").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mesh_shape: Option<String>,
    /// Index of the parent entity inside `Prefab::entities` (root = `None`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<usize>,
    /// Arbitrary additional properties that game-specific systems can read.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<PrefabProperty>,
}

/// A reusable entity blueprint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prefab {
    /// Prefab name (unique identifier).
    pub name: String,
    /// Ordered list of entity templates. Parent indices reference this vec.
    pub entities: Vec<PrefabEntity>,
}

impl Prefab {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            entities: Vec::new(),
        }
    }

    /// Add a root entity and return its index.
    pub fn add_entity(&mut self, entity: PrefabEntity) -> usize {
        let idx = self.entities.len();
        self.entities.push(entity);
        idx
    }

    /// Serialize to pretty JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserialize from JSON.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Load a prefab from a file.
    pub fn load(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let json = std::fs::read_to_string(path)?;
        Self::from_json(&json).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Save a prefab to a file.
    pub fn save(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let json = self
            .to_json()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, json)
    }

    /// Convert to `SceneData` for interop with the scene serialization layer.
    pub fn to_scene_data(&self) -> crate::scene_serde::SceneData {
        let mut data = crate::scene_serde::SceneData::new(&self.name);
        // Build a map from template index → children list.
        let mut children_map: std::collections::HashMap<usize, Vec<usize>> =
            std::collections::HashMap::new();
        for (i, pe) in self.entities.iter().enumerate() {
            if let Some(parent) = pe.parent {
                children_map.entry(parent).or_default().push(i);
            }
        }
        for (i, pe) in self.entities.iter().enumerate() {
            data.entities.push(EntityData {
                name: pe.name.clone(),
                transform: pe.transform,
                mesh_shape: pe.mesh_shape.clone(),
                children: children_map.get(&i).cloned().unwrap_or_default(),
            });
        }
        data
    }
}

// ---------------------------------------------------------------------------
// Instantiation
// ---------------------------------------------------------------------------

/// Spawn entities from a prefab into the world. Returns a list of
/// `(Entity, &PrefabEntity)` pairs in the same order as `prefab.entities`,
/// so callers can process mesh_shape and properties.
///
/// If `instance` is `Some`, each spawned entity gets a [`PrefabInstance`]
/// component and overrides are applied after creation.
pub fn instantiate_prefab<'a>(
    world: &mut World,
    prefab: &'a Prefab,
    instance: Option<PrefabInstance>,
) -> Vec<(Entity, &'a PrefabEntity)> {
    let mut spawned: Vec<(Entity, &PrefabEntity)> = Vec::with_capacity(prefab.entities.len());

    for pe in &prefab.entities {
        let entity = world.spawn();
        world.insert(entity, pe.transform);

        // Insert mesh_shape as a Name-like tag that downstream systems can match.
        if let Some(ref shape) = pe.mesh_shape {
            world.insert(entity, PrefabMeshTag(shape.clone()));
        }

        // Insert properties so game systems can read them.
        if !pe.properties.is_empty() {
            world.insert(entity, PrefabProperties(pe.properties.clone()));
        }

        // Attach prefab instance tracking.
        if let Some(ref inst) = instance {
            world.insert(entity, inst.clone());
        }

        spawned.push((entity, pe));
    }

    // Apply overrides after all entities are spawned.
    if let Some(ref inst) = instance {
        for &(entity, _) in &spawned {
            apply_overrides(world, entity, &inst.overrides);
        }
    }

    spawned
}

/// Tag component inserted for prefab entities that specify a `mesh_shape`.
#[derive(Debug, Clone)]
pub struct PrefabMeshTag(pub String);

/// Component holding arbitrary prefab properties for game-specific systems.
#[derive(Debug, Clone)]
pub struct PrefabProperties(pub Vec<PrefabProperty>);

// ---------------------------------------------------------------------------
// Prefab Instance Overrides
// ---------------------------------------------------------------------------

/// A single field override for a component on a prefab instance.
///
/// Overrides are applied after instantiation, allowing instances to diverge
/// from the template without duplicating the entire prefab data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentOverride {
    /// The component type name (e.g. "Transform", "PointLight").
    pub component_type: String,
    /// Dot-separated field path within the component (e.g. "position.x").
    pub field_path: String,
    /// The override value (JSON).
    pub value: serde_json::Value,
}

/// Component attached to entities spawned from a prefab, tracking the source
/// template and any per-instance property overrides.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrefabInstance {
    /// The name of the source prefab in the [`PrefabLibrary`].
    pub prefab_id: String,
    /// Per-instance overrides applied on top of the template.
    pub overrides: Vec<ComponentOverride>,
}

impl PrefabInstance {
    pub fn new(prefab_id: impl Into<String>) -> Self {
        Self {
            prefab_id: prefab_id.into(),
            overrides: Vec::new(),
        }
    }

    pub fn with_override(
        mut self,
        component_type: &str,
        field_path: &str,
        value: serde_json::Value,
    ) -> Self {
        self.overrides.push(ComponentOverride {
            component_type: component_type.to_string(),
            field_path: field_path.to_string(),
            value,
        });
        self
    }
}

/// Apply [`ComponentOverride`]s from a [`PrefabInstance`] to a Transform
/// component and any types registered in the [`OverrideRegistry`].
pub fn apply_overrides(world: &mut World, entity: Entity, overrides: &[ComponentOverride]) {
    // First, apply built-in Transform overrides.
    for ovr in overrides {
        if ovr.component_type == "Transform" {
            if let Some(t) = world.get_mut::<Transform>(entity) {
                if let Some(field) = TransformField::from_path(&ovr.field_path) {
                    if let Some(v) = ovr.value.as_f64() {
                        field.set(t, v as f32);
                    }
                } else {
                    log::warn!("Unknown override path '{}' for Transform", ovr.field_path);
                }
            }
        }
    }

    // Then, apply registered override handlers for other component types.
    // We collect handler function pointers first to avoid borrowing issues.
    let handlers: Vec<(String, OverrideHandlerFn)> = world
        .resource::<OverrideRegistry>()
        .map(|reg| reg.handlers.iter().map(|(k, v)| (k.clone(), *v)).collect())
        .unwrap_or_default();

    for ovr in overrides {
        if ovr.component_type == "Transform" {
            continue; // already handled above
        }
        if let Some((_, handler)) = handlers.iter().find(|(k, _)| k == &ovr.component_type) {
            handler(world, entity, &ovr.field_path, &ovr.value);
        } else {
            log::warn!(
                "No override handler registered for component type '{}'",
                ovr.component_type
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Override Registry
// ---------------------------------------------------------------------------

/// Function pointer type for component override handlers.
///
/// Arguments: `(world, entity, field_path, json_value)`.
pub type OverrideHandlerFn = fn(&mut World, Entity, &str, &serde_json::Value);

/// Resource that holds per-component-type override handler functions.
///
/// Downstream crates (e.g. quasar-render, quasar-physics) register handlers
/// in their `Plugin::build()` so that prefab overrides can target their
/// component types without quasar-core knowing about them at compile time.
///
/// ```ignore
/// registry.register("PointLight", |world, entity, field, value| {
///     if let Some(light) = world.get_mut::<PointLight>(entity) {
///         match field {
///             "intensity" => if let Some(v) = value.as_f64() { light.intensity = v as f32; },
///             "range"     => if let Some(v) = value.as_f64() { light.range = v as f32; },
///             _ => {}
///         }
///     }
/// });
/// ```
#[derive(Default)]
pub struct OverrideRegistry {
    handlers: std::collections::HashMap<String, OverrideHandlerFn>,
}

impl OverrideRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a handler for a component type name.
    pub fn register(&mut self, component_type: impl Into<String>, handler: OverrideHandlerFn) {
        self.handlers.insert(component_type.into(), handler);
    }
}

// ---------------------------------------------------------------------------
// Prefab Asset
// ---------------------------------------------------------------------------

/// Resource that holds named prefabs.
#[derive(Debug, Clone, Default)]
pub struct PrefabLibrary {
    prefabs: std::collections::HashMap<String, Prefab>,
}

impl PrefabLibrary {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, prefab: Prefab) {
        self.prefabs.insert(prefab.name.clone(), prefab);
    }

    pub fn get(&self, name: &str) -> Option<&Prefab> {
        self.prefabs.get(name)
    }

    pub fn remove(&mut self, name: &str) -> Option<Prefab> {
        self.prefabs.remove(name)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.prefabs.keys().map(|s| s.as_str())
    }
}

// ---------------------------------------------------------------------------
// Prefab Override Diffing & Propagation
// ---------------------------------------------------------------------------

/// Describes a single field difference between a prefab template and an instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrefabFieldDiff {
    /// Component type (e.g. "Transform").
    pub component_type: String,
    /// Dot-separated field path (e.g. "position.x").
    pub field_path: String,
    /// Value in the base template.
    pub base_value: serde_json::Value,
    /// Current value on the instance.
    pub instance_value: serde_json::Value,
}

/// Compute the diff between a prefab instance's current Transform and its
/// base template transform at `entity_index` within the prefab.
///
/// Returns a list of fields that differ, regardless of whether they are
/// covered by an explicit override.
pub fn diff_instance_transform(
    world: &World,
    entity: Entity,
    base_entity: &PrefabEntity,
) -> Vec<PrefabFieldDiff> {
    let mut diffs = Vec::new();
    let Some(t) = world.get::<Transform>(entity) else {
        return diffs;
    };
    let b = &base_entity.transform;

    for field in TransformField::ALL {
        let inst_val = field.get(t);
        let base_val = field.get(b);
        if (inst_val - base_val).abs() > f32::EPSILON {
            diffs.push(PrefabFieldDiff {
                component_type: "Transform".into(),
                field_path: field.path().into(),
                base_value: serde_json::json!(base_val),
                instance_value: serde_json::json!(inst_val),
            });
        }
    }

    diffs
}

/// Returns `true` if the given `(component_type, field_path)` pair is
/// covered by an explicit override in the [`PrefabInstance`].
pub fn is_field_overridden(
    instance: &PrefabInstance,
    component_type: &str,
    field_path: &str,
) -> bool {
    instance
        .overrides
        .iter()
        .any(|o| o.component_type == component_type && o.field_path == field_path)
}

/// Propagate base prefab changes to all instances in the world.
///
/// For every entity carrying a [`PrefabInstance`], look up the base prefab
/// in the [`PrefabLibrary`], and re-apply base field values for any field
/// that is **not** explicitly overridden. Overridden fields are left
/// untouched.
///
/// This should be called after a prefab asset is reloaded.
pub fn propagate_prefab_changes(world: &mut World) {
    // Collect instance info first to avoid borrow conflicts.
    let instances: Vec<(Entity, PrefabInstance)> = world
        .query::<PrefabInstance>()
        .into_iter()
        .map(|(e, inst)| (e, inst.clone()))
        .collect();

    // Need the base prefab data — collect once.
    let prefab_lookup: std::collections::HashMap<String, Vec<PrefabEntity>> = world
        .resource::<PrefabLibrary>()
        .map(|lib| {
            lib.prefabs
                .iter()
                .map(|(k, v)| (k.clone(), v.entities.clone()))
                .collect()
        })
        .unwrap_or_default();

    for (entity, inst) in &instances {
        let Some(base_entities) = prefab_lookup.get(&inst.prefab_id) else {
            continue;
        };
        // For simplicity, apply the first template entity's transform to
        // each instance entity. Multi-entity prefabs would need an index.
        let Some(base) = base_entities.first() else {
            continue;
        };

        // Re-apply non-overridden Transform fields from the base.
        if let Some(t) = world.get_mut::<Transform>(*entity) {
            for field in TransformField::ALL {
                if !is_field_overridden(inst, "Transform", field.path()) {
                    field.set(t, field.get(&base.transform));
                }
            }
        }

        // Re-apply explicit overrides (they win over the base).
        apply_overrides(world, *entity, &inst.overrides);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // TransformField enum tests
    // -----------------------------------------------------------------------

    #[test]
    fn transform_field_all_has_10_variants() {
        assert_eq!(TransformField::ALL.len(), 10);
    }

    #[test]
    fn transform_field_from_path_roundtrip() {
        for field in TransformField::ALL {
            let path = field.path();
            let parsed = TransformField::from_path(path);
            assert_eq!(parsed, Some(*field), "from_path('{path}') should return the original field");
        }
    }

    #[test]
    fn transform_field_from_path_returns_none_for_unknown() {
        assert_eq!(TransformField::from_path("unknown.field"), None);
        assert_eq!(TransformField::from_path("position"), None);
        assert_eq!(TransformField::from_path(""), None);
    }

    #[test]
    fn transform_field_path_returns_correct_strings() {
        assert_eq!(TransformField::PositionX.path(), "position.x");
        assert_eq!(TransformField::PositionY.path(), "position.y");
        assert_eq!(TransformField::PositionZ.path(), "position.z");
        assert_eq!(TransformField::ScaleX.path(), "scale.x");
        assert_eq!(TransformField::ScaleY.path(), "scale.y");
        assert_eq!(TransformField::ScaleZ.path(), "scale.z");
        assert_eq!(TransformField::RotationX.path(), "rotation.x");
        assert_eq!(TransformField::RotationY.path(), "rotation.y");
        assert_eq!(TransformField::RotationZ.path(), "rotation.z");
        assert_eq!(TransformField::RotationW.path(), "rotation.w");
    }

    #[test]
    fn transform_field_get_position() {
        let t = Transform {
            position: glam::Vec3::new(1.0, 2.0, 3.0),
            rotation: glam::Quat::IDENTITY,
            scale: glam::Vec3::ONE,
        };
        assert!((TransformField::PositionX.get(&t) - 1.0).abs() < 1e-6);
        assert!((TransformField::PositionY.get(&t) - 2.0).abs() < 1e-6);
        assert!((TransformField::PositionZ.get(&t) - 3.0).abs() < 1e-6);
    }

    #[test]
    fn transform_field_get_scale() {
        let t = Transform {
            position: glam::Vec3::ZERO,
            rotation: glam::Quat::IDENTITY,
            scale: glam::Vec3::new(0.5, 2.0, 3.0),
        };
        assert!((TransformField::ScaleX.get(&t) - 0.5).abs() < 1e-6);
        assert!((TransformField::ScaleY.get(&t) - 2.0).abs() < 1e-6);
        assert!((TransformField::ScaleZ.get(&t) - 3.0).abs() < 1e-6);
    }

    #[test]
    fn transform_field_get_rotation() {
        let t = Transform {
            position: glam::Vec3::ZERO,
            rotation: glam::Quat::from_xyzw(0.1, 0.2, 0.3, 0.4),
            scale: glam::Vec3::ONE,
        };
        assert!((TransformField::RotationX.get(&t) - 0.1).abs() < 1e-6);
        assert!((TransformField::RotationY.get(&t) - 0.2).abs() < 1e-6);
        assert!((TransformField::RotationZ.get(&t) - 0.3).abs() < 1e-6);
        assert!((TransformField::RotationW.get(&t) - 0.4).abs() < 1e-6);
    }

    #[test]
    fn transform_field_set_position() {
        let mut t = Transform::IDENTITY;
        TransformField::PositionX.set(&mut t, 10.0);
        TransformField::PositionY.set(&mut t, 20.0);
        TransformField::PositionZ.set(&mut t, 30.0);
        assert!((t.position.x - 10.0).abs() < 1e-6);
        assert!((t.position.y - 20.0).abs() < 1e-6);
        assert!((t.position.z - 30.0).abs() < 1e-6);
    }

    #[test]
    fn transform_field_set_scale() {
        let mut t = Transform::IDENTITY;
        TransformField::ScaleX.set(&mut t, 0.5);
        TransformField::ScaleY.set(&mut t, 1.5);
        TransformField::ScaleZ.set(&mut t, 2.5);
        assert!((t.scale.x - 0.5).abs() < 1e-6);
        assert!((t.scale.y - 1.5).abs() < 1e-6);
        assert!((t.scale.z - 2.5).abs() < 1e-6);
    }

    #[test]
    fn transform_field_set_rotation() {
        let mut t = Transform::IDENTITY;
        TransformField::RotationX.set(&mut t, 0.1);
        TransformField::RotationY.set(&mut t, 0.2);
        TransformField::RotationZ.set(&mut t, 0.3);
        TransformField::RotationW.set(&mut t, 0.4);
        assert!((t.rotation.x - 0.1).abs() < 1e-6);
        assert!((t.rotation.y - 0.2).abs() < 1e-6);
        assert!((t.rotation.z - 0.3).abs() < 1e-6);
        assert!((t.rotation.w - 0.4).abs() < 1e-6);
    }

    #[test]
    fn transform_field_get_set_roundtrip() {
        let mut t = Transform::IDENTITY;
        for field in TransformField::ALL {
            let original = field.get(&t);
            field.set(&mut t, 42.0);
            assert!((field.get(&t) - 42.0).abs() < 1e-6);
            // Restore original to not corrupt the transform
            field.set(&mut t, original);
        }
    }

    #[test]
    fn transform_field_all_paths_are_unique() {
        let paths: std::collections::HashSet<&str> =
            TransformField::ALL.iter().map(|f| f.path()).collect();
        assert_eq!(paths.len(), 10, "All field paths should be unique");
    }
}
