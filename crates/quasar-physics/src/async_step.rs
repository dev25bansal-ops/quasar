//! Async physics stepping — runs the physics simulation on a dedicated thread
//! at a fixed 60 Hz rate and provides interpolated positions for rendering.

use std::sync::{Arc, Mutex};
use std::thread;

use crossbeam::channel::{self, Receiver, Sender};

use crate::world::PhysicsWorld;

/// Fixed physics timestep (60 Hz).
const PHYSICS_DT: f32 = 1.0 / 60.0;

/// Commands sent from the game thread to the physics thread.
pub enum PhysicsCommand {
    /// Advance the simulation by one fixed step.
    Step,
    /// Shut down the physics thread.
    Shutdown,
}

/// A snapshot of body positions/rotations after a physics step.
#[derive(Clone)]
pub struct InterpolationSnapshot {
    /// `(body_handle_raw, position, rotation_quat)` per body.
    pub bodies: Vec<(u128, [f32; 3], [f32; 4])>,
}

/// Manages the physics thread and provides interpolated transforms for rendering.
pub struct AsyncPhysicsStepper {
    /// Send commands to the physics thread.
    cmd_tx: Sender<PhysicsCommand>,
    /// The most recent two snapshots for interpolation.
    snapshots: Arc<Mutex<(InterpolationSnapshot, InterpolationSnapshot)>>,
    /// Accumulated time since last physics step (set by the game thread).
    pub accumulated_time: f32,
    /// Handle to the spawned thread.
    _thread: Option<thread::JoinHandle<()>>,
}

impl AsyncPhysicsStepper {
    /// Spawn the physics thread with the given [`PhysicsWorld`].
    ///
    /// The world is moved into the physics thread.
    pub fn new(world: PhysicsWorld) -> Self {
        let empty_snap = InterpolationSnapshot {
            bodies: Vec::new(),
        };
        let snapshots = Arc::new(Mutex::new((empty_snap.clone(), empty_snap)));

        let (cmd_tx, cmd_rx): (Sender<PhysicsCommand>, Receiver<PhysicsCommand>) =
            channel::unbounded();
        let snap_ref = snapshots.clone();

        let handle = thread::Builder::new()
            .name("quasar-physics".into())
            .spawn(move || {
                physics_thread_main(world, cmd_rx, snap_ref);
            })
            .expect("Failed to spawn physics thread");

        Self {
            cmd_tx,
            snapshots,
            accumulated_time: 0.0,
            _thread: Some(handle),
        }
    }

    /// Tick the async stepper by `delta_seconds` of real time.
    ///
    /// Sends one `Step` command per fixed-step interval that has elapsed.
    /// Returns the number of steps dispatched.
    pub fn tick(&mut self, delta_seconds: f32) -> u32 {
        self.accumulated_time += delta_seconds;
        let mut steps = 0u32;
        while self.accumulated_time >= PHYSICS_DT {
            self.accumulated_time -= PHYSICS_DT;
            let _ = self.cmd_tx.send(PhysicsCommand::Step);
            steps += 1;
        }
        steps
    }

    /// Compute the interpolation alpha for the current frame.
    pub fn alpha(&self) -> f32 {
        (self.accumulated_time / PHYSICS_DT).clamp(0.0, 1.0)
    }

    /// Get the interpolated position for body at `index` in the snapshot list.
    ///
    /// `alpha = time_since_last / PHYSICS_DT`
    pub fn interpolated_position(&self, index: usize) -> Option<[f32; 3]> {
        let alpha = self.alpha();
        let guard = self.snapshots.lock().ok()?;
        let (prev, curr) = &*guard;

        let prev_pos = prev.bodies.get(index).map(|b| b.1)?;
        let curr_pos = curr.bodies.get(index).map(|b| b.1)?;

        Some(lerp3(prev_pos, curr_pos, alpha))
    }

    /// Get the interpolated rotation (quaternion slerp approximated as nlerp).
    pub fn interpolated_rotation(&self, index: usize) -> Option<[f32; 4]> {
        let alpha = self.alpha();
        let guard = self.snapshots.lock().ok()?;
        let (prev, curr) = &*guard;

        let prev_rot = prev.bodies.get(index).map(|b| b.2)?;
        let curr_rot = curr.bodies.get(index).map(|b| b.2)?;

        Some(nlerp4(prev_rot, curr_rot, alpha))
    }

    /// Get the latest snapshot pair for custom interpolation.
    pub fn snapshots(&self) -> Option<(InterpolationSnapshot, InterpolationSnapshot)> {
        self.snapshots.lock().ok().map(|g| g.clone())
    }

    /// Shut down the physics thread.
    pub fn shutdown(&self) {
        let _ = self.cmd_tx.send(PhysicsCommand::Shutdown);
    }
}

impl Drop for AsyncPhysicsStepper {
    fn drop(&mut self) {
        self.shutdown();
    }
}

// ---------------------------------------------------------------------------
// Physics thread
// ---------------------------------------------------------------------------

fn physics_thread_main(
    mut world: PhysicsWorld,
    cmd_rx: Receiver<PhysicsCommand>,
    snapshots: Arc<Mutex<(InterpolationSnapshot, InterpolationSnapshot)>>,
) {
    loop {
        match cmd_rx.recv() {
            Ok(PhysicsCommand::Step) => {
                world.step_with_dt(PHYSICS_DT);

                // Build snapshot.
                let snap = snapshot_from_world(&world);
                if let Ok(mut guard) = snapshots.lock() {
                    guard.0 = guard.1.clone();
                    guard.1 = snap;
                }
            }
            Ok(PhysicsCommand::Shutdown) | Err(_) => break,
        }
    }
}

fn snapshot_from_world(world: &PhysicsWorld) -> InterpolationSnapshot {
    let mut bodies = Vec::new();
    for (handle, rb) in world.bodies.iter() {
        let t = rb.translation();
        let r = rb.rotation();
        // Encode handle as u128 for exterior storage.
        let handle_bits = handle.into_raw_parts();
        let id = ((handle_bits.0 as u128) << 64) | (handle_bits.1 as u128);
        bodies.push((id, [t.x, t.y, t.z], [r.i, r.j, r.k, r.w]));
    }

    InterpolationSnapshot { bodies }
}

// ---------------------------------------------------------------------------
// Math helpers
// ---------------------------------------------------------------------------

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// Normalized-lerp quaternion interpolation (fast slerp approximation).
fn nlerp4(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    // Ensure shortest path (flip if dot < 0).
    let dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
    let sign = if dot < 0.0 { -1.0 } else { 1.0 };
    let r = [
        a[0] + (b[0] * sign - a[0]) * t,
        a[1] + (b[1] * sign - a[1]) * t,
        a[2] + (b[2] * sign - a[2]) * t,
        a[3] + (b[3] * sign - a[3]) * t,
    ];
    let len = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2] + r[3] * r[3]).sqrt();
    if len < 1e-10 {
        return a;
    }
    [r[0] / len, r[1] / len, r[2] / len, r[3] / len]
}
