# Rollback Netcode

Rollback netcode provides responsive multiplayer by predicting game state locally and correcting when the server disagrees.

## How It Works

```
Timeline:
         Predicted           Confirmed
         ┌───────┐           ┌───────┐
Client:  │ 1 2 3 │ 4 5 6 7   │ 1 2 3 │ 4 5 6 7
         └───────┘           └───────┘
              │                   │
              └───── Input ───────┘
                    Sent

         Received State
         ┌───────┐
Server:  │ 1 2 3 │ 4 ? ?
         └───────┘
              │
              └── Compare with prediction
                  └── If mismatch, rollback & replay
```

## Components

### Input History

```rust,ignore
pub struct InputHistory {
    inputs: Vec<(u64, Vec<InputData>)>,
    max_frames: usize,
}

impl InputHistory {
    pub fn new() -> Self {
        Self {
            inputs: Vec::new(),
            max_frames: 60,  // 1 second at 60 FPS
        }
    }

    pub fn push(&mut self, frame: u64, inputs: Vec<InputData>) {
        self.inputs.push((frame, inputs));

        // Remove old inputs
        while self.inputs.len() > self.max_frames {
            self.inputs.remove(0);
        }
    }

    pub fn get(&self, frame: u64) -> Option<&Vec<InputData>> {
        self.inputs.iter()
            .find(|(f, _)| *f == frame)
            .map(|(_, inputs)| inputs)
    }

    pub fn get_range(&self, start: u64, end: u64) -> Vec<(u64, &Vec<InputData>)> {
        self.inputs.iter()
            .filter(|(f, _)| *f >= start && *f < end)
            .map(|(f, i)| (*f, i))
            .collect()
    }
}
```

### State History

```rust,ignore
pub struct HistoryBuffer {
    states: Vec<(u64, WorldSnapshot)>,
    max_frames: usize,
}

#[derive(Clone)]
pub struct WorldSnapshot {
    pub entities: Vec<EntitySnapshot>,
}

#[derive(Clone)]
pub struct EntitySnapshot {
    pub entity: Entity,
    pub position: Vec3,
    pub velocity: Vec3,
    pub rotation: Quat,
}

impl HistoryBuffer {
    pub fn push(&mut self, frame: u64, state: WorldSnapshot) {
        self.states.push((frame, state));

        while self.states.len() > self.max_frames {
            self.states.remove(0);
        }
    }

    pub fn get(&self, frame: u64) -> Option<&WorldSnapshot> {
        self.states.iter()
            .find(|(f, _)| *f == frame)
            .map(|(_, s)| s)
    }
}
```

## Client Prediction

### Predict Local State

```rust,ignore
pub struct PredictionSystem;

impl System for PredictionSystem {
    fn run(&mut self, world: &mut World) {
        let dt = world.resource::<Time>().delta_seconds();

        // Apply inputs locally
        for (entity, (pos, vel, player)) in world.query_mut_3::<Position, Velocity, Player>() {
            if player.is_local {
                let inputs = world.resource::<InputState>().current();
                apply_inputs_to_velocity(vel, inputs);
            }
        }

        // Run physics
        physics_step(world, dt);

        // Record state
        let frame = world.resource::<Time>().frame();
        let snapshot = capture_snapshot(world);
        world.resource_mut::<HistoryBuffer>().push(frame, snapshot);
    }
}

fn apply_inputs_to_velocity(vel: &mut Velocity, inputs: &[InputData]) {
    for input in inputs {
        match input.input_type {
            InputType::MoveForward => vel.z += input.value * 10.0,
            InputType::MoveBackward => vel.z -= input.value * 10.0,
            InputType::MoveLeft => vel.x -= input.value * 10.0,
            InputType::MoveRight => vel.x += input.value * 10.0,
            InputType::Jump if vel.y == 0.0 => vel.y = 10.0,
            _ => {}
        }
    }
}
```

## Rollback System

### Detect Misprediction

```rust,ignore
pub struct RollbackSystem;

impl System for RollbackSystem {
    fn run(&mut self, world: &mut World) {
        let client = world.resource::<NetworkClient>().unwrap();
        let history = world.resource::<HistoryBuffer>().unwrap();
        let input_history = world.resource::<InputHistory>().unwrap();

        // Receive server state
        for (_, message) in client.poll_messages() {
            if let NetworkPayload::StateSnapshot { frame, entities } = message.payload {
                // Get our predicted state
                let predicted = history.get(frame);

                if let Some(predicted) = predicted {
                    // Compare
                    let misprediction = compare_states(predicted, &entities);

                    if !misprediction.is_empty() {
                        log::info!("Misprediction at frame {}, rolling back", frame);

                        // Rollback and replay
                        rollback_and_replay(world, frame, &entities, &input_history);
                    }
                }
            }
        }
    }
}

fn compare_states(predicted: &WorldSnapshot, server: &[EntitySnapshot]) -> Vec<Misprediction> {
    let mut mispredictions = Vec::new();

    for server_entity in server {
        if let Some(predicted_entity) = predicted.get(server_entity.entity) {
            let pos_diff = predicted_entity.position.distance(server_entity.position);

            if pos_diff > 0.1 {  // Threshold
                mispredictions.push(Misprediction {
                    entity: server_entity.entity,
                    field: "position".into(),
                    predicted: predicted_entity.position,
                    actual: server_entity.position,
                });
            }
        }
    }

    mispredictions
}
```

### Rollback and Replay

```rust,ignore
fn rollback_and_replay(
    world: &mut World,
    server_frame: u64,
    server_state: &[EntitySnapshot],
    input_history: &InputHistory,
) {
    let current_frame = world.resource::<Time>().frame();

    // Step 1: Restore server state
    for snapshot in server_state {
        if let Some(pos) = world.get_mut::<Position>(snapshot.entity) {
            pos.0 = snapshot.position;
        }
        if let Some(vel) = world.get_mut::<Velocity>(snapshot.entity) {
            vel.0 = snapshot.velocity;
        }
    }

    // Step 2: Replay inputs from server_frame to current_frame
    let dt = 1.0 / 60.0;
    for frame in server_frame..current_frame {
        if let Some(inputs) = input_history.get(frame) {
            // Apply inputs
            for (entity, vel) in world.query_mut::<Velocity>() {
                apply_inputs_to_velocity(vel, inputs);
            }

            // Run physics
            physics_step(world, dt);
        }
    }

    // Step 3: Record new predicted state
    let snapshot = capture_snapshot(world);
    world.resource_mut::<HistoryBuffer>().push(current_frame, snapshot);
}
```

## Server Authority

### Server Validation

```rust,ignore
pub struct ServerValidationSystem;

impl System for ServerValidationSystem {
    fn run(&mut self, world: &mut World) {
        let server = world.resource_mut::<NetworkServer>().unwrap();

        for (client_id, message) in server.poll_messages() {
            if let NetworkPayload::Input { inputs, .. } = message.payload {
                // Validate inputs
                if !validate_inputs(&inputs) {
                    log::warn!("Invalid inputs from client {:?}", client_id);
                    continue;
                }

                // Store for processing
                world.resource_mut::<PendingInputs>().push(client_id, inputs);
            }
        }
    }
}

fn validate_inputs(inputs: &[InputData]) -> bool {
    for input in inputs {
        // Check for NaN/Inf
        if !input.value.is_finite() {
            return false;
        }

        // Check bounds
        if input.value.abs() > 100.0 {
            return false;
        }

        // Check rate limiting
        // ...
    }
    true
}
```

### Server State Broadcast

```rust,ignore
pub struct ServerBroadcastSystem;

impl System for ServerBroadcastSystem {
    fn run(&mut self, world: &mut World) {
        let server = world.resource_mut::<NetworkServer>().unwrap();
        let frame = server.frame();

        // Broadcast every few frames
        if frame % 3 != 0 {
            return;
        }

        // Capture state
        let entities: Vec<EntitySnapshot> = world.query::<(Entity, Position, Velocity)>()
            .iter()
            .map(|(e, pos, vel)| EntitySnapshot {
                entity: e,
                position: pos.0,
                velocity: vel.0,
            })
            .collect();

        // Broadcast
        server.broadcast(NetworkPayload::StateSnapshot { frame, entities });
    }
}
```

## Lag Compensation

### Server-Side Rewinding

```rust,ignore
pub struct LagCompensationManager {
    history: Vec<(u64, WorldSnapshot)>,
    max_duration_ms: u64,
}

impl LagCompensationManager {
    pub fn get_state_at_time(&self, time_ms: u64) -> Option<&WorldSnapshot> {
        // Find closest state before time
        self.history.iter()
            .rev()
            .find(|(t, _)| *t <= time_ms)
            .map(|(_, s)| s)
    }

    pub fn rewind_for_client(&self, client_rtt_ms: f64) -> Option<&WorldSnapshot> {
        let current_time = current_time_ms();
        let target_time = current_time - (client_rtt_ms as u64);
        self.get_state_at_time(target_time)
    }
}

// For hit detection
pub fn process_shot(
    manager: &LagCompensationManager,
    shooter_id: ClientId,
    target_pos: Vec3,
    direction: Vec3,
) -> Option<Entity> {
    let rtt = get_client_rtt(shooter_id);
    let past_state = manager.rewind_for_client(rtt)?;

    // Check against historical positions
    for (entity, pos) in past_state.query::<Position>() {
        if ray_intersects(target_pos, direction, pos.0) {
            return Some(entity);
        }
    }
    None
}
```

## Smoothing

### State Interpolation

```rust,ignore
pub struct InterpolationSystem;

impl System for InterpolationSystem {
    fn run(&mut self, world: &mut World) {
        let interpolation = world.resource_mut::<StateInterpolation>().unwrap();

        // Get two most recent states
        let (state_a, time_a) = interpolation.states.get(interpolation.states.len() - 2)?;
        let (state_b, time_b) = interpolation.states.last()?;

        // Calculate interpolation factor
        let current_time = current_time_ms();
        let t = ((current_time - time_a) as f32) / ((time_b - time_a) as f32);
        let t = t.clamp(0.0, 1.0);

        // Interpolate entity positions
        for snapshot_a in &state_a.entities {
            if let Some(snapshot_b) = state_b.get(snapshot_a.entity) {
                if let Some(pos) = world.get_mut::<Position>(snapshot_a.entity) {
                    pos.0 = snapshot_a.position.lerp(snapshot_b.position, t);
                }
            }
        }
    }
}
```

## Best Practices

### 1. Limit Prediction Scope

```rust,ignore
// Only predict player-controlled entities
for (entity, (pos, vel, player)) in world.query_mut_3::<Position, Velocity, Player>() {
    if player.is_local {
        // Predict
    }
}
```

### 2. Use Fixed Timestep

```rust,ignore
// Consistent physics across all clients
const FIXED_DT: f32 = 1.0 / 60.0;

fn physics_step(world: &mut World, dt: f32) {
    // Always use FIXED_DT, not actual dt
    // This ensures identical results on all machines
}
```

### 3. Handle Desync Gracefully

```rust,ignore
fn handle_desync(world: &mut World, entity: Entity) {
    // Snap to server position for large errors
    if error_magnitude > SNAP_THRESHOLD {
        snap_to_server(world, entity);
    } else {
        // Smooth correction
        interpolate_to_server(world, entity);
    }
}
```

## Debugging

### Visualize Prediction

```rust,ignore
fn debug_prediction(world: &World) {
    let history = world.resource::<HistoryBuffer>().unwrap();

    for (frame, state) in &history.states {
        for snapshot in &state.entities {
            // Draw ghost showing predicted position
            debug_draw_sphere(snapshot.position, 0.1, Color::RED.alpha(0.3));
        }
    }
}
```

## Next Steps

- [Network Protocol](protocol.md)
- [Multiplayer Example](../examples/multiplayer.md)
