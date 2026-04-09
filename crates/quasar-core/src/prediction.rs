//! Client-side prediction (GGPO-style).
//!
//! The client applies inputs immediately for responsive gameplay.  The server
//! processes the same inputs and periodically confirms state.  On mismatch the
//! client rolls back to the last confirmed snapshot and re-applies unconfirmed
//! inputs to reconcile.
//!
//! Entity interpolation smooths movement for non-player entities.
//!
//! Requires `PhysicsSnapshot` from `quasar-physics` for state save/restore.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::network::{ClientId, EntitySnapshot, InputData};

/// Maximum number of unconfirmed frames we keep before forcing a resync.
const MAX_PREDICTION_FRAMES: usize = 64;

/// Number of entity snapshots to keep for interpolation.
const INTERPOLATION_HISTORY_SIZE: usize = 32;

/// An input frame as sent to the server and retained locally for rollback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputFrame {
    /// Simulation tick this input was generated for.
    pub tick: u64,
    /// Sequence number for ordering detection.
    pub sequence: u64,
    /// The inputs for this frame.
    pub inputs: Vec<InputData>,
}

/// Opaque physics snapshot handle supplied by the game.
///
/// The prediction system stores/restores these but does not inspect them.
/// They must implement `Clone` so we can keep a ring buffer.
pub trait PredictionSnapshot: Clone + Send + 'static {}

impl<T: Clone + Send + 'static> PredictionSnapshot for T {}

/// State confirmed by the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfirmation {
    /// The tick the server has confirmed up to.
    pub confirmed_tick: u64,
    /// Authoritative position per entity: `(entity_net_id, pos)`.
    pub positions: Vec<(u64, [f32; 3])>,
}

/// Snapshot of non-player entity for interpolation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityInterpolationState {
    pub entity_id: u64,
    pub tick: u64,
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

/// Entity interpolator for smoothing non-player movement.
pub struct EntityInterpolator {
    /// Ring buffer of entity snapshots from server.
    history: VecDeque<EntityInterpolationState>,
    /// Last server tick received.
    last_server_tick: u64,
    /// Interpolation delay in ticks.
    delay_ticks: u32,
}

impl EntityInterpolator {
    pub fn new(delay_ticks: u32) -> Self {
        Self {
            history: VecDeque::with_capacity(INTERPOLATION_HISTORY_SIZE),
            last_server_tick: 0,
            delay_ticks,
        }
    }

    /// Add entity snapshot from server state.
    pub fn add_snapshot(&mut self, snapshot: EntitySnapshot) {
        let state = EntityInterpolationState {
            entity_id: snapshot.entity_id.0,
            tick: snapshot.frame,
            position: snapshot.position,
            rotation: snapshot.rotation,
            scale: snapshot.scale,
        };

        self.history.push_back(state);
        if self.history.len() > INTERPOLATION_HISTORY_SIZE {
            self.history.pop_front();
        }

        self.last_server_tick = snapshot.frame;
    }

    /// Get interpolated position for an entity at current tick.
    ///
    /// Returns `Some(position, rotation, scale)` if interpolation data is available,
    /// `None` otherwise (e.g., no snapshots yet or entity not found).
    pub fn interpolate_entity(
        &self,
        entity_id: u64,
        current_tick: u64,
    ) -> Option<([f32; 3], [f32; 4], [f32; 3])> {
        // Find two snapshots around our target tick (accounting for delay).
        let target_tick = current_tick.saturating_sub(self.delay_ticks as u64);

        // Find snapshots around target tick.
        let snapshots: Vec<&EntityInterpolationState> = self
            .history
            .iter()
            .filter(|s| s.entity_id == entity_id)
            .collect();

        if snapshots.len() < 2 {
            return None;
        }

        // Find the two snapshots that bracket our target tick.
        let before = snapshots.iter().rev().find(|s| s.tick <= target_tick);
        let after = snapshots.iter().find(|s| s.tick >= target_tick);

        match (before, after) {
            (None, None) => None,
            (None, Some(_after)) => {
                // Only future snapshots - use the earliest.
                let earliest = snapshots
                    .iter()
                    .min_by_key(|s| s.tick)
                    .unwrap_or(&snapshots[0]);
                Some((earliest.position, earliest.rotation, earliest.scale))
            }
            (Some(_before), None) => {
                // Only past snapshots - use the latest.
                let latest = snapshots
                    .iter()
                    .max_by_key(|s| s.tick)
                    .unwrap_or(&snapshots[0]);
                Some((latest.position, latest.rotation, latest.scale))
            }
            (Some(before), Some(after)) => {
                if before.tick == after.tick {
                    // Both are the same snapshot.
                    Some((before.position, before.rotation, before.scale))
                } else {
                    // Lerp between before and after.
                    let delta =
                        (target_tick - before.tick) as f32 / (after.tick - before.tick) as f32;
                    let delta = delta.clamp(0.0, 1.0);

                    let pos = lerp_vec3(&before.position, &after.position, delta);
                    let rot = slerp_quat(&before.rotation, &after.rotation, delta);
                    let scale = lerp_vec3(&before.scale, &after.scale, delta);
                    Some((pos, rot, scale))
                }
            }
        }
    }

    /// Clean up snapshots older than a given tick to free memory.
    pub fn prune(&mut self, oldest_tick: u64) {
        while let Some(front) = self.history.front() {
            if front.tick
                < oldest_tick
                    .saturating_sub(1)
                    .saturating_sub(INTERPOLATION_HISTORY_SIZE as u64)
            {
                self.history.pop_front();
            } else {
                break;
            }
        }
    }

    /// Check if we have sufficient data for interpolation at a given tick.
    pub fn can_interpolate(&self, _entity_id: u64, current_tick: u64) -> bool {
        let target_tick = current_tick.saturating_sub(self.delay_ticks as u64);

        if self.history.is_empty() {
            return false;
        }

        let min_tick = self.history.front().map(|s| s.tick).unwrap_or(0);
        let max_tick = self.history.back().map(|s| s.tick).unwrap_or(0);

        min_tick <= target_tick && target_tick <= max_tick
    }

    /// Get the number of snapshots stored.
    pub fn snapshot_count(&self) -> usize {
        self.history.len()
    }

    /// Get the most recent server tick received.
    pub fn last_server_tick(&self) -> u64 {
        self.last_server_tick
    }
}

fn lerp_vec3(a: &[f32; 3], b: &[f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + t * (b[0] - a[0]),
        a[1] + t * (b[1] - a[1]),
        a[2] + t * (b[2] - a[2]),
    ]
}

fn slerp_quat(a: &[f32; 4], b: &[f32; 4], t: f32) -> [f32; 4] {
    // Simplified slerp - in production use full SLERP
    let mut result = [
        a[0] + t * (b[0] - a[0]),
        a[1] + t * (b[1] - a[1]),
        a[2] + t * (b[2] - a[2]),
        a[3] + t * (b[3] - a[3]),
    ];

    // Normalize
    let len = (result[0] * result[0]
        + result[1] * result[1]
        + result[2] * result[2]
        + result[3] * result[3])
        .sqrt();

    if len > 0.0 {
        result[0] /= len;
        result[1] /= len;
        result[2] /= len;
        result[3] /= len;
    }

    result
}

/// Client-side prediction manager.
///
/// Generic over `S` â€” the physics snapshot type â€” so it works with any engine
/// that can serialize its physics state.
pub struct PredictionManager<S: PredictionSnapshot> {
    /// Ring of `(tick, snapshot, inputs)` â€” unconfirmed frames.
    history: VecDeque<(u64, S, Vec<InputData>)>,
    /// Input sequence counter.
    next_sequence: u64,
    /// The last server-confirmed tick.
    pub confirmed_tick: u64,
    /// Local predicted tick (always >= confirmed_tick).
    pub predicted_tick: u64,
    /// Client ID for outgoing messages.
    pub client_id: ClientId,
    /// Tolerance for position mismatch before forcing rollback (squared).
    pub mismatch_threshold_sq: f32,
    /// Entity interpolator for smoothing non-player movement.
    pub interpolator: EntityInterpolator,
}

impl<S: PredictionSnapshot> PredictionManager<S> {
    pub fn new(client_id: ClientId) -> Self {
        Self {
            history: VecDeque::with_capacity(MAX_PREDICTION_FRAMES),
            next_sequence: 0,
            confirmed_tick: 0,
            predicted_tick: 0,
            client_id,
            mismatch_threshold_sq: 0.001 * 0.001,
            interpolator: EntityInterpolator::new(3), // 3 ticks delay
        }
    }

    /// Set the interpolation delay in ticks.
    pub fn set_interpolation_delay(&mut self, delay_ticks: u32) {
        self.interpolator = EntityInterpolator::new(delay_ticks);
    }

    /// Set the reconciliation threshold (in world units).
    pub fn set_reconciliation_threshold(&mut self, threshold: f32) {
        self.mismatch_threshold_sq = threshold * threshold;
    }

    /// Record a predicted frame with sequence numbering.
    ///
    /// Call **after** applying `inputs` locally and capturing a `snapshot`
    /// of the physics state at `tick`.
    pub fn record_prediction(&mut self, tick: u64, snapshot: S, inputs: Vec<InputData>) -> u64 {
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.wrapping_add(1);

        if self.history.len() >= MAX_PREDICTION_FRAMES {
            self.history.pop_front();
        }
        self.history.push_back((tick, snapshot, inputs.clone()));
        self.predicted_tick = tick;

        sequence
    }

    /// Process a server confirmation.
    ///
    /// Returns `Some(snapshot, remaining_inputs)` if a rollback is needed:
    /// the caller should restore `snapshot`, then re-apply each `InputFrame`
    /// in sequence.  Returns `None` if the server state matches the prediction.
    pub fn on_server_confirm(
        &mut self,
        confirmation: &ServerConfirmation,
        local_positions: &[(u64, [f32; 3])],
    ) -> Option<(S, Vec<InputFrame>)> {
        let server_tick = confirmation.confirmed_tick;
        if server_tick <= self.confirmed_tick {
            return None;
        }

        // Discard frames older than server_tick.
        while self
            .history
            .front()
            .is_some_and(|(t, _, _)| *t < server_tick)
        {
            self.history.pop_front();
        }

        self.confirmed_tick = server_tick;

        // Check for mismatch.
        let mismatch = confirmation.positions.iter().any(|(eid, server_pos)| {
            local_positions
                .iter()
                .find(|(lid, _)| *lid == *eid)
                .is_none_or(|(_, local_pos)| {
                    let dx = server_pos[0] - local_pos[0];
                    let dy = server_pos[1] - local_pos[1];
                    let dz = server_pos[2] - local_pos[2];
                    dx * dx + dy * dy + dz * dz > self.mismatch_threshold_sq
                })
        });

        if !mismatch {
            return None;
        }

        // Need rollback: find the snapshot at server_tick.
        let entry = self.history.iter().find(|(t, _, _)| *t == server_tick);

        let snapshot = match entry {
            Some((_, snap, _)) => snap.clone(),
            None => return None, // Snapshot lost â€” can't rollback
        };

        // Gather inputs for all frames after server_tick that need replaying.
        // Include sequence numbers in the returned frame data.
        let replay_inputs: Vec<InputFrame> = self
            .history
            .iter()
            .filter(|(t, _, _)| *t > server_tick)
            .map(|(tick, _, inputs)| InputFrame {
                tick: *tick,
                sequence: self.next_sequence.wrapping_sub(1),
                inputs: inputs.clone(),
            })
            .collect();

        Some((snapshot, replay_inputs))
    }

    /// Number of unconfirmed frames currently buffered.
    pub fn unconfirmed_frames(&self) -> usize {
        self.history.len()
    }

    /// Number of frames between client and server (measured by sequence numbers).
    pub fn client_server_delta_frames(&self) -> i32 {
        if self.history.is_empty() {
            return 0;
        }

        let latest_tick = self.history.back().map(|s| s.0).unwrap_or(0);
        i32::try_from(latest_tick.saturating_sub(self.confirmed_tick)).unwrap_or(0)
    }

    /// Get input sequence to use for the next predicted frame.
    pub fn next_sequence(&self) -> u64 {
        self.next_sequence
    }

    /// Clear all history (e.g., on reconnect).
    pub fn reset(&mut self) {
        self.history.clear();
        self.confirmed_tick = 0;
        self.predicted_tick = 0;
        self.next_sequence = 0;
        // Reset interpolator as well - no data to interpolate.
        self.interpolator = EntityInterpolator::new(3);
    }

    /// Update entity interpolator with snapshots from a state packet.
    pub fn update_interpolation(&mut self, entities: &[EntitySnapshot]) {
        for entity in entities {
            self.interpolator.add_snapshot(entity.clone());
        }
    }

    /// Get interpolated position for a non-player entity.
    pub fn interpolate_entity(
        &self,
        entity_id: u64,
        current_tick: u64,
    ) -> Option<([f32; 3], [f32; 4], [f32; 3])> {
        self.interpolator
            .interpolate_entity(entity_id, current_tick)
    }

    /// Clean up old interpolation snapshots.
    pub fn prune_interpolation(&mut self, oldest_tick: u64) {
        self.interpolator.prune(oldest_tick);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::{ClientId, EntitySnapshot, InputData, InputType, NetworkEntityId};

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn make_input_data(input_type: InputType, value: f32) -> InputData {
        InputData { input_type, value }
    }

    fn make_entity_snapshot(entity_id: u64, frame: u64, position: [f32; 3]) -> EntitySnapshot {
        EntitySnapshot {
            entity_id: NetworkEntityId(entity_id),
            frame,
            position,
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        }
    }

    fn make_server_confirmation(
        confirmed_tick: u64,
        positions: Vec<(u64, [f32; 3])>,
    ) -> ServerConfirmation {
        ServerConfirmation {
            confirmed_tick,
            positions,
        }
    }

    // -----------------------------------------------------------------------
    // 1. PredictionManager initial state
    // -----------------------------------------------------------------------

    #[test]
    fn prediction_manager_initial_state() {
        let manager: PredictionManager<()> = PredictionManager::new(ClientId(1));

        assert_eq!(manager.next_sequence(), 0);
        assert_eq!(manager.confirmed_tick, 0);
        assert_eq!(manager.predicted_tick, 0);
        assert_eq!(manager.client_id, ClientId(1));
        assert_eq!(manager.unconfirmed_frames(), 0);
        assert_eq!(manager.client_server_delta_frames(), 0);
        assert_eq!(manager.interpolator.snapshot_count(), 0);
        assert_eq!(manager.interpolator.last_server_tick(), 0);
    }

    // -----------------------------------------------------------------------
    // 2. Record prediction increments tick and sequence
    // -----------------------------------------------------------------------

    #[test]
    fn record_prediction_increments_tick_and_sequence() {
        let mut manager: PredictionManager<()> = PredictionManager::new(ClientId(42));

        let inputs = vec![make_input_data(InputType::MoveForward, 1.0)];
        let seq0 = manager.record_prediction(1, (), inputs.clone());
        assert_eq!(seq0, 0);
        assert_eq!(manager.next_sequence(), 1);
        assert_eq!(manager.predicted_tick, 1);
        assert_eq!(manager.unconfirmed_frames(), 1);

        let inputs2 = vec![make_input_data(InputType::Jump, 1.0)];
        let seq1 = manager.record_prediction(2, (), inputs2);
        assert_eq!(seq1, 1);
        assert_eq!(manager.next_sequence(), 2);
        assert_eq!(manager.predicted_tick, 2);
        assert_eq!(manager.unconfirmed_frames(), 2);
    }

    // -----------------------------------------------------------------------
    // 3. Ring buffer overflow evicts oldest
    // -----------------------------------------------------------------------

    #[test]
    fn ring_buffer_overflow_evicts_oldest() {
        let mut manager: PredictionManager<()> = PredictionManager::new(ClientId(1));

        // Fill up to capacity
        for tick in 0..MAX_PREDICTION_FRAMES as u64 {
            manager.record_prediction(tick, (), vec![]);
        }
        assert_eq!(manager.unconfirmed_frames(), MAX_PREDICTION_FRAMES);

        // One more should evict the oldest (tick 0)
        manager.record_prediction(MAX_PREDICTION_FRAMES as u64, (), vec![]);
        assert_eq!(manager.unconfirmed_frames(), MAX_PREDICTION_FRAMES);

        // The predicted tick should be the newest
        assert_eq!(manager.predicted_tick, MAX_PREDICTION_FRAMES as u64);
        assert_eq!(manager.next_sequence(), MAX_PREDICTION_FRAMES as u64 + 1);
    }

    // -----------------------------------------------------------------------
    // 4. Server confirmation without mismatch returns None
    // -----------------------------------------------------------------------

    #[test]
    fn server_confirmation_no_mismatch_returns_none() {
        let mut manager: PredictionManager<()> = PredictionManager::new(ClientId(1));

        // Record some predictions
        manager.record_prediction(1, (), vec![make_input_data(InputType::MoveForward, 1.0)]);
        manager.record_prediction(2, (), vec![make_input_data(InputType::MoveRight, 0.5)]);

        // Server confirms with matching positions (within threshold)
        let confirmation = make_server_confirmation(2, vec![(1, [0.0, 0.0, 0.0])]);
        let local_positions = vec![(1, [0.0, 0.0, 0.0])];

        let result = manager.on_server_confirm(&confirmation, &local_positions);
        assert!(result.is_none());
        assert_eq!(manager.confirmed_tick, 2);
    }

    // -----------------------------------------------------------------------
    // 5. Server confirmation with mismatch triggers rollback
    // -----------------------------------------------------------------------

    #[test]
    fn server_confirmation_with_mismatch_triggers_rollback() {
        let mut manager: PredictionManager<()> = PredictionManager::new(ClientId(1));

        // Record predictions for ticks 1 and 2
        manager.record_prediction(1, (), vec![make_input_data(InputType::MoveForward, 1.0)]);
        manager.record_prediction(2, (), vec![make_input_data(InputType::MoveForward, 1.0)]);

        // Server confirms tick 2 but reports a significantly different position
        let confirmation = make_server_confirmation(2, vec![(1, [10.0, 0.0, 0.0])]);
        let local_positions = vec![(1, [0.0, 0.0, 0.0])]; // Local thinks entity is at origin

        let result = manager.on_server_confirm(&confirmation, &local_positions);
        assert!(result.is_some());

        let (_snapshot, replay_inputs) = result.unwrap();
        // Replay inputs should contain frames after the confirmed tick
        assert_eq!(manager.confirmed_tick, 2);
        // Inputs for ticks > 2 should be included (if any exist in history)
        for input_frame in &replay_inputs {
            assert!(input_frame.tick > 2);
        }
    }

    // -----------------------------------------------------------------------
    // 6. Sequence number wrapping
    // -----------------------------------------------------------------------

    #[test]
    fn sequence_number_wrapping() {
        let mut manager: PredictionManager<()> = PredictionManager::new(ClientId(1));

        // Set sequence near u64::MAX to test wrapping
        manager.next_sequence = u64::MAX;

        let seq = manager.record_prediction(1, (), vec![]);
        assert_eq!(seq, u64::MAX);
        assert_eq!(manager.next_sequence(), 0); // Wrapped around

        let seq2 = manager.record_prediction(2, (), vec![]);
        assert_eq!(seq2, 0);
        assert_eq!(manager.next_sequence(), 1);
    }

    // -----------------------------------------------------------------------
    // 7. Interpolator returns None without data
    // -----------------------------------------------------------------------

    #[test]
    fn interpolator_returns_none_without_data() {
        let interpolator = EntityInterpolator::new(3);
        let result = interpolator.interpolate_entity(1, 10);
        assert!(result.is_none());

        assert!(!interpolator.can_interpolate(1, 10));
        assert_eq!(interpolator.snapshot_count(), 0);
    }

    // -----------------------------------------------------------------------
    // 8. Interpolator linear interpolation
    // -----------------------------------------------------------------------

    #[test]
    fn interpolator_linear_interpolation() {
        let mut interpolator = EntityInterpolator::new(0); // No delay for simplicity

        // Add two snapshots for entity 1 at ticks 10 and 20
        let snap1 = make_entity_snapshot(1, 10, [0.0, 0.0, 0.0]);
        let snap2 = make_entity_snapshot(1, 20, [10.0, 0.0, 0.0]);

        interpolator.add_snapshot(snap1);
        interpolator.add_snapshot(snap2);

        // Interpolate at tick 15 (target_tick = 15 - 0 = 15, midpoint)
        let result = interpolator.interpolate_entity(1, 15);
        assert!(result.is_some());

        let (pos, _rot, _scale) = result.unwrap();
        // Should be halfway between [0,0,0] and [10,0,0]
        assert!((pos[0] - 5.0).abs() < 0.001);
        assert!((pos[1] - 0.0).abs() < 0.001);
        assert!((pos[2] - 0.0).abs() < 0.001);

        // Interpolate at tick 12 (target = 12, closer to first snapshot)
        let result = interpolator.interpolate_entity(1, 12);
        assert!(result.is_some());
        let (pos, _, _) = result.unwrap();
        // Should be 20% of the way from [0,0,0] to [10,0,0]
        let expected = 0.0 + (12.0 - 10.0) as f32 / (20.0 - 10.0) as f32 * 10.0;
        assert!((pos[0] - expected).abs() < 0.001);
    }

    // -----------------------------------------------------------------------
    // 9. Interpolator pruning removes old snapshots
    // -----------------------------------------------------------------------

    #[test]
    fn interpolator_pruning_removes_old_snapshots() {
        let mut interpolator = EntityInterpolator::new(0);

        // Add snapshots at various ticks
        for tick in 0..10 {
            let snap = make_entity_snapshot(1, tick, [tick as f32, 0.0, 0.0]);
            interpolator.add_snapshot(snap);
        }

        assert_eq!(interpolator.snapshot_count(), 10);

        // Prune snapshots older than tick 5
        interpolator.prune(5);

        // Should have removed some old snapshots
        // The prune threshold is: oldest_tick - 1 - INTERPOLATION_HISTORY_SIZE
        // = 5 - 1 - 32 = -28, so nothing should be pruned at this threshold
        // Let's test with a much larger oldest_tick
        interpolator.prune(100);
        // Now threshold = 100 - 1 - 32 = 67, all snapshots < 67 should be pruned
        assert_eq!(interpolator.snapshot_count(), 0);
    }

    #[test]
    fn interpolator_pruning_respects_history_size() {
        let mut interpolator = EntityInterpolator::new(0);

        // Add 40 snapshots (more than INTERPOLATION_HISTORY_SIZE of 32)
        for tick in 0..40 {
            let snap = make_entity_snapshot(1, tick, [tick as f32, 0.0, 0.0]);
            interpolator.add_snapshot(snap);
        }

        // add_snapshot caps at INTERPOLATION_HISTORY_SIZE
        assert_eq!(interpolator.snapshot_count(), INTERPOLATION_HISTORY_SIZE);

        // The oldest should have been evicted by the ring buffer
        let oldest_in_history = interpolator.history.front().map(|s| s.tick);
        assert_eq!(oldest_in_history, Some(8)); // 40 - 32 = 8
    }

    // -----------------------------------------------------------------------
    // 10. Prediction reset clears all state
    // -----------------------------------------------------------------------

    #[test]
    fn prediction_reset_clears_all_state() {
        let mut manager: PredictionManager<()> = PredictionManager::new(ClientId(1));

        // Add some state
        manager.record_prediction(1, (), vec![make_input_data(InputType::MoveForward, 1.0)]);
        manager.record_prediction(2, (), vec![make_input_data(InputType::Jump, 1.0)]);
        manager.confirmed_tick = 1;

        // Add interpolation data
        manager.update_interpolation(&[make_entity_snapshot(1, 1, [0.0, 0.0, 0.0])]);
        manager.update_interpolation(&[make_entity_snapshot(1, 2, [1.0, 0.0, 0.0])]);

        assert_eq!(manager.unconfirmed_frames(), 2);
        assert_eq!(manager.confirmed_tick, 1);
        assert_eq!(manager.predicted_tick, 2);
        assert_eq!(manager.next_sequence(), 2);
        assert_eq!(manager.interpolator.snapshot_count(), 2);

        // Reset
        manager.reset();

        assert_eq!(manager.unconfirmed_frames(), 0);
        assert_eq!(manager.confirmed_tick, 0);
        assert_eq!(manager.predicted_tick, 0);
        assert_eq!(manager.next_sequence(), 0);
        assert_eq!(manager.interpolator.snapshot_count(), 0);
        assert_eq!(manager.client_server_delta_frames(), 0);
    }

    // -----------------------------------------------------------------------
    // 11. Mismatch threshold respects configured value
    // -----------------------------------------------------------------------

    #[test]
    fn mismatch_threshold_respects_configured_value() {
        // Test with a generous threshold first (1.0 unit squared = 1.0)
        {
            let mut manager: PredictionManager<()> = PredictionManager::new(ClientId(1));
            manager.set_reconciliation_threshold(1.0);
            assert_eq!(manager.mismatch_threshold_sq, 1.0);

            manager.record_prediction(1, (), vec![]);
            manager.record_prediction(2, (), vec![]);
            manager.record_prediction(3, (), vec![]);

            // Position difference of 0.5 units (squared = 0.25, below threshold)
            let confirmation = make_server_confirmation(2, vec![(1, [0.5, 0.0, 0.0])]);
            let local_positions = vec![(1, [0.0, 0.0, 0.0])];

            let result = manager.on_server_confirm(&confirmation, &local_positions);
            assert!(result.is_none()); // Should NOT trigger rollback
            assert_eq!(manager.confirmed_tick, 2);
        }

        // Test with a tight threshold (0.1 unit squared = 0.01)
        {
            let mut manager: PredictionManager<()> = PredictionManager::new(ClientId(1));
            manager.set_reconciliation_threshold(0.1);
            assert!((manager.mismatch_threshold_sq - 0.01).abs() < f32::EPSILON);

            manager.record_prediction(1, (), vec![]);
            manager.record_prediction(2, (), vec![]);
            manager.record_prediction(3, (), vec![]);

            // Same position difference (0.5 units, squared = 0.25 > 0.01)
            let confirmation = make_server_confirmation(2, vec![(1, [0.5, 0.0, 0.0])]);
            let local_positions = vec![(1, [0.0, 0.0, 0.0])];

            let result = manager.on_server_confirm(&confirmation, &local_positions);
            assert!(result.is_some()); // SHOULD trigger rollback now
            assert_eq!(manager.confirmed_tick, 2);
        }
    }

    // -----------------------------------------------------------------------
    // Additional edge-case tests
    // -----------------------------------------------------------------------

    #[test]
    fn server_confirmation_ignores_stale_ticks() {
        let mut manager: PredictionManager<()> = PredictionManager::new(ClientId(1));

        manager.record_prediction(5, (), vec![]);
        manager.confirmed_tick = 3;

        // Server confirms a tick we already confirmed - should be ignored
        let confirmation = make_server_confirmation(2, vec![]);
        let result = manager.on_server_confirm(&confirmation, &[]);
        assert!(result.is_none());
        assert_eq!(manager.confirmed_tick, 3); // Unchanged
    }

    #[test]
    fn server_confirmation_discards_old_history() {
        let mut manager: PredictionManager<()> = PredictionManager::new(ClientId(1));

        manager.record_prediction(1, (), vec![]);
        manager.record_prediction(2, (), vec![]);
        manager.record_prediction(3, (), vec![]);
        manager.record_prediction(4, (), vec![]);

        assert_eq!(manager.unconfirmed_frames(), 4);

        // Server confirms up to tick 2
        // Frames with tick < 2 are discarded (only tick 1).
        // Frame at tick 2 is NOT discarded (< is strict, not <=).
        let confirmation = make_server_confirmation(2, vec![]);
        manager.on_server_confirm(&confirmation, &[]);

        // Frame 1 discarded, frames 2, 3, 4 remain
        assert_eq!(manager.unconfirmed_frames(), 3);
        assert_eq!(manager.confirmed_tick, 2);
    }

    #[test]
    fn interpolator_handles_single_snapshot() {
        let mut interpolator = EntityInterpolator::new(0);
        let snap = make_entity_snapshot(42, 10, [5.0, 5.0, 5.0]);
        interpolator.add_snapshot(snap);

        // With only one snapshot, interpolation should return None
        let result = interpolator.interpolate_entity(42, 10);
        assert!(result.is_none());
    }

    #[test]
    fn interpolator_handles_entity_not_in_history() {
        let mut interpolator = EntityInterpolator::new(0);
        interpolator.add_snapshot(make_entity_snapshot(1, 10, [0.0, 0.0, 0.0]));
        interpolator.add_snapshot(make_entity_snapshot(1, 20, [10.0, 0.0, 0.0]));

        // Query for entity that doesn't exist
        let result = interpolator.interpolate_entity(999, 15);
        assert!(result.is_none());
    }

    #[test]
    fn interpolator_handles_future_only_snapshots() {
        let mut interpolator = EntityInterpolator::new(0);
        interpolator.add_snapshot(make_entity_snapshot(1, 100, [50.0, 0.0, 0.0]));
        interpolator.add_snapshot(make_entity_snapshot(1, 200, [100.0, 0.0, 0.0]));

        // Query at tick 50, which is before all snapshots
        let result = interpolator.interpolate_entity(1, 50);
        assert!(result.is_some());
        let (pos, _, _) = result.unwrap();
        // Should return the earliest snapshot
        assert!((pos[0] - 50.0).abs() < 0.001);
    }

    #[test]
    fn interpolator_handles_past_only_snapshots() {
        let mut interpolator = EntityInterpolator::new(0);
        interpolator.add_snapshot(make_entity_snapshot(1, 10, [5.0, 0.0, 0.0]));
        interpolator.add_snapshot(make_entity_snapshot(1, 20, [10.0, 0.0, 0.0]));

        // Query at tick 100, which is after all snapshots
        let result = interpolator.interpolate_entity(1, 100);
        assert!(result.is_some());
        let (pos, _, _) = result.unwrap();
        // Should return the latest snapshot
        assert!((pos[0] - 10.0).abs() < 0.001);
    }

    #[test]
    fn interpolator_same_tick_snapshots() {
        let mut interpolator = EntityInterpolator::new(0);
        interpolator.add_snapshot(make_entity_snapshot(1, 10, [5.0, 0.0, 0.0]));
        interpolator.add_snapshot(make_entity_snapshot(1, 10, [10.0, 0.0, 0.0]));

        // When before and after are the same tick, should return that snapshot
        let result = interpolator.interpolate_entity(1, 10);
        assert!(result.is_some());
        let (pos, _, _) = result.unwrap();
        // Returns the first matching snapshot
        assert!((pos[0] - 5.0).abs() < 0.001 || (pos[0] - 10.0).abs() < 0.001);
    }

    #[test]
    fn interpolator_delay_ticks_affects_target() {
        let mut interpolator = EntityInterpolator::new(5); // 5 tick delay
        interpolator.add_snapshot(make_entity_snapshot(1, 10, [0.0, 0.0, 0.0]));
        interpolator.add_snapshot(make_entity_snapshot(1, 20, [10.0, 0.0, 0.0]));

        // At current_tick 25, target = 25 - 5 = 20, should use tick 20 snapshot
        let result = interpolator.interpolate_entity(1, 25);
        assert!(result.is_some());
        let (pos, _, _) = result.unwrap();
        assert!((pos[0] - 10.0).abs() < 0.001);
    }

    #[test]
    fn interpolator_can_interpolate_checks_range() {
        let mut interpolator = EntityInterpolator::new(2);
        interpolator.add_snapshot(make_entity_snapshot(1, 10, [0.0, 0.0, 0.0]));
        interpolator.add_snapshot(make_entity_snapshot(1, 20, [10.0, 0.0, 0.0]));

        // target_tick = 15 - 2 = 13, which is in [10, 20]
        assert!(interpolator.can_interpolate(1, 15));

        // target_tick = 5 - 2 = 3, which is < 10
        assert!(!interpolator.can_interpolate(1, 5));

        // target_tick = 25 - 2 = 23, which is > 20
        assert!(!interpolator.can_interpolate(1, 25));

        // Empty history
        let empty_interp = EntityInterpolator::new(0);
        assert!(!empty_interp.can_interpolate(1, 10));
    }

    #[test]
    fn prediction_manager_set_interpolation_delay() {
        let mut manager: PredictionManager<()> = PredictionManager::new(ClientId(1));
        assert_eq!(manager.interpolator.delay_ticks, 3); // Default

        manager.set_interpolation_delay(10);
        assert_eq!(manager.interpolator.delay_ticks, 10);
    }

    #[test]
    fn prediction_manager_client_server_delta() {
        let mut manager: PredictionManager<()> = PredictionManager::new(ClientId(1));

        manager.record_prediction(10, (), vec![]);
        manager.record_prediction(20, (), vec![]);

        assert_eq!(manager.client_server_delta_frames(), 20); // 20 - 0

        // Simulate server confirming up to tick 10
        manager.confirmed_tick = 10;
        assert_eq!(manager.client_server_delta_frames(), 10); // 20 - 10
    }

    #[test]
    fn lerp_vec3_interpolates_correctly() {
        let a = [0.0, 0.0, 0.0];
        let b = [10.0, 20.0, 30.0];

        let result = lerp_vec3(&a, &b, 0.5);
        assert!((result[0] - 5.0).abs() < 0.001);
        assert!((result[1] - 10.0).abs() < 0.001);
        assert!((result[2] - 15.0).abs() < 0.001);

        let result = lerp_vec3(&a, &b, 0.0);
        assert!((result[0] - 0.0).abs() < 0.001);

        let result = lerp_vec3(&a, &b, 1.0);
        assert!((result[0] - 10.0).abs() < 0.001);
    }

    #[test]
    fn slerp_quat_normalizes_result() {
        let a = [1.0, 0.0, 0.0, 0.0];
        let b = [0.0, 1.0, 0.0, 0.0];

        let result = slerp_quat(&a, &b, 0.5);
        let len = (result[0] * result[0]
            + result[1] * result[1]
            + result[2] * result[2]
            + result[3] * result[3])
            .sqrt();
        assert!((len - 1.0).abs() < 0.001); // Should be normalized
    }

    #[test]
    fn update_interpolation_adds_snapshots() {
        let mut manager: PredictionManager<()> = PredictionManager::new(ClientId(1));

        let snapshots = vec![
            make_entity_snapshot(1, 1, [0.0, 0.0, 0.0]),
            make_entity_snapshot(2, 1, [1.0, 0.0, 0.0]),
        ];
        manager.update_interpolation(&snapshots);

        assert_eq!(manager.interpolator.snapshot_count(), 2);
        assert_eq!(manager.interpolator.last_server_tick(), 1);
    }

    #[test]
    fn interpolate_entity_delegates_to_interpolator() {
        let mut manager: PredictionManager<()> = PredictionManager::new(ClientId(1));
        manager.update_interpolation(&[
            make_entity_snapshot(1, 10, [0.0, 0.0, 0.0]),
            make_entity_snapshot(1, 20, [10.0, 0.0, 0.0]),
        ]);

        let result = manager.interpolate_entity(1, 15);
        assert!(result.is_some());
    }

    #[test]
    fn prune_interpolation_delegates_to_interpolator() {
        let mut manager: PredictionManager<()> = PredictionManager::new(ClientId(1));

        for tick in 0..40 {
            manager.update_interpolation(&[make_entity_snapshot(1, tick, [tick as f32, 0.0, 0.0])]);
        }

        manager.prune_interpolation(100);
        assert_eq!(manager.interpolator.snapshot_count(), 0);
    }

    #[test]
    fn mismatch_check_with_missing_entity_triggers_rollback() {
        let mut manager: PredictionManager<()> = PredictionManager::new(ClientId(1));
        manager.record_prediction(1, (), vec![]);
        manager.record_prediction(2, (), vec![]);

        // Server has entity 1, but local doesn't have it
        let confirmation = make_server_confirmation(2, vec![(1, [5.0, 0.0, 0.0])]);
        let local_positions: Vec<(u64, [f32; 3])> = vec![]; // Missing entity

        let result = manager.on_server_confirm(&confirmation, &local_positions);
        assert!(result.is_some()); // is_none_or returns true when entity is missing
    }

    #[test]
    fn multiple_entities_partial_mismatch() {
        let mut manager: PredictionManager<()> = PredictionManager::new(ClientId(1));
        manager.record_prediction(1, (), vec![]);
        manager.record_prediction(2, (), vec![]);

        // Two entities: one matches, one doesn't
        let confirmation = make_server_confirmation(
            2,
            vec![
                (1, [0.0, 0.0, 0.0]),  // Matches
                (2, [100.0, 0.0, 0.0]), // Mismatch
            ],
        );
        let local_positions = vec![
            (1, [0.0, 0.0, 0.0]),
            (2, [0.0, 0.0, 0.0]),
        ];

        let result = manager.on_server_confirm(&confirmation, &local_positions);
        assert!(result.is_some()); // Any mismatch triggers rollback
    }

    #[test]
    fn replay_inputs_includes_all_unconfirmed_frames() {
        let mut manager: PredictionManager<()> = PredictionManager::new(ClientId(1));

        manager.record_prediction(1, (), vec![make_input_data(InputType::MoveForward, 1.0)]);
        manager.record_prediction(2, (), vec![make_input_data(InputType::Jump, 1.0)]);
        manager.record_prediction(3, (), vec![make_input_data(InputType::Attack, 1.0)]);
        manager.record_prediction(4, (), vec![make_input_data(InputType::MoveRight, 0.5)]);

        // Confirm tick 2 with mismatch to trigger rollback
        let confirmation = make_server_confirmation(2, vec![(1, [100.0, 0.0, 0.0])]);
        let local_positions = vec![(1, [0.0, 0.0, 0.0])];

        let result = manager.on_server_confirm(&confirmation, &local_positions);
        assert!(result.is_some());

        let (_snapshot, replay_inputs) = result.unwrap();
        // Should include frames with tick > 2
        assert_eq!(replay_inputs.len(), 2);
        assert!(replay_inputs.iter().all(|f| f.tick > 2));
    }
}

// ---------------------------------------------------------------------------
// Property-based tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod proptests {
    use super::*;
    use crate::network::NetworkEntityId;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn prop_interpolation_is_linear(
            start in prop::array::uniform3(-100f32..100f32),
            end in prop::array::uniform3(-100f32..100f32),
            t in 0.0f32..=1.0,
        ) {
            // Use fixed frame distance to avoid dependent strategy issues.
            let frame_before: u64 = 0;
            let frame_after: u64 = 64;
            let interp_frame = frame_before + (t * (frame_after - frame_before) as f32) as u64;

            let mut interp = EntityInterpolator::new(0);
            interp.add_snapshot(EntitySnapshot {
                entity_id: NetworkEntityId(1),
                frame: frame_before,
                position: start,
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0, 1.0, 1.0],
            });
            interp.add_snapshot(EntitySnapshot {
                entity_id: NetworkEntityId(1),
                frame: frame_after,
                position: end,
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0, 1.0, 1.0],
            });

            let result = interp.interpolate_entity(1, interp_frame);
            // interpolate_entity may return None at edges; when it returns
            // Some, verify the interpolated position matches linear lerp.
            if let Some((pos, _, _)) = result {
                let actual_t = (interp_frame - frame_before) as f32 / (frame_after - frame_before) as f32;
                let expected_x = start[0] + (end[0] - start[0]) * actual_t;
                let expected_y = start[1] + (end[1] - start[1]) * actual_t;
                let expected_z = start[2] + (end[2] - start[2]) * actual_t;
                prop_assert!((pos[0] - expected_x).abs() < 0.001);
                prop_assert!((pos[1] - expected_y).abs() < 0.001);
                prop_assert!((pos[2] - expected_z).abs() < 0.001);
            }
        }
    }
}
