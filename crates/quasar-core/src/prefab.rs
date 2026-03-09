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
        Self::from_json(&json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
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

    pub fn with_override(mut self, component_type: &str, field_path: &str, value: serde_json::Value) -> Self {
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
                match ovr.field_path.as_str() {
                    "position.x" => if let Some(v) = ovr.value.as_f64() { t.position.x = v as f32; },
                    "position.y" => if let Some(v) = ovr.value.as_f64() { t.position.y = v as f32; },
                    "position.z" => if let Some(v) = ovr.value.as_f64() { t.position.z = v as f32; },
                    "scale.x" => if let Some(v) = ovr.value.as_f64() { t.scale.x = v as f32; },
                    "scale.y" => if let Some(v) = ovr.value.as_f64() { t.scale.y = v as f32; },
                    "scale.z" => if let Some(v) = ovr.value.as_f64() { t.scale.z = v as f32; },
                    "rotation.x" => if let Some(v) = ovr.value.as_f64() { t.rotation.x = v as f32; },
                    "rotation.y" => if let Some(v) = ovr.value.as_f64() { t.rotation.y = v as f32; },
                    "rotation.z" => if let Some(v) = ovr.value.as_f64() { t.rotation.z = v as f32; },
                    "rotation.w" => if let Some(v) = ovr.value.as_f64() { t.rotation.w = v as f32; },
                    _ => {
                        log::warn!(
                            "Unknown override path '{}' for Transform",
                            ovr.field_path
                        );
                    }
                }
            }
        }
    }

    // Then, apply registered override handlers for other component types.
    // We collect handler function pointers first to avoid borrowing issues.
    let handlers: Vec<(String, OverrideHandlerFn)> = world
        .resource::<OverrideRegistry>()
        .map(|reg| {
            reg.handlers
                .iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect()
        })
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

    macro_rules! check {
        ($comp:literal, $path:literal, $inst:expr, $base:expr) => {
            if ($inst - $base).abs() > f32::EPSILON {
                diffs.push(PrefabFieldDiff {
                    component_type: $comp.into(),
                    field_path: $path.into(),
                    base_value: serde_json::json!($base),
                    instance_value: serde_json::json!($inst),
                });
            }
        };
    }
    check!("Transform", "position.x", t.position.x, b.position.x);
    check!("Transform", "position.y", t.position.y, b.position.y);
    check!("Transform", "position.z", t.position.z, b.position.z);
    check!("Transform", "scale.x", t.scale.x, b.scale.x);
    check!("Transform", "scale.y", t.scale.y, b.scale.y);
    check!("Transform", "scale.z", t.scale.z, b.scale.z);
    check!("Transform", "rotation.x", t.rotation.x, b.rotation.x);
    check!("Transform", "rotation.y", t.rotation.y, b.rotation.y);
    check!("Transform", "rotation.z", t.rotation.z, b.rotation.z);
    check!("Transform", "rotation.w", t.rotation.w, b.rotation.w);

    diffs
}

/// Returns `true` if the given `(component_type, field_path)` pair is
/// covered by an explicit override in the [`PrefabInstance`].
pub fn is_field_overridden(instance: &PrefabInstance, component_type: &str, field_path: &str) -> bool {
    instance.overrides.iter().any(|o| {
        o.component_type == component_type && o.field_path == field_path
    })
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
            if !is_field_overridden(inst, "Transform", "position.x") { t.position.x = base.transform.position.x; }
            if !is_field_overridden(inst, "Transform", "position.y") { t.position.y = base.transform.position.y; }
            if !is_field_overridden(inst, "Transform", "position.z") { t.position.z = base.transform.position.z; }
            if !is_field_overridden(inst, "Transform", "scale.x") { t.scale.x = base.transform.scale.x; }
            if !is_field_overridden(inst, "Transform", "scale.y") { t.scale.y = base.transform.scale.y; }
            if !is_field_overridden(inst, "Transform", "scale.z") { t.scale.z = base.transform.scale.z; }
            if !is_field_overridden(inst, "Transform", "rotation.x") { t.rotation.x = base.transform.rotation.x; }
            if !is_field_overridden(inst, "Transform", "rotation.y") { t.rotation.y = base.transform.rotation.y; }
            if !is_field_overridden(inst, "Transform", "rotation.z") { t.rotation.z = base.transform.rotation.z; }
            if !is_field_overridden(inst, "Transform", "rotation.w") { t.rotation.w = base.transform.rotation.w; }
        }

        // Re-apply explicit overrides (they win over the base).
        apply_overrides(world, *entity, &inst.overrides);
    }
}
