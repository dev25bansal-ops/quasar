# Quasar Engine Architecture

Quasar is a modular, data-driven game engine built around an Entity Component System (ECS) architecture. This document provides an overview of the engine's design and how the various systems interact.

## High-Level Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        Game Application                       │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐         │
│  │ Plugins │  │ Systems │  │ Resources│ │  Events │         │
│  └────┬────┘  └────┬────┘  └────┬────┘  └────┬────┘         │
│       │            │            │            │               │
│       └────────────┴────────────┴────────────┘               │
│                           │                                   │
│                    ┌──────┴──────┐                           │
│                    │    World    │  ECS Core                 │
│                    │  (Entities, │                           │
│                    │ Components) │                           │
│                    └─────────────┘                           │
└─────────────────────────────────────────────────────────────┘
         │              │              │              │
    ┌────┴────┐    ┌────┴────┐    ┌────┴────┐    ┌────┴────┐
    │ Render  │    │ Physics │    │  Audio  │    │ Network │
    │  (wgpu) │    │ (Rapier)│    │ (Kira)  │    │(QUIC/UDP)│
    └─────────┘    └─────────┘    └─────────┘    └─────────┘
         │              │              │              │
    ┌────┴──────────────┴──────────────┴──────────────┴────┐
    │                    Platform Layer                     │
    │   Windows │ macOS │ Linux │ Android │ iOS │ Web     │
    └───────────────────────────────────────────────────────┘
```

## Core Crates

### quasar-core

The heart of the engine, providing:

- **ECS**: Entity Component System with archetype storage
- **Events**: Type-safe event bus for decoupled communication
- **Assets**: Content-addressed asset management with hot-reload
- **Scene**: Scene graph and serialization
- **Navigation**: NavMesh pathfinding

### quasar-render

Modern rendering pipeline built on wgpu:

- **Render Graph**: Declarative pass composition
- **PBR Materials**: Physically-based rendering
- **Post-Processing**: Bloom, tonemapping, FXAA, TAA
- **Shadows**: Cascaded shadow maps

### quasar-physics

Physics simulation via Rapier3D:

- **Rigid Bodies**: Dynamic, static, kinematic
- **Colliders**: Box, sphere, capsule, mesh
- **Joints**: Constraints between bodies
- **Raycasting**: Spatial queries

### quasar-audio

Spatial audio system using Kira:

- **Sound Effects**: One-shot and looping sounds
- **Music**: Streaming audio
- **DSP Effects**: EQ, compressor, reverb, limiter
- **Audio Graph**: Effect chains per bus

### quasar-scripting

Lua scripting integration via mlua:

- **Sandboxed Execution**: Secure by default
- **Hot-Reload**: Edit scripts at runtime
- **Engine Bindings**: Access to ECS, math, input

### quasar-network

Multiplayer networking:

- **Transport**: QUIC and UDP backends
- **Replication**: Entity state synchronization
- **Rollback**: Input prediction and correction
- **Lag Compensation**: Server-side rewinding

## ECS Architecture

### Storage Model

Quasar uses **archetype-based storage**, similar to Bevy and Unity DOTS:

```
Archetype A: [Position, Velocity]
├── Entity 1: Position(1, 0, 0), Velocity(0.1, 0, 0)
├── Entity 2: Position(5, 2, 3), Velocity(0, 0.5, 0)
└── Entity 3: Position(0, 0, 0), Velocity(1, 1, 1)

Archetype B: [Position, Velocity, Health]
├── Entity 4: Position(10, 0, 0), Velocity(0, 0, 0), Health(100)
└── Entity 5: Position(20, 5, 10), Velocity(0, 0, 0), Health(50)
```

When a component is added/removed, entities migrate between archetypes.

### Query Execution

```rust,ignore
// Zero-allocation query iteration
for (entity, pos) in world.query_iter::<Position>() {
    // Process position
}

// Multiple components
for (entity, (pos, vel)) in world.query_iter_2::<Position, Velocity>() {
    pos.x += vel.dx;
}
```

### System Scheduling

Systems run in stages:

1. **PreUpdate**: Input processing, asset reloads
2. **Update**: Game logic
3. **PostUpdate**: Transform propagation, cleanup
4. **Render**: Drawing (separate thread)

## Rendering Pipeline

### Render Graph

The render graph manages GPU resources and pass execution:

```
┌────────────┐     ┌────────────┐     ┌────────────┐
│ ShadowPass │────▶│ GBufferPass│────▶│ LightPass  │
└────────────┘     └────────────┘     └────────────┘
                                             │
                                             ▼
┌────────────┐     ┌────────────┐     ┌────────────┐
│  UIPass    │◀────│ PostProcess│◀────│  SSGIPass  │
└────────────┘     └────────────┘     └────────────┘
```

### Frame Flow

1. **Shadow Pass**: Render depth from light views
2. **G-Buffer Pass**: Render albedo, normal, depth
3. **Lighting Pass**: Deferred shading
4. **SSGI Pass**: Screen-space global illumination
5. **Post-Process**: Bloom, tonemap, FXAA
6. **UI Pass**: egui rendering

## Asset Pipeline

### Content-Addressed Storage

Assets are stored by content hash (blake3):

```rust,ignore
pub struct AssetDatabase {
    cache: HashMap<ContentHash, Asset>,
    loader: AssetLoader,
}

impl AssetDatabase {
    pub fn load(&mut self, path: &Path) -> ContentHash {
        let hash = blake3::hash(&fs::read(path));
        self.cache.entry(hash).or_insert_with(|| {
            self.loader.load(path)
        })
    }
}
```

### Hot Reload

The asset server watches for file changes:

```rust,ignore
// In your system
for event in asset_server.poll_events() {
    match event {
        AssetEvent::Reloaded { path, .. } => {
            // Asset was modified, refresh GPU resources
        }
        _ => {}
    }
}
```

## Networking Model

### Client-Server Architecture

```
┌──────────────┐         ┌──────────────┐
│   Client A   │◀───────▶│              │
├──────────────┤         │    Server    │
│   Client B   │◀───────▶│              │
├──────────────┤         │ (Authoritative)│
│   Client C   │◀───────▶│              │
└──────────────┘         └──────────────┘
```

### State Synchronization

- **Snapshots**: Full entity state sent periodically
- **Deltas**: Changed components only
- **Interpolation**: Smooth movement between updates

### Rollback Netcode

```
Client Input Timeline:
Frame:  1   2   3   4   5   6   7
Input:  A   B   C   D   E   F   G
        │   │   │   │   │
        └───┴───┴───┴───┴──▶ Predicted frames

Server Confirmation:
Frame:  1   2   3   4   5
State:  ✓   ✓   ✓   ?   ?
                    └──▶ Rollback if mismatch
```

## Plugin System

### Creating a Plugin

```rust,ignore
pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn name(&self) -> &str { "physics" }

    fn build(&self, app: &mut App) {
        // Insert resources
        app.world.insert_resource(PhysicsWorld::new());

        // Add systems
        app.schedule.add_system(SystemStage::Update, physics_step);
        app.schedule.add_system(SystemStage::PostUpdate, sync_transforms);
    }

    fn dependencies(&self) -> &[&'static str] {
        &["transform"]
    }
}
```

### Plugin Lifecycle

1. **Registration**: `App::add_plugin()`
2. **Dependency Resolution**: Plugins load in order
3. **Build Phase**: Resources and systems added
4. **Runtime**: Systems execute in schedule

## Memory Management

### Pools and Arenas

- **Uniform Ring Buffer**: Reused GPU uniform data
- **Message Pool**: Network message buffer pool
- **Archetype Arenas**: Contiguous component storage

### Frame Budget

```rust,ignore
pub struct FrameBudget {
    pub target_ms: f32,      // 16.67 for 60 FPS
    pub update_ms: f32,      // Time in update
    pub render_ms: f32,      // Time in render
    pub budget_remaining: f32,
}
```

## Threading Model

### Main Thread

- ECS updates
- Game logic
- Input handling

### Render Thread

- GPU command recording
- Resource management
- Present

### Worker Threads (Rayon)

- Parallel physics
- Asset loading
- Message deserialization

## Next Steps

- [ECS Entities](ecs/entities.md)
- [Render Graph](rendering/render-graph.md)
- [Network Protocol](networking/protocol.md)
