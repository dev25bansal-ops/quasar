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
    pub fn acknowledge(
        &mut self,
        tick: u64,
        current_hashes: &std::collections::HashMap<NetworkEntityId, [u64; MAX_COMPONENT_SLOTS]>,
    ) {
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
        self.deltas
            .iter()
            .map(|d| 8 + 8 + 4 + d.component_data.len())
            .sum()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // -----------------------------------------------------------------------
    // EntityDelta – basic construction
    // -----------------------------------------------------------------------

    #[test]
    fn entity_delta_new_is_empty() {
        let entity = NetworkEntityId(42);
        let delta = EntityDelta::new(entity);

        assert_eq!(delta.entity_id, entity);
        assert_eq!(delta.changed_mask, 0);
        assert!(delta.component_data.is_empty());
        assert_eq!(delta.component_count(), 0);
    }

    // -----------------------------------------------------------------------
    // EntityDelta – set_component updates mask and appends data
    // -----------------------------------------------------------------------

    #[test]
    fn set_component_updates_mask_and_appends_data() {
        let entity = NetworkEntityId(1);
        let mut delta = EntityDelta::new(entity);

        delta.set_component(0, b"hello");
        delta.set_component(3, b"world!");

        // Bit 0 and bit 3 should be set.
        assert_eq!(delta.changed_mask, (1u64 << 0) | (1u64 << 3));
        assert_eq!(delta.component_count(), 2);

        // component_data should contain two length-prefixed blobs.
        // Slot 0: 5 bytes "hello" → [5, 0, 0, 0, ...bytes]
        // Slot 3: 6 bytes "world!" → [6, 0, 0, 0, ...bytes]
        let expected_len = (4 + 5) + (4 + 6);
        assert_eq!(delta.component_data.len(), expected_len);

        // Verify first component
        let first_len = u32::from_le_bytes([
            delta.component_data[0],
            delta.component_data[1],
            delta.component_data[2],
            delta.component_data[3],
        ]) as usize;
        assert_eq!(first_len, 5);
        assert_eq!(&delta.component_data[4..9], b"hello");

        // Verify second component
        let second_len = u32::from_le_bytes([
            delta.component_data[9],
            delta.component_data[10],
            delta.component_data[11],
            delta.component_data[12],
        ]) as usize;
        assert_eq!(second_len, 6);
        assert_eq!(&delta.component_data[13..19], b"world!");
    }

    // -----------------------------------------------------------------------
    // EntityDelta – overflow protection (slot >= 64)
    // -----------------------------------------------------------------------

    #[test]
    fn set_component_ignores_overflow_slots() {
        let entity = NetworkEntityId(2);
        let mut delta = EntityDelta::new(entity);

        // Valid slot
        delta.set_component(63, b"data");
        assert_eq!(delta.changed_mask, 1u64 << 63);
        assert_eq!(delta.component_count(), 1);

        // Out-of-range slots should be silently ignored.
        delta.set_component(64, b"should_be_ignored");
        delta.set_component(100, b"also_ignored");
        delta.set_component(usize::MAX, b"definitely_ignored");

        // Mask and data should be unchanged.
        assert_eq!(delta.changed_mask, 1u64 << 63);
        assert_eq!(delta.component_count(), 1);
    }

    // -----------------------------------------------------------------------
    // DeltaComponentIter – roundtrip: set components → iterate → verify
    // -----------------------------------------------------------------------

    #[test]
    fn delta_component_iter_roundtrip() {
        let entity = NetworkEntityId(7);
        let mut delta = EntityDelta::new(entity);

        let components = vec![
            (0, vec![1, 2, 3]),
            (5, vec![10, 20, 30, 40]),
            (10, vec![255]),
            (63, vec![99, 88]),
        ];

        for (slot, data) in &components {
            delta.set_component(*slot, data);
        }

        assert_eq!(delta.component_count(), 4);

        let collected: Vec<_> = delta.iter_components().collect();
        assert_eq!(collected.len(), 4);

        for (i, (slot, data)) in collected.iter().enumerate() {
            assert_eq!(*slot, components[i].0);
            assert_eq!(*data, components[i].1);
        }
    }

    // -----------------------------------------------------------------------
    // DeltaComponentIter – empty delta yields no items
    // -----------------------------------------------------------------------

    #[test]
    fn delta_component_iter_empty() {
        let entity = NetworkEntityId(99);
        let delta = EntityDelta::new(entity);

        let mut iter = delta.iter_components();
        assert!(iter.next().is_none());
        assert_eq!(delta.component_count(), 0);
    }

    // -----------------------------------------------------------------------
    // DeltaComponentIter – handles truncated data gracefully
    // -----------------------------------------------------------------------

    #[test]
    fn delta_component_iter_handles_truncated_data() {
        // Manually craft a delta with truncated component data.
        let entity = NetworkEntityId(10);
        let mut delta = EntityDelta::new(entity);

        // Set one valid component.
        delta.set_component(0, b"valid");

        // Manually corrupt the data: set bit 5 but provide no data.
        delta.changed_mask |= 1u64 << 5;
        // component_data is NOT extended → iterator should stop gracefully.

        let collected: Vec<_> = delta.iter_components().collect();
        assert_eq!(collected.len(), 1);
        assert_eq!(collected[0], (0, b"valid".as_slice()));
    }

    #[test]
    fn delta_component_iter_handles_truncated_length_prefix() {
        let entity = NetworkEntityId(11);
        let mut delta = EntityDelta::new(entity);

        // Set bit 0 and bit 1, but only provide 2 bytes of data (need 4 for length).
        delta.changed_mask = 0b11; // slots 0 and 1
        delta.component_data = vec![0xFF, 0xFF]; // truncated length prefix

        let collected: Vec<_> = delta.iter_components().collect();
        // Should return nothing because the length prefix is incomplete.
        assert!(collected.is_empty());
    }

    #[test]
    fn delta_component_iter_handles_truncated_payload() {
        let entity = NetworkEntityId(12);
        let mut delta = EntityDelta::new(entity);

        // Set bit 0, claim 100 bytes but only provide 3.
        delta.changed_mask = 0b1;
        delta.component_data.extend_from_slice(&100u32.to_le_bytes());
        delta.component_data.extend_from_slice(&[1, 2, 3]); // only 3 bytes

        let collected: Vec<_> = delta.iter_components().collect();
        assert!(collected.is_empty());
    }

    // -----------------------------------------------------------------------
    // ClientBaseline – compute delta detects changes
    // -----------------------------------------------------------------------

    #[test]
    fn client_baseline_compute_delta_detects_changes() {
        let client_id = ClientId(100);
        let mut baseline = ClientBaseline::new(client_id);

        let entity = NetworkEntityId(5);

        // Baseline: slot 0 hash = 10, slot 1 hash = 20, others 0.
        let mut baseline_hashes: HashMap<NetworkEntityId, [u64; MAX_COMPONENT_SLOTS]> =
            HashMap::new();
        let mut bh = [0u64; MAX_COMPONENT_SLOTS];
        bh[0] = 10;
        bh[1] = 20;
        baseline_hashes.insert(entity, bh);

        // Acknowledge the baseline.
        baseline.acknowledge(5, &baseline_hashes);

        // Current state: slot 0 unchanged, slot 1 changed, slot 2 is new.
        let mut current = [0u64; MAX_COMPONENT_SLOTS];
        current[0] = 10; // same
        current[1] = 99; // changed
        current[2] = 50; // new

        let serialize_slot = |slot: usize| -> Option<Vec<u8>> {
            match slot {
                1 => Some(vec![0xAA, 0xBB]),
                2 => Some(vec![0xCC]),
                _ => None,
            }
        };

        let delta = baseline.compute_delta(entity, &current, serialize_slot);

        // Should detect changes in slots 1 and 2 only.
        assert_eq!(delta.component_count(), 2);
        assert_eq!(delta.changed_mask, (1u64 << 1) | (1u64 << 2));

        let collected: Vec<_> = delta.iter_components().collect();
        assert_eq!(collected.len(), 2);
        assert_eq!(collected[0], (1, vec![0xAA, 0xBB].as_slice()));
        assert_eq!(collected[1], (2, vec![0xCC].as_slice()));
    }

    #[test]
    fn client_baseline_compute_delta_no_changes() {
        let client_id = ClientId(200);
        let mut baseline = ClientBaseline::new(client_id);

        let entity = NetworkEntityId(3);

        let mut hashes = HashMap::new();
        let mut h = [0u64; MAX_COMPONENT_SLOTS];
        h[0] = 42;
        h[5] = 99;
        hashes.insert(entity, h);

        baseline.acknowledge(10, &hashes);

        // Current state identical to baseline → no delta.
        let delta = baseline.compute_delta(entity, &h, |_slot| Some(vec![1, 2, 3]));

        assert_eq!(delta.component_count(), 0);
        assert!(delta.component_data.is_empty());
    }

    #[test]
    fn client_baseline_compute_delta_new_entity() {
        let client_id = ClientId(300);
        let baseline = ClientBaseline::new(client_id);

        let entity = NetworkEntityId(7);

        // Entity not in baseline → all slots are new.
        let mut current = [0u64; MAX_COMPONENT_SLOTS];
        current[0] = 1;
        current[3] = 2;

        let serialize_slot = |slot: usize| -> Option<Vec<u8>> {
            match slot {
                0 => Some(vec![10]),
                3 => Some(vec![20, 30]),
                _ => None,
            }
        };

        let delta = baseline.compute_delta(entity, &current, serialize_slot);

        assert_eq!(delta.component_count(), 2);
        let collected: Vec<_> = delta.iter_components().collect();
        assert_eq!(collected.len(), 2);
        assert_eq!(collected[0], (0, vec![10].as_slice()));
        assert_eq!(collected[1], (3, vec![20, 30].as_slice()));
    }

    // -----------------------------------------------------------------------
    // ClientBaseline – acknowledge updates tick
    // -----------------------------------------------------------------------

    #[test]
    fn client_baseline_acknowledge_updates_tick() {
        let client_id = ClientId(500);
        let mut baseline = ClientBaseline::new(client_id);

        assert_eq!(baseline.acked_tick, 0);
        assert!(baseline.entity_hashes.is_empty());

        let entity = NetworkEntityId(1);
        let mut hashes = HashMap::new();
        let mut h = [0u64; MAX_COMPONENT_SLOTS];
        h[0] = 123;
        hashes.insert(entity, h);

        baseline.acknowledge(42, &hashes);

        assert_eq!(baseline.acked_tick, 42);
        assert_eq!(baseline.entity_hashes.len(), 1);
        assert_eq!(baseline.entity_hashes[&entity][0], 123);
    }

    // -----------------------------------------------------------------------
    // DeltaFrame – byte size calculation
    // -----------------------------------------------------------------------

    #[test]
    fn delta_frame_byte_size_empty() {
        let frame = DeltaFrame::new(0);
        assert_eq!(frame.byte_size(), 0);
    }

    #[test]
    fn delta_frame_byte_size() {
        let mut frame = DeltaFrame::new(10);

        let mut delta1 = EntityDelta::new(NetworkEntityId(1));
        delta1.set_component(0, b"abc"); // 4 + 3 = 7 bytes
        frame.deltas.push(delta1);

        let mut delta2 = EntityDelta::new(NetworkEntityId(2));
        delta2.set_component(5, b"defghi"); // 4 + 6 = 10 bytes
        frame.deltas.push(delta2);

        // Each entity: 8 (entity_id) + 8 (changed_mask) + 4 (vec len) + component_data.len()
        // entity1: 8 + 8 + 4 + 7 = 27
        // entity2: 8 + 8 + 4 + 10 = 30
        let expected_total = (8 + 8 + 4 + 7) + (8 + 8 + 4 + 10);
        assert_eq!(frame.byte_size(), expected_total);
    }

    #[test]
    fn delta_frame_tick_preserved() {
        let frame = DeltaFrame::new(12345);
        assert_eq!(frame.tick, 12345);
    }

    // -----------------------------------------------------------------------
    // All 64 slots can be set
    // -----------------------------------------------------------------------

    #[test]
    fn all_64_slots_can_be_set() {
        let entity = NetworkEntityId(999);
        let mut delta = EntityDelta::new(entity);

        for slot in 0..MAX_COMPONENT_SLOTS {
            delta.set_component(slot, &[slot as u8]);
        }

        // All 64 bits should be set.
        assert_eq!(delta.changed_mask, u64::MAX);
        assert_eq!(delta.component_count(), 64);

        // Verify all slots via iterator.
        let collected: Vec<_> = delta.iter_components().collect();
        assert_eq!(collected.len(), 64);

        for (i, (slot, data)) in collected.iter().enumerate() {
            assert_eq!(*slot, i);
            assert_eq!(*data, &[i as u8]);
        }
    }

    // -----------------------------------------------------------------------
    // Serde roundtrip for EntityDelta and DeltaFrame
    // -----------------------------------------------------------------------

    #[test]
    fn entity_delta_serde_roundtrip() {
        let entity = NetworkEntityId(77);
        let mut delta = EntityDelta::new(entity);
        delta.set_component(2, b"test_data");
        delta.set_component(7, b"more_data_here");

        let config = bincode::config::standard();
        let serialized = bincode::serde::encode_to_vec(&delta, config).expect("serialize failed");
        let (deserialized, _): (EntityDelta, usize) =
            bincode::serde::decode_from_slice(&serialized, config).expect("deserialize failed");

        assert_eq!(deserialized.entity_id, entity);
        assert_eq!(deserialized.changed_mask, delta.changed_mask);
        assert_eq!(deserialized.component_data, delta.component_data);
        assert_eq!(deserialized.component_count(), delta.component_count());

        let collected: Vec<_> = deserialized.iter_components().collect();
        assert_eq!(collected.len(), 2);
        assert_eq!(collected[0], (2, b"test_data".as_slice()));
        assert_eq!(collected[1], (7, b"more_data_here".as_slice()));
    }

    #[test]
    fn delta_frame_serde_roundtrip() {
        let mut frame = DeltaFrame::new(500);

        let mut d1 = EntityDelta::new(NetworkEntityId(10));
        d1.set_component(0, b"pos");
        frame.deltas.push(d1);

        let mut d2 = EntityDelta::new(NetworkEntityId(20));
        d2.set_component(1, b"vel");
        d2.set_component(3, b"rot");
        frame.deltas.push(d2);

        let config = bincode::config::standard();
        let serialized = bincode::serde::encode_to_vec(&frame, config).expect("serialize failed");
        let (deserialized, _): (DeltaFrame, usize) =
            bincode::serde::decode_from_slice(&serialized, config).expect("deserialize failed");

        assert_eq!(deserialized.tick, 500);
        assert_eq!(deserialized.deltas.len(), 2);
        assert_eq!(deserialized.byte_size(), frame.byte_size());
    }

    // -----------------------------------------------------------------------
    // Multiple entities in single DeltaFrame
    // -----------------------------------------------------------------------

    #[test]
    fn delta_frame_multiple_entities() {
        let mut frame = DeltaFrame::new(1);

        for i in 0..10 {
            let mut delta = EntityDelta::new(NetworkEntityId(i));
            delta.set_component(0, &[i as u8; 4]);
            frame.deltas.push(delta);
        }

        assert_eq!(frame.deltas.len(), 10);
        assert!(frame.byte_size() > 0);

        for (i, delta) in frame.deltas.iter().enumerate() {
            assert_eq!(delta.entity_id, NetworkEntityId(i as u64));
            assert_eq!(delta.component_count(), 1);
        }
    }

    // -----------------------------------------------------------------------
    // Edge cases – empty component data, single-byte data, large data
    // -----------------------------------------------------------------------

    #[test]
    fn set_component_empty_data() {
        let entity = NetworkEntityId(1);
        let mut delta = EntityDelta::new(entity);

        delta.set_component(0, b"");

        assert_eq!(delta.changed_mask, 1);
        assert_eq!(delta.component_count(), 1);

        let collected: Vec<_> = delta.iter_components().collect();
        assert_eq!(collected.len(), 1);
        assert_eq!(collected[0].0, 0);
        assert!(collected[0].1.is_empty());
    }

    #[test]
    fn set_component_single_byte() {
        let entity = NetworkEntityId(2);
        let mut delta = EntityDelta::new(entity);

        delta.set_component(31, &[0x42]);

        let collected: Vec<_> = delta.iter_components().collect();
        assert_eq!(collected.len(), 1);
        assert_eq!(collected[0], (31, vec![0x42].as_slice()));
    }

    #[test]
    fn set_component_large_data() {
        let entity = NetworkEntityId(3);
        let mut delta = EntityDelta::new(entity);

        let large_data: Vec<u8> = (0..10_000).map(|i| (i % 256) as u8).collect();
        delta.set_component(10, &large_data);

        let collected: Vec<_> = delta.iter_components().collect();
        assert_eq!(collected.len(), 1);
        assert_eq!(collected[0].0, 10);
        assert_eq!(collected[0].1, large_data.as_slice());
    }

    // -----------------------------------------------------------------------
    // Sparse slot patterns
    // -----------------------------------------------------------------------

    #[test]
    fn sparse_slot_pattern_every_other() {
        let entity = NetworkEntityId(50);
        let mut delta = EntityDelta::new(entity);

        for slot in (0..MAX_COMPONENT_SLOTS).step_by(2) {
            delta.set_component(slot, &[slot as u8]);
        }

        assert_eq!(delta.component_count(), 32);
        // Even bits set: 0b101010... = 0x5555555555555555
        assert_eq!(delta.changed_mask, 0x5555_5555_5555_5555);

        let collected: Vec<_> = delta.iter_components().collect();
        assert_eq!(collected.len(), 32);

        for (i, (slot, data)) in collected.iter().enumerate() {
            let expected_slot = i * 2;
            assert_eq!(*slot, expected_slot);
            assert_eq!(*data, &[expected_slot as u8]);
        }
    }

    // -----------------------------------------------------------------------
    // ClientBaseline – multiple entities with selective changes
    // -----------------------------------------------------------------------

    #[test]
    fn client_baseline_multiple_entities_selective_changes() {
        let client_id = ClientId(700);
        let mut baseline = ClientBaseline::new(client_id);

        let entity_a = NetworkEntityId(100);
        let entity_b = NetworkEntityId(200);

        let mut baseline_hashes = HashMap::new();
        let mut ha = [0u64; MAX_COMPONENT_SLOTS];
        ha[0] = 1;
        ha[1] = 2;
        baseline_hashes.insert(entity_a, ha);

        let mut hb = [0u64; MAX_COMPONENT_SLOTS];
        hb[0] = 10;
        hb[1] = 20;
        baseline_hashes.insert(entity_b, hb);

        baseline.acknowledge(20, &baseline_hashes);

        // Entity A: only slot 0 changed.
        let mut current_a = [0u64; MAX_COMPONENT_SLOTS];
        current_a[0] = 999;
        current_a[1] = 2; // unchanged

        // Entity B: only slot 1 changed.
        let mut current_b = [0u64; MAX_COMPONENT_SLOTS];
        current_b[0] = 10; // unchanged
        current_b[1] = 888;

        let serialize_a = |slot: usize| -> Option<Vec<u8>> {
            if slot == 0 {
                Some(vec![0xA1])
            } else {
                None
            }
        };

        let serialize_b = |slot: usize| -> Option<Vec<u8>> {
            if slot == 1 {
                Some(vec![0xB1])
            } else {
                None
            }
        };

        let delta_a = baseline.compute_delta(entity_a, &current_a, serialize_a);
        let delta_b = baseline.compute_delta(entity_b, &current_b, serialize_b);

        assert_eq!(delta_a.component_count(), 1);
        assert_eq!(delta_a.changed_mask, 1u64 << 0);

        assert_eq!(delta_b.component_count(), 1);
        assert_eq!(delta_b.changed_mask, 1u64 << 1);
    }

    // -----------------------------------------------------------------------
    // ClientBaseline – serialize_slot returning None skips slot
    // -----------------------------------------------------------------------

    #[test]
    fn client_baseline_serialize_none_skips_slot() {
        let client_id = ClientId(800);
        let mut baseline = ClientBaseline::new(client_id);

        let entity = NetworkEntityId(5);
        let baseline_hashes = HashMap::new();
        baseline.acknowledge(1, &baseline_hashes);

        let mut current = [0u64; MAX_COMPONENT_SLOTS];
        current[0] = 1; // new slot
        current[1] = 2; // new slot

        // serialize_slot returns None for slot 1.
        let serialize = |slot: usize| -> Option<Vec<u8>> {
            if slot == 0 {
                Some(vec![0xAA])
            } else {
                None
            }
        };

        let delta = baseline.compute_delta(entity, &current, serialize);

        // Only slot 0 should appear (slot 1 was skipped by serialize closure).
        assert_eq!(delta.component_count(), 1);
        assert_eq!(delta.changed_mask, 1u64 << 0);

        let collected: Vec<_> = delta.iter_components().collect();
        assert_eq!(collected.len(), 1);
        assert_eq!(collected[0], (0, vec![0xAA].as_slice()));
    }

    // -----------------------------------------------------------------------
    // Property-based tests using proptest
    // -----------------------------------------------------------------------

    use proptest::prelude::*;

    /// Property: For any set of (slot, data) pairs within valid range,
    /// the iterator roundtrip recovers every pair in order.
    proptest! {
        #[test]
        fn proptest_delta_roundtrip_any_components(
            slots_and_data in prop::collection::vec(
                (0usize..64, prop::collection::vec(any::<u8>(), 0..256)),
                1..16,
            )
        ) {
            // Deduplicate by keeping last value per slot (mimics real behavior).
            let mut slot_map = std::collections::BTreeMap::new();
            for (slot, data) in slots_and_data {
                slot_map.insert(slot, data);
            }

            let entity = NetworkEntityId(42);
            let mut delta = EntityDelta::new(entity);

            for (slot, data) in &slot_map {
                delta.set_component(*slot, data);
            }

            let expected_count = slot_map.len();
            assert_eq!(delta.component_count() as usize, expected_count);

            let collected: Vec<_> = delta.iter_components().collect();
            assert_eq!(collected.len(), expected_count);

            let expected: Vec<_> = slot_map.iter().map(|(s, d)| (*s, d.as_slice())).collect();
            for (actual, expected_item) in collected.iter().zip(expected.iter()) {
                assert_eq!(actual.0, expected_item.0);
                assert_eq!(actual.1, expected_item.1);
            }
        }
    }

    /// Property: Byte size of a DeltaFrame is consistent with its contents.
    proptest! {
        #[test]
        fn proptest_delta_frame_byte_size_consistent(
            num_entities in 0usize..32,
            component_counts in prop::collection::vec(1usize..8, 0..32),
            data_sizes in prop::collection::vec(0usize..512, 0..32),
        ) {
            let count = num_entities.min(component_counts.len()).min(data_sizes.len());
            let mut frame = DeltaFrame::new(100);

            for i in 0..count {
                let mut delta = EntityDelta::new(NetworkEntityId(i as u64));
                let num_comps = component_counts[i].min(64);
                for slot in 0..num_comps {
                    let data = vec![0u8; data_sizes[i]];
                    delta.set_component(slot, &data);
                }
                frame.deltas.push(delta);
            }

            // Compute expected size manually.
            let expected: usize = frame.deltas.iter().map(|d| {
                8 + 8 + 4 + d.component_data.len()
            }).sum();

            prop_assert_eq!(frame.byte_size(), expected);
        }
    }

    /// Property: Setting the same slot twice updates the mask (no change) and appends data again.
    proptest! {
        #[test]
        fn proptest_set_component_same_slot_appends(
            slot in 0usize..64,
            data1 in prop::collection::vec(any::<u8>(), 1..64),
            data2 in prop::collection::vec(any::<u8>(), 1..64),
        ) {
            let entity = NetworkEntityId(1);
            let mut delta = EntityDelta::new(entity);

            delta.set_component(slot, &data1);
            delta.set_component(slot, &data2);

            // Mask should have only one bit set.
            prop_assert_eq!(delta.changed_mask, 1u64 << slot);
            // Component count should still be 1 (one bit set).
            prop_assert_eq!(delta.component_count(), 1);

            // Iterator should yield one component (the first one, since mask has one bit).
            let collected: Vec<_> = delta.iter_components().collect();
            // The iterator reads one length-prefixed blob per set bit.
            // Since we appended twice but only one bit is set, iterator reads only the first blob.
            prop_assert_eq!(collected.len(), 1);
            prop_assert_eq!(collected[0].0, slot);
            prop_assert_eq!(collected[0].1, data1.as_slice());
        }
    }

    /// Property: Sparse random slot patterns iterate correctly in ascending order.
    proptest! {
        #[test]
        fn proptest_sparse_slots_iter_in_order(
            slot_indices in prop::collection::btree_set(0usize..64, 1..32),
            data_vec in prop::collection::vec(prop::collection::vec(any::<u8>(), 0..128), 1..32),
        ) {
            // Use exactly as many slots and data entries as the shorter of the two.
            let count = slot_indices.len().min(data_vec.len());
            let slots: Vec<_> = slot_indices.iter().copied().take(count).collect();
            let data: Vec<_> = data_vec.iter().take(count).collect();

            let entity = NetworkEntityId(777);
            let mut delta = EntityDelta::new(entity);

            for (i, &slot) in slots.iter().enumerate() {
                delta.set_component(slot, data[i]);
            }

            let collected: Vec<_> = delta.iter_components().collect();
            prop_assert_eq!(collected.len(), slots.len());

            for (i, (slot, d)) in collected.iter().enumerate() {
                prop_assert_eq!(*slot, slots[i]);
                prop_assert_eq!(*d, data[i].as_slice());
            }
        }
    }
}
