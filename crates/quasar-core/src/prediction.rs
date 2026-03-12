//! Client-side prediction (GGPO-style).
//!
//! The client applies inputs immediately for responsive gameplay.  The server
//! processes the same inputs and periodically confirms state.  On mismatch the
//! client rolls back to the last confirmed snapshot and re-applies unconfirmed
//! inputs to reconcile.
//!
//! Entity interpolation smooths movement for non-player entities.
//!
//! Requires [`PhysicsSnapshot`] from `quasar-physics` for state save/restore.

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
        let target_tick = if current_tick > self.delay_ticks as u64 {
            current_tick - self.delay_ticks as u64
        } else {
            0
        };

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
                let earliest = snapshots.iter().min_by_key(|s| s.tick).unwrap();
                Some((earliest.position, earliest.rotation, earliest.scale))
            }
            (Some(_before), None) => {
                // Only past snapshots - use the latest.
                let latest = snapshots.iter().max_by_key(|s| s.tick).unwrap();
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
        let target_tick = if current_tick > self.delay_ticks as u64 {
            current_tick - self.delay_ticks as u64
        } else {
            0
        };

        if self.history.is_empty() {
            return false;
        }

        let min_tick = self.history.front().unwrap().tick;
        let max_tick = self.history.back().unwrap().tick;

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
/// Generic over `S` — the physics snapshot type — so it works with any engine
/// that can serialize its physics state.
pub struct PredictionManager<S: PredictionSnapshot> {
    /// Ring of `(tick, snapshot, inputs)` — unconfirmed frames.
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
            .map_or(false, |(t, _, _)| *t < server_tick)
        {
            self.history.pop_front();
        }

        self.confirmed_tick = server_tick;

        // Check for mismatch.
        let mismatch = confirmation.positions.iter().any(|(eid, server_pos)| {
            local_positions
                .iter()
                .find(|(lid, _)| *lid == *eid)
                .map_or(true, |(_, local_pos)| {
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
            None => return None, // Snapshot lost — can't rollback
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

        let latest_tick = self.history.back().unwrap().0;
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
