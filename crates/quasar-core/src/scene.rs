//! Scene graph — hierarchical parent-child entity relationships and scene management.
//!
//! Supports:
//! - Parent-child relationships between entities
//! - Scene trees with arbitrary depth
//! - Scene lifecycle (create, activate, deactivate)
//! - Named entities for easy lookup

use std::collections::HashMap;

use crate::Entity;

/// Tracks parent → children and child → parent relationships.
///
/// Stored as a resource in the ECS `World`. Systems can query the hierarchy
/// to walk parent chains, enumerate children, etc.
#[derive(Debug, Default)]
pub struct SceneGraph {
    /// child → parent
    parents: HashMap<u32, Entity>,
    /// parent → children (ordered)
    children: HashMap<u32, Vec<Entity>>,
    /// entity → name
    names: HashMap<u32, String>,
    /// name → entity (fast reverse lookup)
    name_lookup: HashMap<String, Entity>,
}

impl SceneGraph {
    pub fn new() -> Self {
        Self::default()
    }

    // ------------------------------------------------------------------
    // Hierarchy
    // ------------------------------------------------------------------

    /// Set `child` as a child of `parent`.
    pub fn set_parent(&mut self, child: Entity, parent: Entity) {
        // Remove from old parent if any.
        if let Some(old_parent) = self.parents.get(&child.index()) {
            let old_idx = old_parent.index();
            if let Some(siblings) = self.children.get_mut(&old_idx) {
                siblings.retain(|e| e.index() != child.index());
            }
        }
        self.parents.insert(child.index(), parent);
        self.children
            .entry(parent.index())
            .or_default()
            .push(child);
    }

    /// Remove parent relationship for `child`.
    pub fn unparent(&mut self, child: Entity) {
        if let Some(parent) = self.parents.remove(&child.index()) {
            if let Some(siblings) = self.children.get_mut(&parent.index()) {
                siblings.retain(|e| e.index() != child.index());
            }
        }
    }

    /// Get the parent of an entity, if any.
    pub fn parent(&self, child: Entity) -> Option<Entity> {
        self.parents.get(&child.index()).copied()
    }

    /// Get the children of an entity.
    pub fn children(&self, parent: Entity) -> &[Entity] {
        self.children
            .get(&parent.index())
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Check if an entity has children.
    pub fn has_children(&self, entity: Entity) -> bool {
        self.children
            .get(&entity.index())
            .map_or(false, |v| !v.is_empty())
    }

    /// Get all root entities (entities without parents).
    pub fn roots(&self, all_entities: &[Entity]) -> Vec<Entity> {
        all_entities
            .iter()
            .filter(|e| !self.parents.contains_key(&e.index()))
            .copied()
            .collect()
    }

    /// Walk the ancestor chain from `entity` up to the root. Returns the chain
    /// **not** including `entity` itself.
    pub fn ancestors(&self, entity: Entity) -> Vec<Entity> {
        let mut chain = Vec::new();
        let mut current = entity;
        while let Some(p) = self.parents.get(&current.index()) {
            chain.push(*p);
            current = *p;
        }
        chain
    }

    /// Recursively collect all descendants of `entity`.
    pub fn descendants(&self, entity: Entity) -> Vec<Entity> {
        let mut result = Vec::new();
        self.collect_descendants(entity, &mut result);
        result
    }

    fn collect_descendants(&self, entity: Entity, out: &mut Vec<Entity>) {
        for &child in self.children(entity) {
            out.push(child);
            self.collect_descendants(child, out);
        }
    }

    // ------------------------------------------------------------------
    // Named entities
    // ------------------------------------------------------------------

    /// Assign a name to an entity.
    pub fn set_name(&mut self, entity: Entity, name: impl Into<String>) {
        let name = name.into();
        // Remove old name if any.
        if let Some(old_name) = self.names.get(&entity.index()) {
            self.name_lookup.remove(old_name);
        }
        self.name_lookup.insert(name.clone(), entity);
        self.names.insert(entity.index(), name);
    }

    /// Get the name of an entity.
    pub fn name(&self, entity: Entity) -> Option<&str> {
        self.names.get(&entity.index()).map(|s| s.as_str())
    }

    /// Look up an entity by name.
    pub fn find_by_name(&self, name: &str) -> Option<Entity> {
        self.name_lookup.get(name).copied()
    }

    // ------------------------------------------------------------------
    // Cleanup
    // ------------------------------------------------------------------

    /// Remove all hierarchy and name data for an entity (call on despawn).
    pub fn remove_entity(&mut self, entity: Entity) {
        // Reparent children to grandparent or make them roots.
        let old_parent = self.parents.remove(&entity.index());
        if let Some(children) = self.children.remove(&entity.index()) {
            for child in &children {
                if let Some(parent) = old_parent {
                    self.parents.insert(child.index(), parent);
                    self.children
                        .entry(parent.index())
                        .or_default()
                        .push(*child);
                } else {
                    self.parents.remove(&child.index());
                }
            }
        }

        // Remove from parent's child list.
        if let Some(parent) = old_parent {
            if let Some(siblings) = self.children.get_mut(&parent.index()) {
                siblings.retain(|e| e.index() != entity.index());
            }
        }

        // Remove name.
        if let Some(name) = self.names.remove(&entity.index()) {
            self.name_lookup.remove(&name);
        }
    }
}

/// A named scene that can be loaded/unloaded.
///
/// Scenes group related entities and can be activated/deactivated as a unit —
/// useful for level management, UI screens, etc.
#[derive(Debug)]
pub struct Scene {
    pub name: String,
    pub root_entities: Vec<Entity>,
    pub active: bool,
}

impl Scene {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            root_entities: Vec::new(),
            active: true,
        }
    }

    /// Add a root entity to this scene.
    pub fn add_root(&mut self, entity: Entity) {
        self.root_entities.push(entity);
    }

    /// Deactivate this scene.
    pub fn deactivate(&mut self) {
        self.active = false;
    }

    /// Activate this scene.
    pub fn activate(&mut self) {
        self.active = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::World;

    #[test]
    fn parent_child_relationships() {
        let mut world = World::new();
        let parent = world.spawn();
        let child_a = world.spawn();
        let child_b = world.spawn();

        let mut graph = SceneGraph::new();
        graph.set_parent(child_a, parent);
        graph.set_parent(child_b, parent);

        assert_eq!(graph.parent(child_a), Some(parent));
        assert_eq!(graph.children(parent).len(), 2);
    }

    #[test]
    fn named_entities() {
        let mut world = World::new();
        let e = world.spawn();

        let mut graph = SceneGraph::new();
        graph.set_name(e, "Player");

        assert_eq!(graph.name(e), Some("Player"));
        assert_eq!(graph.find_by_name("Player"), Some(e));
        assert_eq!(graph.find_by_name("Missing"), None);
    }

    #[test]
    fn descendants_collect() {
        let mut world = World::new();
        let root = world.spawn();
        let child = world.spawn();
        let grandchild = world.spawn();

        let mut graph = SceneGraph::new();
        graph.set_parent(child, root);
        graph.set_parent(grandchild, child);

        let desc = graph.descendants(root);
        assert_eq!(desc.len(), 2);
    }

    #[test]
    fn unparent_removes_relationship() {
        let mut world = World::new();
        let parent = world.spawn();
        let child = world.spawn();

        let mut graph = SceneGraph::new();
        graph.set_parent(child, parent);
        assert_eq!(graph.parent(child), Some(parent));

        graph.unparent(child);
        assert_eq!(graph.parent(child), None);
        assert_eq!(graph.children(parent).len(), 0);
    }
}
