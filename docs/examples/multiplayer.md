# Multiplayer Game Example

This guide walks through building a simple multiplayer game using Quasar's networking features.

## Project Setup

### Cargo.toml

```toml
[package]
name = "multiplayer-demo"
version = "0.1.0"
edition = "2021"

[dependencies]
quasar-engine = { path = "../quasar/crates/quasar-engine" }
quasar-network = { path = "../quasar/crates/quasar-network", features = ["quinn-transport"] }
quasar-core = { path = "../quasar/crates/quasar-core" }
serde = { version = "1", features = ["derive"] }
```

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        Game Client                           │
├─────────────────────────────────────────────────────────────┤
│  ┌──────────────┐   ┌──────────────┐   ┌──────────────┐    │
│  │ Input System │──▶│Prediction    │──▶│Local State   │    │
│  └──────────────┘   └──────────────┘   └──────────────┘    │
│         │                                       │             │
│         ▼                                       ▼             │
│  ┌──────────────┐                       ┌──────────────┐    │
│  │ Network Send │                       │ Render       │    │
│  └──────────────┘                       └──────────────┘    │
└─────────────────────────────────────────────────────────────┘
         │                                        ▲
         │ Input Packets                     State Snapshots
         ▼                                        │
┌─────────────────────────────────────────────────────────────┐
│                        Game Server                           │
├─────────────────────────────────────────────────────────────┤
│  ┌──────────────┐   ┌──────────────┐   ┌──────────────┐    │
│  │Network Recv  │──▶│Game Logic    │──▶│Broadcast     │    │
│  └──────────────┘   └──────────────┘   └──────────────┘    │
│                            │                                 │
│                            ▼                                 │
│                     ┌──────────────┐                        │
│                     │ Authoritative│                        │
│                     │ State        │                        │
│                     └──────────────┘                        │
└─────────────────────────────────────────────────────────────┘
```

## Shared Components

```rust,ignore
// shared.rs

use quasar_core::Component;
use serde::{Serialize, Deserialize};

#[derive(Component, Clone, Serialize, Deserialize)]
pub struct Player {
    pub id: u64,
    pub name: String,
}

#[derive(Component, Clone, Serialize, Deserialize)]
pub struct Position {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Component, Clone, Serialize, Deserialize)]
pub struct Velocity {
    pub dx: f32,
    pub dy: f32,
    pub dz: f32,
}

#[derive(Component, Clone, Serialize, Deserialize)]
pub struct Health {
    pub current: f32,
    pub max: f32,
}
```

## Server Implementation

```rust,ignore
// server.rs

use quasar_core::{App, Plugin, System, World, SystemStage};
use quasar_network::{NetworkServer, NetworkConfig, NetworkPayload, ClientId};

pub struct GameServerPlugin;

impl Plugin for GameServerPlugin {
    fn name(&self) -> &str { "game_server" }

    fn build(&self, app: &mut App) {
        app.world.insert_resource(NetworkServer::new(NetworkConfig {
            port: 7777,
            max_clients: 32,
            tick_rate: 60,
        }));

        app.schedule.add_system(SystemStage::Update, Box::new(HandleConnections));
        app.schedule.add_system(SystemStage::Update, Box::new(ProcessInputs));
        app.schedule.add_system(SystemStage::Update, Box::new(BroadcastState));
    }
}

pub struct HandleConnections;

impl System for HandleConnections {
    fn name(&self) -> &str { "handle_connections" }

    fn run(&mut self, world: &mut World) {
        let server = world.resource_mut::<NetworkServer>().unwrap();

        for event in server.poll_events() {
            match event {
                NetworkEvent::Connected(client_id) => {
                    log::info!("Client connected: {:?}", client_id);
                    spawn_player(world, client_id);
                }
                NetworkEvent::Disconnected(client_id) => {
                    log::info!("Client disconnected: {:?}", client_id);
                    despawn_player(world, client_id);
                }
            }
        }
    }
}

fn spawn_player(world: &mut World, client_id: ClientId) {
    let entity = world.spawn();
    world.insert(entity, Player { id: client_id.0, name: format!("Player_{}", client_id.0) });
    world.insert(entity, Position { x: 0.0, y: 0.0, z: 0.0 });
    world.insert(entity, Velocity { dx: 0.0, dy: 0.0, dz: 0.0 });
    world.insert(entity, Health { current: 100.0, max: 100.0 });
}

pub struct ProcessInputs;

impl System for ProcessInputs {
    fn name(&self) -> &str { "process_inputs" }

    fn run(&mut self, world: &mut World) {
        let server = world.resource_mut::<NetworkServer>().unwrap();

        for (client_id, message) in server.poll_messages() {
            if let NetworkPayload::Input { inputs, .. } = message.payload {
                apply_inputs(world, client_id, &inputs);
            }
        }
    }
}

fn apply_inputs(world: &mut World, client_id: ClientId, inputs: &[InputData]) {
    // Find player entity
    for (entity, player) in world.query::<Player>() {
        if player.id == client_id.0 {
            // Get velocity component
            if let Some(vel) = world.get_mut::<Velocity>(entity) {
                for input in inputs {
                    match input.input_type {
                        InputType::MoveForward => vel.dz += input.value * 10.0,
                        InputType::MoveBackward => vel.dz -= input.value * 10.0,
                        InputType::MoveLeft => vel.dx -= input.value * 10.0,
                        InputType::MoveRight => vel.dx += input.value * 10.0,
                        _ => {}
                    }
                }
            }
        }
    }
}

pub struct BroadcastState;

impl System for BroadcastState {
    fn name(&self) -> &str { "broadcast_state" }

    fn run(&mut self, world: &mut World) {
        let server = world.resource_mut::<NetworkServer>().unwrap();

        // Build snapshot
        let mut entities = Vec::new();
        for (entity, pos) in world.query::<Position>() {
            entities.push(EntitySnapshot {
                entity_id: NetworkEntityId(entity.index()),
                position: [pos.x, pos.y, pos.z],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0, 1.0, 1.0],
                frame: server.frame(),
            });
        }

        // Broadcast to all clients
        server.broadcast(NetworkPayload::StateSnapshot {
            frame: server.frame(),
            entities,
        });
    }
}

fn main() {
    env_logger::init();

    App::new()
        .add_plugin(GameServerPlugin)
        .run();
}
```

## Client Implementation

```rust,ignore
// client.rs

use quasar_core::{App, Plugin, System, World, SystemStage};
use quasar_network::{NetworkClient, NetworkConfig, NetworkPayload};

pub struct GameClientPlugin {
    pub server_addr: String,
}

impl Plugin for GameClientPlugin {
    fn name(&self) -> &str { "game_client" }

    fn build(&self, app: &mut App) {
        app.world.insert_resource(NetworkClient::connect(&self.server_addr));
        app.world.insert_resource(InputHistory::new());

        app.schedule.add_system(SystemStage::PreUpdate, Box::new(ReceiveState));
        app.schedule.add_system(SystemStage::Update, Box::new(SendInput));
        app.schedule.add_system(SystemStage::Update, Box::new(PredictMovement));
    }
}

pub struct SendInput;

impl System for SendInput {
    fn name(&self) -> &str { "send_input" }

    fn run(&mut self, world: &mut World) {
        let client = world.resource_mut::<NetworkClient>().unwrap();
        let input = gather_input(world);

        // Record in history for prediction
        let history = world.resource_mut::<InputHistory>().unwrap();
        history.push(client.frame(), input.clone());

        // Send to server
        client.send(NetworkPayload::Input {
            client_id: client.id(),
            inputs: input,
        });
    }
}

fn gather_input(world: &World) -> Vec<InputData> {
    let mut inputs = Vec::new();

    if let Some(keyboard) = world.resource::<KeyboardInput>() {
        if keyboard.is_pressed(KeyCode::W) {
            inputs.push(InputData { input_type: InputType::MoveForward, value: 1.0 });
        }
        if keyboard.is_pressed(KeyCode::S) {
            inputs.push(InputData { input_type: InputType::MoveBackward, value: 1.0 });
        }
        if keyboard.is_pressed(KeyCode::A) {
            inputs.push(InputData { input_type: InputType::MoveLeft, value: 1.0 });
        }
        if keyboard.is_pressed(KeyCode::D) {
            inputs.push(InputData { input_type: InputType::MoveRight, value: 1.0 });
        }
    }

    inputs
}

pub struct PredictMovement;

impl System for PredictMovement {
    fn name(&self) -> &str { "predict_movement" }

    fn run(&mut self, world: &mut World) {
        let dt = world.resource::<Time>().unwrap().delta_seconds();

        for (entity, (pos, vel)) in world.query_mut_2::<Position, Velocity>() {
            pos.x += vel.dx * dt;
            pos.y += vel.dy * dt;
            pos.z += vel.dz * dt;

            // Damping
            vel.dx *= 0.95;
            vel.dy *= 0.95;
            vel.dz *= 0.95;
        }
    }
}

pub struct ReceiveState;

impl System for ReceiveState {
    fn name(&self) -> &str { "receive_state" }

    fn run(&mut self, world: &mut World) {
        let client = world.resource_mut::<NetworkClient>().unwrap();

        for (_, message) in client.poll_messages() {
            if let NetworkPayload::StateSnapshot { frame, entities } = message.payload {
                apply_snapshot(world, frame, &entities);
            }
        }
    }
}

fn apply_snapshot(world: &mut World, frame: u64, entities: &[EntitySnapshot]) {
    // Check for misprediction
    let history = world.resource::<InputHistory>().unwrap();
    let predicted_state = history.get_state(frame);

    for snapshot in entities {
        if let Some(entity) = world.get_entity_by_network_id(snapshot.entity_id) {
            if let Some(pos) = world.get_mut::<Position>(entity) {
                // Interpolate to server position
                pos.x = pos.x.lerp(snapshot.position[0], 0.1);
                pos.y = pos.y.lerp(snapshot.position[1], 0.1);
                pos.z = pos.z.lerp(snapshot.position[2], 0.1);
            }
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let server_addr = args.get(1).unwrap_or(&"127.0.0.1:7777".to_string()).clone();

    App::new()
        .add_plugin(RenderPlugin::default())
        .add_plugin(GameClientPlugin { server_addr })
        .run();
}
```

## Running the Game

### Start Server

```bash
cargo run --bin server
```

### Start Client(s)

```bash
cargo run --bin client -- 127.0.0.1:7777
```

## Optimization Tips

### 1. Use Delta Compression

Only send changed state:

```rust,ignore
impl BroadcastState {
    fn run(&mut self, world: &mut World) {
        let mut compressor = DeltaCompressor::new();

        for (entity, pos) in world.query::<Position>() {
            let data = serialize_position(pos);
            if let Some(delta) = compressor.encode(entity, &data) {
                // Only send delta
                deltas.push(delta);
            }
        }
    }
}
```

### 2. Interpolate Client State

Smooth out network jitter:

```rust,ignore
fn apply_snapshot(world: &mut World, snapshot: &[EntitySnapshot]) {
    for snap in snapshot {
        if let Some(pos) = world.get_mut::<Position>(snap.entity_id) {
            // Interpolate instead of snap
            pos.x = lerp(pos.x, snap.position[0], INTERPOLATION_FACTOR);
            pos.y = lerp(pos.y, snap.position[1], INTERPOLATION_FACTOR);
            pos.z = lerp(pos.z, snap.position[2], INTERPOLATION_FACTOR);
        }
    }
}
```

### 3. Batch Messages

```rust,ignore
// Bad - many small packets
for entity in entities {
    send(NetworkPayload::EntityUpdate { entity, ... });
}

// Better - one batched packet
send(NetworkPayload::StateSnapshot { entities, ... });
```

## Debugging

### Network Visualization

```rust,ignore
// Add debug overlay
fn debug_network(world: &World) {
    let client = world.resource::<NetworkClient>().unwrap();
    let metrics = client.metrics();

    egui::Window::new("Network").show(ctx, |ui| {
        ui.label(format!("RTT: {:.1}ms", metrics.rtt_ms));
        ui.label(format!("Packet Loss: {:.1}%", metrics.packet_loss * 100.0));
        ui.label(format!("In: {} bps", metrics.bytes_received));
        ui.label(format!("Out: {} bps", metrics.bytes_sent));
    });
}
```

## Next Steps

- [Network Protocol](../networking/protocol.md)
- [Rollback Netcode](../networking/rollback.md)
