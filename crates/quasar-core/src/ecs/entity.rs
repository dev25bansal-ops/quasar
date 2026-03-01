//! Entity — a lightweight handle to an object in the game world.
//!
//! Entities use generational indices to prevent dangling references:
//! when an entity is despawned and its slot reused, the old `Entity`
//! handle will fail to match because the generation has incremented.

use std::fmt;

/// A unique handle identifying an entity in the [`World`](super::World).
///
/// Consists of an index (slot) and a generation counter. Two `Entity` values
/// with the same index but different generations refer to different logical
/// entities.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Entity {
    /// Slot index in the entity allocator.
    pub(crate) index: u32,
    /// Generation counter — incremented each time a slot is recycled.
    pub(crate) generation: u32,
}

impl Entity {
    /// Create a new entity handle. Typically only called by [`World`](super::World).
    pub(crate) fn new(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }

    /// Returns the raw index of this entity.
    #[inline]
    pub fn index(&self) -> u32 {
        self.index
    }

    /// Returns the generation of this entity.
    #[inline]
    pub fn generation(&self) -> u32 {
        self.generation
    }
}

impl fmt::Debug for Entity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Entity({}v{})", self.index, self.generation)
    }
}

impl fmt::Display for Entity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}v{}", self.index, self.generation)
    }
}

/// Allocator that hands out [`Entity`] handles with generational recycling.
pub(crate) struct EntityAllocator {
    /// Current generation for each slot. Index = slot.
    generations: Vec<u32>,
    /// Free list of recycled slot indices.
    free_list: Vec<u32>,
    /// Total number of currently alive entities.
    alive_count: u32,
}

impl EntityAllocator {
    pub fn new() -> Self {
        Self {
            generations: Vec::new(),
            free_list: Vec::new(),
            alive_count: 0,
        }
    }

    /// Allocate a fresh [`Entity`].
    pub fn allocate(&mut self) -> Entity {
        self.alive_count += 1;

        if let Some(index) = self.free_list.pop() {
            // Reuse a recycled slot — generation was already bumped on dealloc.
            let generation = self.generations[index as usize];
            Entity::new(index, generation)
        } else {
            // Grow the pool.
            let index = self.generations.len() as u32;
            self.generations.push(0);
            Entity::new(index, 0)
        }
    }

    /// Deallocate an entity, bumping the generation so old handles become stale.
    /// Returns `true` if the entity was alive and successfully deallocated.
    pub fn deallocate(&mut self, entity: Entity) -> bool {
        let idx = entity.index as usize;
        if idx < self.generations.len() && self.generations[idx] == entity.generation {
            self.generations[idx] += 1;
            self.free_list.push(entity.index);
            self.alive_count -= 1;
            true
        } else {
            false
        }
    }

    /// Check whether an entity handle is still valid (alive).
    pub fn is_alive(&self, entity: Entity) -> bool {
        let idx = entity.index as usize;
        idx < self.generations.len() && self.generations[idx] == entity.generation
    }

    /// Number of currently alive entities.
    pub fn alive_count(&self) -> u32 {
        self.alive_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocate_and_deallocate() {
        let mut alloc = EntityAllocator::new();

        let e1 = alloc.allocate();
        let e2 = alloc.allocate();
        assert_eq!(e1.index(), 0);
        assert_eq!(e2.index(), 1);
        assert_eq!(alloc.alive_count(), 2);

        assert!(alloc.deallocate(e1));
        assert!(!alloc.is_alive(e1));
        assert!(alloc.is_alive(e2));

        // Reuse slot 0 with bumped generation.
        let e3 = alloc.allocate();
        assert_eq!(e3.index(), 0);
        assert_eq!(e3.generation(), 1);
        assert_ne!(e1, e3);
    }
}
