//! Delta compression for network entity replication.
//!
//! Only components that changed since the last acknowledged client baseline
//! are transmitted.  Each [`EntityDelta`] carries a 64-bit change mask where
//! each bit corresponds to a registered component slot.

use serde::{Deserialize, Serialize};

use crate::network::{ClientId, NetworkEntityId};

/// Maximum number of component types tracked per entity (one bit per type).
pub const MAX_COMPONENT_SLOTS: usize = 64;

/// A delta update for a single entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityDelta {
    /// The entity this delta applies to.
    pub entity_id: NetworkEntityId,
    /// Bitmask of changed component slots (bit *i* ⇒ slot *i* changed).
    pub changed_mask: u64,
    /// Serialized data for each changed component, in order of set bits.
    pub component_data: Vec<u8>,
}

impl EntityDelta {
    /// Create a new empty delta for an entity.
    pub fn new(entity_id: NetworkEntityId) -> Self {
        Self {
            entity_id,
            changed_mask: 0,
            component_data: Vec::new(),
        }
    }

    /// Mark slot `index` as changed and append its serialised bytes.
    pub fn set_component(&mut self, slot: usize, data: &[u8]) {
        if slot >= MAX_COMPONENT_SLOTS {
            return;
        }
        self.changed_mask |= 1u64 << slot;
        // Length-prefix each component so the receiver can split them.
        self.component_data
            .extend_from_slice(&(data.len() as u32).to_le_bytes());
        self.component_data.extend_from_slice(data);
    }

    /// Number of components in this delta.
    pub fn component_count(&self) -> u32 {
        self.changed_mask.count_ones()
    }

    /// Iterate over `(slot_index, &[u8])` pairs for each changed component.
    pub fn iter_components(&self) -> DeltaComponentIter<'_> {
        DeltaComponentIter {
            mask: self.changed_mask,
            data: &self.component_data,
            offset: 0,
            bit: 0,
        }
    }
}

/// Iterator over the components encoded in an [`EntityDelta`].
pub struct DeltaComponentIter<'a> {
    mask: u64,
    data: &'a [u8],
    offset: usize,
    bit: usize,
}

impl<'a> Iterator for DeltaComponentIter<'a> {
    type Item = (usize, &'a [u8]);

    fn next(&mut self) -> Option<Self::Item> {
        while self.bit < MAX_COMPONENT_SLOTS {
            let b = self.bit;
            self.bit += 1;
            if self.mask & (1u64 << b) != 0 {
                if self.offset + 4 > self.data.len() {
                    return None;
                }
                let len = u32::from_le_bytes([
                    self.data[self.offset],
                    self.data[self.offset + 1],
                    self.data[self.offset + 2],
                    self.data[self.offset + 3],
                ]) as usize;
                self.offset += 4;
                if self.offset + len > self.data.len() {
                    return None;
                }
                let slice = &self.data[self.offset..self.offset + len];
                self.offset += len;
                return Some((b, slice));
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Baseline tracking
// ---------------------------------------------------------------------------

/// Per-client baseline for delta computation.
///
/// The server keeps the last acknowledged state for each client so it can
/// diff against the current state and send only changes.
pub struct ClientBaseline {
    pub client_id: ClientId,
    /// Last acknowledged server tick for this client.
    pub acked_tick: u64,
    /// Per-entity component hashes at the acked tick.
    /// Key = entity net id, Value = per-slot hash (0 = absent).
    pub entity_hashes: std::collections::HashMap<NetworkEntityId, [u64; MAX_COMPONENT_SLOTS]>,
}

impl ClientBaseline {
    pub fn new(client_id: ClientId) -> Self {
        Self {
            client_id,
            acked_tick: 0,
            entity_hashes: std::collections::HashMap::new(),
        }
    }

    /// Update the baseline when the client acknowledges a tick.
    pub fn acknowledge(&mut self, tick: u64, current_hashes: &std::collections::HashMap<NetworkEntityId, [u64; MAX_COMPONENT_SLOTS]>) {
        self.acked_tick = tick;
        self.entity_hashes = current_hashes.clone();
    }

    /// Compute an [`EntityDelta`] for `entity_id` by comparing current hashes
    /// and serialized data against the baseline.
    ///
    /// `current_hashes` — per-slot content hash for the entity's current state.
    /// `serialize_slot` — closure that serializes slot `i` into bytes.
    pub fn compute_delta<F>(
        &self,
        entity_id: NetworkEntityId,
        current_hashes: &[u64; MAX_COMPONENT_SLOTS],
        mut serialize_slot: F,
    ) -> EntityDelta
    where
        F: FnMut(usize) -> Option<Vec<u8>>,
    {
        let baseline = self
            .entity_hashes
            .get(&entity_id)
            .copied()
            .unwrap_or([0u64; MAX_COMPONENT_SLOTS]);

        let mut delta = EntityDelta::new(entity_id);

        for slot in 0..MAX_COMPONENT_SLOTS {
            if current_hashes[slot] != baseline[slot] {
                if let Some(data) = serialize_slot(slot) {
                    delta.set_component(slot, &data);
                }
            }
        }

        delta
    }
}

/// A bundle of entity deltas for a single network frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaFrame {
    /// Server tick this delta represents.
    pub tick: u64,
    /// Deltas for entities that changed since the client's baseline.
    pub deltas: Vec<EntityDelta>,
}

impl DeltaFrame {
    pub fn new(tick: u64) -> Self {
        Self {
            tick,
            deltas: Vec::new(),
        }
    }

    /// Total serialised byte count of all deltas in this frame.
    pub fn byte_size(&self) -> usize {
        self.deltas.iter().map(|d| 8 + 8 + 4 + d.component_data.len()).sum()
    }
}
