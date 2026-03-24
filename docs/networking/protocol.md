# Network Protocol

Quasar's networking layer provides client-server multiplayer support with rollback netcode, state replication, and lag compensation.

## Architecture

```
┌──────────────┐                    ┌──────────────┐
│   Client A   │◀──────QUIC/UDP────▶│              │
├──────────────┤                    │    Server    │
│   Client B   │◀──────QUIC/UDP────▶│ (Authoritative)
├──────────────┤                    │              │
│   Client C   │◀──────QUIC/UDP────▶│              │
└──────────────┘                    └──────────────┘
       │                                   │
       ▼                                   ▼
  InputPrediction                    StateReplication
  RollbackNetcode                    LagCompensation
```

## Transport

### UDP Transport

Low-latency unreliable transport:

```rust,ignore
let transport = UdpTransport::bind("0.0.0.0:7777")?;

transport.send_to(&addr, &data)?;
let (from, payload) = transport.receive_raw()?;
```

### QUIC Transport

Reliable + unreliable streams:

```rust,ignore
let transport = QuicTransport::bind("0.0.0.0:7777", config)?;

// Reliable channel
transport.send_reliable(&client_id, &data)?;

// Unreliable channel (game state)
transport.send_unreliable(&client_id, &data)?;
```

## Protocol Messages

### Message Structure

```rust,ignore
pub struct NetworkMessage {
    pub sequence: u64,
    pub timestamp: u64,
    pub payload: NetworkPayload,
}

pub enum NetworkPayload {
    // Connection
    ConnectionRequest { client_id: ClientId },
    ConnectionAccepted { client_id: ClientId },
    ConnectionDenied { reason: String },
    Disconnect { client_id: ClientId },

    // Entity replication
    EntitySpawn { entity_id: NetworkEntityId, components: Vec<ComponentData> },
    EntityDespawn { entity_id: NetworkEntityId },
    EntityUpdate { entity_id: NetworkEntityId, components: Vec<ComponentData> },

    // Input
    Input { client_id: ClientId, inputs: Vec<InputData> },

    // State sync
    StateSnapshot { frame: u64, entities: Vec<EntitySnapshot> },

    // RPC
    Rpc { entity_id: NetworkEntityId, method: String, args: Vec<u8> },
}
```

### Serialization

Messages are serialized with bincode:

```rust,ignore
let config = bincode::config::standard()
    .with_limit::<32>()                    // Max recursion depth
    .with_limit::<{ 1024 * 1024 }>();      // Max 1MB

let encoded = bincode::serde::encode_to_vec(&msg, config)?;
let (decoded, _): (NetworkMessage, _) = bincode::serde::decode_from_slice(&encoded, config)?;
```

## State Replication

### Entity Mapping

```rust,ignore
// Server entity ID
let server_entity = Entity { index: 42, generation: 1 };

// Network entity ID (sent to clients)
let network_id = NetworkEntityId(42);

// Client maps back
let client_entity = world.spawn();
network_to_entity.insert(network_id, client_entity);
```

### Snapshot Sync

```rust,ignore
pub struct EntitySnapshot {
    pub entity_id: NetworkEntityId,
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
    pub frame: u64,
}

// Server sends
let snapshot = StateSnapshot {
    frame: current_frame,
    entities: world.query::<Transform>().iter()
        .map(|(e, t)| EntitySnapshot::from_entity(e, t))
        .collect(),
};
server.broadcast(NetworkPayload::StateSnapshot(snapshot));
```

### Delta Compression

Only send changed components:

```rust,ignore
pub struct DeltaCompressor {
    last_sent: HashMap<NetworkEntityId, Vec<u8>>,
}

impl DeltaCompressor {
    pub fn encode(&mut self, entity: NetworkEntityId, data: &[u8]) -> Option<Vec<u8>> {
        let last = self.last_sent.get(&entity)?;
        let delta = compute_delta(last, data);
        self.last_sent.insert(entity, data.to_vec());
        Some(delta)
    }
}
```

## Input Handling

### Client Input

```rust,ignore
pub struct InputData {
    pub input_type: InputType,
    pub value: f32,
}

pub enum InputType {
    MoveForward,
    MoveBackward,
    MoveLeft,
    MoveRight,
    Jump,
    Attack,
    Interact,
    Custom(String),
}

// Client sends input
let input = InputData { input_type: InputType::MoveForward, value: 1.0 };
client.send(NetworkPayload::Input { client_id, inputs: vec![input] });
```

### Input History

```rust,ignore
pub struct InputHistory {
    inputs: Vec<(u64, Vec<InputData>)>,
    max_frames: usize,
}

impl InputHistory {
    pub fn push(&mut self, frame: u64, inputs: Vec<InputData>) {
        self.inputs.push((frame, inputs));
        if self.inputs.len() > self.max_frames {
            self.inputs.remove(0);
        }
    }

    pub fn get(&self, frame: u64) -> Option<&Vec<InputData>> {
        self.inputs.iter().find(|(f, _)| *f == frame).map(|(_, i)| i)
    }
}
```

## Rollback Netcode

### Prediction

```rust,ignore
pub struct Misprediction {
    pub frame: u64,
    pub local_state: Vec<u8>,
    pub server_state: Vec<u8>,
}

// Client prediction
fn predict_movement(world: &mut World, inputs: &[InputData]) {
    for (entity, (pos, vel)) in world.query_mut_2::<Position, Velocity>() {
        for input in inputs {
            match input.input_type {
                InputType::MoveForward => vel.dx += input.value,
                // ...
            }
        }
        pos.x += vel.dx;
    }
}
```

### Rollback

```rust,ignore
pub fn rollback_system(
    world: &mut World,
    history: &mut HistoryBuffer,
    server_frame: u64,
    server_state: &[EntitySnapshot],
) {
    // Find misprediction
    let local_state = history.get(server_frame);
    let misprediction = compare_states(local_state, server_state);

    if !misprediction.is_empty() {
        // Rollback to server frame
        restore_state(world, server_state);

        // Replay inputs since that frame
        for frame in server_frame..current_frame {
            let inputs = input_history.get(frame);
            apply_inputs(world, inputs);
            physics_step(world);
        }
    }
}
```

## Lag Compensation

### Server-Side Rewinding

```rust,ignore
pub struct LagCompensationManager {
    history: HistoryBuffer,
    max_history_ms: u64,
}

impl LagCompensationManager {
    pub fn get_state_at(&self, timestamp: u64) -> Option<&WorldSnapshot> {
        self.history.get_by_time(timestamp)
    }

    pub fn rewind_for_client(&self, client_rtt: f64) -> Option<&WorldSnapshot> {
        let rewind_time = current_time() - client_rtt;
        self.get_state_at(rewind_time)
    }
}
```

### Hit Detection

```rust,ignore
fn process_shot(
    manager: &LagCompensationManager,
    shooter: ClientId,
    target_pos: Vec3,
    direction: Vec3,
) -> bool {
    let client_rtt = get_client_rtt(shooter);

    // Rewind world to client's view
    let past_state = manager.rewind_for_client(client_rtt)?;

    // Check hit against historical positions
    for (entity, pos) in past_state.query::<Position>() {
        if ray_intersects(target_pos, direction, pos) {
            return true;
        }
    }
    false
}
```

## Rate Limiting

### Message Rate Limits

```rust,ignore
pub struct RateLimiter {
    window_start: Instant,
    message_count: usize,
    max_per_window: usize,
    window_duration: Duration,
}

impl RateLimiter {
    pub fn check_and_increment(&mut self) -> bool {
        if self.window_start.elapsed() > self.window_duration {
            self.window_start = Instant::now();
            self.message_count = 0;
        }

        if self.message_count < self.max_per_window {
            self.message_count += 1;
            true
        } else {
            false
        }
    }
}
```

### Client Security

```rust,ignore
pub struct ClientSecurityState {
    pub rate_limiter: RateLimiter,
    pub last_sequence: u64,
}

impl NetworkTransportResource {
    pub fn validate_message(&self, msg: &NetworkMessage, from: SocketAddr) -> Result<(), NetworkError> {
        // Validate sequence (prevent replay attacks)
        if msg.sequence <= self.last_sequence {
            return Err(NetworkError::InvalidSequence);
        }

        // Validate entity IDs
        if let NetworkPayload::EntityUpdate { entity_id, .. } = &msg.payload {
            if !self.valid_entities.contains(entity_id) {
                return Err(NetworkError::InvalidEntity);
            }
        }

        Ok(())
    }
}
```

## Connection Flow

### Client Connection

```
Client                              Server
   │                                   │
   │──── ConnectionRequest ───────────▶│
   │                                   │
   │◀─── ConnectionAccepted ───────────│
   │                                   │
   │──── Input (every frame) ─────────▶│
   │                                   │
   │◀─── StateSnapshot (periodic) ─────│
   │                                   │
```

### Disconnection

```rust,ignore
// Graceful disconnect
client.send(NetworkPayload::Disconnect { client_id });

// Timeout detection
if last_received.elapsed() > TIMEOUT_DURATION {
    server.disconnect_client(client_id);
}
```

## Metrics

### Connection Metrics

```rust,ignore
pub struct ConnectionMetrics {
    pub rtt_ms: f64,
    pub packet_loss: f32,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub messages_per_second: f32,
}
```

### Network Stats

```rust,ignore
pub struct NetworkMetrics {
    pub total_clients: usize,
    pub total_bytes_sent: u64,
    pub total_bytes_received: u64,
    pub average_rtt: f64,
    pub tick_rate: u32,
}
```

## Best Practices

### 1. Serialize Efficiently

```rust,ignore
// Bad - JSON
let data = serde_json::to_vec(&msg)?;

// Better - bincode
let data = bincode::serde::encode_to_vec(&msg, config)?;
```

### 2. Batch Messages

```rust,ignore
// Bad - send each entity separately
for entity in entities {
    send(NetworkPayload::EntityUpdate { entity, ... });
}

// Better - batch in one message
let updates: Vec<_> = entities.iter()
    .map(|e| EntityUpdate::from(e))
    .collect();
send(NetworkPayload::BatchUpdates { updates });
```

### 3. Use Unreliable for Game State

```rust,ignore
// Game state - unreliable (newest only)
transport.send_unreliable(&client, &state)?;

// Important events - reliable
transport.send_reliable(&client, &event)?;
```

## Security

### Entity Validation

```rust,ignore
fn validate_entity_access(client: ClientId, entity: NetworkEntityId, world: &World) -> bool {
    // Check client owns this entity
    if let Some(owner) = world.get::<Owner>(entity) {
        owner.0 == client
    } else {
        false
    }
}
```

### Input Validation

```rust,ignore
fn validate_input(input: &InputData) -> bool {
    // Clamp values to reasonable range
    if !input.value.is_finite() || input.value.abs() > 1000.0 {
        return false;
    }
    true
}
```

## Next Steps

- [Rollback Netcode](rollback.md)
- [Multiplayer Example](../examples/multiplayer.md)
