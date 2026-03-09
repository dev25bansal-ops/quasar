//! Client-side prediction (GGPO-style).
//!
//! The client applies inputs immediately for responsive gameplay.  The server
//! processes the same inputs and periodically confirms state.  On mismatch the
//! client rolls back to the last confirmed snapshot and re-applies unconfirmed
//! inputs to reconcile.
//!
//! Requires [`PhysicsSnapshot`] from `quasar-physics` for state save/restore.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::network::{ClientId, InputData};

/// Maximum number of unconfirmed frames we keep before forcing a resync.
const MAX_PREDICTION_FRAMES: usize = 64;

/// An input frame as sent to the server and retained locally for rollback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputFrame {
    /// Simulation tick this input was generated for.
    pub tick: u64,
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

/// Client-side prediction manager.
///
/// Generic over `S` — the physics snapshot type — so it works with any engine
/// that can serialize its physics state.
pub struct PredictionManager<S: PredictionSnapshot> {
    /// Ring of `(tick, snapshot, inputs)` — unconfirmed frames.
    history: VecDeque<(u64, S, Vec<InputData>)>,
    /// The last server-confirmed tick.
    pub confirmed_tick: u64,
    /// Local predicted tick (always >= confirmed_tick).
    pub predicted_tick: u64,
    /// Client ID for outgoing messages.
    pub client_id: ClientId,
    /// Tolerance for position mismatch before forcing rollback (squared).
    pub mismatch_threshold_sq: f32,
}

impl<S: PredictionSnapshot> PredictionManager<S> {
    pub fn new(client_id: ClientId) -> Self {
        Self {
            history: VecDeque::with_capacity(MAX_PREDICTION_FRAMES),
            confirmed_tick: 0,
            predicted_tick: 0,
            client_id,
            mismatch_threshold_sq: 0.001 * 0.001,
        }
    }

    /// Record a predicted frame.
    ///
    /// Call **after** applying `inputs` locally and capturing a `snapshot`
    /// of the physics state at `tick`.
    pub fn record_prediction(&mut self, tick: u64, snapshot: S, inputs: Vec<InputData>) {
        if self.history.len() >= MAX_PREDICTION_FRAMES {
            self.history.pop_front();
        }
        self.history.push_back((tick, snapshot, inputs));
        self.predicted_tick = tick;
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
    ) -> Option<(S, Vec<Vec<InputData>>)> {
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
        let entry = self
            .history
            .iter()
            .find(|(t, _, _)| *t == server_tick);

        let snapshot = match entry {
            Some((_, snap, _)) => snap.clone(),
            None => return None, // Snapshot lost — can't rollback
        };

        // Gather inputs for all frames after server_tick that need replaying.
        let replay_inputs: Vec<Vec<InputData>> = self
            .history
            .iter()
            .filter(|(t, _, _)| *t > server_tick)
            .map(|(_, _, inputs)| inputs.clone())
            .collect();

        Some((snapshot, replay_inputs))
    }

    /// Number of unconfirmed frames currently buffered.
    pub fn unconfirmed_frames(&self) -> usize {
        self.history.len()
    }

    /// Clear all history (e.g., on reconnect).
    pub fn reset(&mut self) {
        self.history.clear();
        self.confirmed_tick = 0;
        self.predicted_tick = 0;
    }
}
