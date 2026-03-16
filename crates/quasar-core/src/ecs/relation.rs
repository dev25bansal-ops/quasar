//! Generic typed entity relationships for the ECS.
//!
//! Relations are directional edges between entities.  `add::<R>(source, target)`
//! means "source **R** target" — for example, `add::<ChildOf>(child, parent)`
//! means "child **ChildOf** parent".
//!
//! The graph maintains both forward (source → targets) and reverse
//! (target → sources) maps so lookups in either direction are O(1) amortised.

use std::any::TypeId;
use std::collections::HashMap;

use super::Entity;

// ------------------------------------------------------------------
// Relation trait + built-in relation types
// ------------------------------------------------------------------

/// Marker trait for relation types.
pub trait Relation: 'static {}

/// Parent-child hierarchy relation.
/// `add::<ChildOf>(child, parent)` — the child is a child of the parent.
pub struct ChildOf;
impl Relation for ChildOf {}

/// Ownership relation — owned entities are cascade-despawned with the owner.
/// `add::<OwnedBy>(owned, owner)` — the owned entity belongs to owner.
pub struct OwnedBy;
impl Relation for OwnedBy {}

// ------------------------------------------------------------------
// RelationGraph
// ------------------------------------------------------------------

/// Stores typed entity relationships as a directed edge graph.
pub struct RelationGraph {
    /// (relation TypeId, source index) → list of target entities
    forward: HashMap<(TypeId, u32), Vec<Entity>>,
    /// (relation TypeId, target index) → list of source entities
    reverse: HashMap<(TypeId, u32), Vec<Entity>>,
}

impl Default for RelationGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl RelationGraph {
    pub fn new() -> Self {
        Self {
            forward: HashMap::new(),
            reverse: HashMap::new(),
        }
    }

    /// Add a directed relation: `source R target`.
    ///
    /// Duplicate edges are silently ignored.
    pub fn add<R: Relation>(&mut self, source: Entity, target: Entity) {
        let rid = TypeId::of::<R>();
        let fwd = self.forward.entry((rid, source.index())).or_default();
        if !fwd.iter().any(|e| e.index() == target.index()) {
            fwd.push(target);
        }
        let rev = self.reverse.entry((rid, target.index())).or_default();
        if !rev.iter().any(|e| e.index() == source.index()) {
            rev.push(source);
        }
    }

    /// Remove a specific directed relation between source and target.
    pub fn remove<R: Relation>(&mut self, source: Entity, target: Entity) {
        let rid = TypeId::of::<R>();
        if let Some(fwd) = self.forward.get_mut(&(rid, source.index())) {
            fwd.retain(|e| e.index() != target.index());
        }
        if let Some(rev) = self.reverse.get_mut(&(rid, target.index())) {
            rev.retain(|e| e.index() != source.index());
        }
    }

    /// Remove **all** relations (of every type) that involve `entity` as
    /// either source or target.  Call this on despawn.
    pub fn remove_entity(&mut self, entity: Entity) {
        let idx = entity.index();

        // Collect keys to avoid borrow conflicts.
        let fwd_keys: Vec<(TypeId, u32)> = self
            .forward
            .keys()
            .filter(|(_, i)| *i == idx)
            .copied()
            .collect();

        for key in fwd_keys {
            if let Some(targets) = self.forward.remove(&key) {
                let rid = key.0;
                for t in targets {
                    if let Some(rev) = self.reverse.get_mut(&(rid, t.index())) {
                        rev.retain(|e| e.index() != idx);
                    }
                }
            }
        }

        let rev_keys: Vec<(TypeId, u32)> = self
            .reverse
            .keys()
            .filter(|(_, i)| *i == idx)
            .copied()
            .collect();

        for key in rev_keys {
            if let Some(sources) = self.reverse.remove(&key) {
                let rid = key.0;
                for s in sources {
                    if let Some(fwd) = self.forward.get_mut(&(rid, s.index())) {
                        fwd.retain(|e| e.index() != idx);
                    }
                }
            }
        }
    }

    /// Get all targets that `source` has relation `R` to.
    pub fn targets<R: Relation>(&self, source: Entity) -> &[Entity] {
        self.forward
            .get(&(TypeId::of::<R>(), source.index()))
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Get all sources that have relation `R` **to** `target`.
    pub fn sources<R: Relation>(&self, target: Entity) -> &[Entity] {
        self.reverse
            .get(&(TypeId::of::<R>(), target.index()))
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Check whether `source R target` exists.
    pub fn has<R: Relation>(&self, source: Entity, target: Entity) -> bool {
        self.forward
            .get(&(TypeId::of::<R>(), source.index()))
            .is_some_and(|v| v.iter().any(|e| e.index() == target.index()))
    }

    /// Collect all entities transitively owned by `owner` via [`OwnedBy`].
    /// Useful for cascade despawn.
    pub fn owned_recursive(&self, owner: Entity) -> Vec<Entity> {
        let mut result = Vec::new();
        self.collect_owned(owner, &mut result);
        result
    }

    fn collect_owned(&self, owner: Entity, out: &mut Vec<Entity>) {
        // sources of OwnedBy pointing at `owner` = entities owned by owner
        for &e in self.sources::<OwnedBy>(owner) {
            out.push(e);
            self.collect_owned(e, out);
        }
    }
}

// ------------------------------------------------------------------
// Tests
// ------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::Entity;

    struct Likes;
    impl Relation for Likes {}

    fn e(idx: u32) -> Entity {
        Entity::new(idx, 0)
    }

    #[test]
    fn add_and_query() {
        let mut g = RelationGraph::new();
        let a = e(0);
        let b = e(1);
        g.add::<ChildOf>(a, b);

        assert!(g.has::<ChildOf>(a, b));
        assert!(!g.has::<ChildOf>(b, a));
        assert_eq!(g.targets::<ChildOf>(a).len(), 1);
        assert_eq!(g.sources::<ChildOf>(b).len(), 1);
    }

    #[test]
    fn duplicate_ignored() {
        let mut g = RelationGraph::new();
        let a = e(0);
        let b = e(1);
        g.add::<ChildOf>(a, b);
        g.add::<ChildOf>(a, b);

        assert_eq!(g.targets::<ChildOf>(a).len(), 1);
    }

    #[test]
    fn remove_relation() {
        let mut g = RelationGraph::new();
        let a = e(0);
        let b = e(1);
        g.add::<Likes>(a, b);
        g.remove::<Likes>(a, b);

        assert!(!g.has::<Likes>(a, b));
        assert!(g.targets::<Likes>(a).is_empty());
        assert!(g.sources::<Likes>(b).is_empty());
    }

    #[test]
    fn remove_entity_cleans_all() {
        let mut g = RelationGraph::new();
        let a = e(0);
        let b = e(1);
        let c = e(2);

        g.add::<ChildOf>(a, b);
        g.add::<ChildOf>(c, b);
        g.add::<Likes>(b, a);

        g.remove_entity(b);

        assert!(!g.has::<ChildOf>(a, b));
        assert!(!g.has::<ChildOf>(c, b));
        assert!(!g.has::<Likes>(b, a));
    }

    #[test]
    fn owned_recursive_cascade() {
        let mut g = RelationGraph::new();
        let owner = e(0);
        let a = e(1);
        let b = e(2);

        g.add::<OwnedBy>(a, owner); // a owned by owner
        g.add::<OwnedBy>(b, a); // b owned by a

        let cascade = g.owned_recursive(owner);
        assert_eq!(cascade.len(), 2);
    }
}
