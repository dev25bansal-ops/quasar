//! Interest management — server-side spatial partitioning for network relevance.
//!
//! Entities are bucketed into a uniform grid.  Each client has an Area of
//! Interest (AoI) defined by a position and radius.  Only entities whose grid
//! cell overlaps the client's AoI receive updates.

use std::collections::{HashMap, HashSet};

use crate::network::{ClientId, NetworkEntityId};

/// Side length of each grid cell in world units.
const DEFAULT_CELL_SIZE: f32 = 50.0;

/// A 3-D cell coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CellCoord {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

/// Area of Interest for a single client.
#[derive(Debug, Clone)]
pub struct AreaOfInterest {
    pub position: [f32; 3],
    pub radius: f32,
}

impl Default for AreaOfInterest {
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            radius: 100.0,
        }
    }
}

/// Server-side interest manager backed by a uniform spatial grid.
pub struct InterestManager {
    cell_size: f32,
    /// Entity → position (kept up-to-date every tick).
    entity_positions: HashMap<NetworkEntityId, [f32; 3]>,
    /// Cell → set of entities residing in that cell.
    grid: HashMap<CellCoord, HashSet<NetworkEntityId>>,
    /// Client → AoI.
    client_aois: HashMap<ClientId, AreaOfInterest>,
}

impl InterestManager {
    pub fn new() -> Self {
        Self::with_cell_size(DEFAULT_CELL_SIZE)
    }

    pub fn with_cell_size(cell_size: f32) -> Self {
        Self {
            cell_size: cell_size.max(1.0),
            entity_positions: HashMap::new(),
            grid: HashMap::new(),
            client_aois: HashMap::new(),
        }
    }

    // ------------------------------------------------------------------
    // Entity registration
    // ------------------------------------------------------------------

    /// Register or update an entity's position.
    pub fn update_entity(&mut self, entity_id: NetworkEntityId, position: [f32; 3]) {
        // Remove from old cell if position changed.
        if let Some(old_pos) = self.entity_positions.get(&entity_id) {
            let old_cell = self.cell_of(*old_pos);
            let new_cell = self.cell_of(position);
            if old_cell != new_cell {
                if let Some(set) = self.grid.get_mut(&old_cell) {
                    set.remove(&entity_id);
                    if set.is_empty() {
                        self.grid.remove(&old_cell);
                    }
                }
                self.grid.entry(new_cell).or_default().insert(entity_id);
            }
        } else {
            let cell = self.cell_of(position);
            self.grid.entry(cell).or_default().insert(entity_id);
        }
        self.entity_positions.insert(entity_id, position);
    }

    /// Remove an entity from the grid.
    pub fn remove_entity(&mut self, entity_id: NetworkEntityId) {
        if let Some(pos) = self.entity_positions.remove(&entity_id) {
            let cell = self.cell_of(pos);
            if let Some(set) = self.grid.get_mut(&cell) {
                set.remove(&entity_id);
                if set.is_empty() {
                    self.grid.remove(&cell);
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Client AoI
    // ------------------------------------------------------------------

    /// Set / update the AoI for a client.
    pub fn set_client_aoi(&mut self, client_id: ClientId, aoi: AreaOfInterest) {
        self.client_aois.insert(client_id, aoi);
    }

    /// Remove a client's AoI (e.g. on disconnect).
    pub fn remove_client(&mut self, client_id: &ClientId) {
        self.client_aois.remove(client_id);
    }

    // ------------------------------------------------------------------
    // Queries
    // ------------------------------------------------------------------

    /// Return the set of entities relevant to `client_id`.
    pub fn relevant_entities(&self, client_id: &ClientId) -> HashSet<NetworkEntityId> {
        let aoi = match self.client_aois.get(client_id) {
            Some(a) => a,
            None => return HashSet::new(),
        };

        let cells = self.cells_in_sphere(aoi.position, aoi.radius);
        let radius_sq = aoi.radius * aoi.radius;

        let mut result = HashSet::new();
        for cell in cells {
            if let Some(entities) = self.grid.get(&cell) {
                for &eid in entities {
                    if let Some(pos) = self.entity_positions.get(&eid) {
                        let dx = pos[0] - aoi.position[0];
                        let dy = pos[1] - aoi.position[1];
                        let dz = pos[2] - aoi.position[2];
                        if dx * dx + dy * dy + dz * dz <= radius_sq {
                            result.insert(eid);
                        }
                    }
                }
            }
        }
        result
    }

    /// Check whether a single entity is relevant to a client.
    pub fn is_relevant(&self, client_id: &ClientId, entity_id: &NetworkEntityId) -> bool {
        let aoi = match self.client_aois.get(client_id) {
            Some(a) => a,
            None => return false,
        };
        let pos = match self.entity_positions.get(entity_id) {
            Some(p) => p,
            None => return false,
        };
        let dx = pos[0] - aoi.position[0];
        let dy = pos[1] - aoi.position[1];
        let dz = pos[2] - aoi.position[2];
        dx * dx + dy * dy + dz * dz <= aoi.radius * aoi.radius
    }

    /// Number of entities tracked.
    pub fn entity_count(&self) -> usize {
        self.entity_positions.len()
    }

    /// Number of non-empty grid cells.
    pub fn cell_count(&self) -> usize {
        self.grid.len()
    }

    // ------------------------------------------------------------------
    // Internal helpers
    // ------------------------------------------------------------------

    fn cell_of(&self, pos: [f32; 3]) -> CellCoord {
        CellCoord {
            x: (pos[0] / self.cell_size).floor() as i32,
            y: (pos[1] / self.cell_size).floor() as i32,
            z: (pos[2] / self.cell_size).floor() as i32,
        }
    }

    /// All cells that a sphere at `center` with `radius` could overlap.
    fn cells_in_sphere(&self, center: [f32; 3], radius: f32) -> Vec<CellCoord> {
        let min = [
            ((center[0] - radius) / self.cell_size).floor() as i32,
            ((center[1] - radius) / self.cell_size).floor() as i32,
            ((center[2] - radius) / self.cell_size).floor() as i32,
        ];
        let max = [
            ((center[0] + radius) / self.cell_size).floor() as i32,
            ((center[1] + radius) / self.cell_size).floor() as i32,
            ((center[2] + radius) / self.cell_size).floor() as i32,
        ];

        let mut cells = Vec::new();
        for x in min[0]..=max[0] {
            for y in min[1]..=max[1] {
                for z in min[2]..=max[2] {
                    cells.push(CellCoord { x, y, z });
                }
            }
        }
        cells
    }
}

impl Default for InterestManager {
    fn default() -> Self {
        Self::new()
    }
}
