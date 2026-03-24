# Getting Started with Quasar Engine

Quasar is a data-driven game engine built in Rust, featuring an Entity Component System (ECS), modern rendering with wgpu, and cross-platform support.

## Prerequisites

- **Rust 1.75+** - Install via [rustup](https://rustup.rs/)
- **Platform dependencies**:
  - **Windows**: Visual Studio Build Tools with C++ workload
  - **macOS**: Xcode Command Line Tools (`xcode-select --install`)
  - **Linux**: `build-essential`, `pkg-config`, `libx11-dev`, `libasound2-dev`

## Quick Start

### 1. Clone and Build

```bash
git clone https://github.com/dev25bansal-ops/quasar.git
cd quasar
cargo build --release
```

### 2. Run an Example

```bash
# Spinning cube - minimal rendering example
cargo run -p spinning_cube --release

# Physics sandbox - physics and collision
cargo run -p physics_sandbox --release

# Audio demo - spatial audio
cargo run -p audio_demo --release

# Scripting demo - Lua integration
cargo run -p scripting_demo --release
```

### 3. Create Your First Game

Create a new Rust project and add Quasar as a dependency:

```toml
# Cargo.toml
[dependencies]
quasar-engine = { path = "../quasar/crates/quasar-engine" }
```

```rust,ignore
// src/main.rs
use quasar_engine::prelude::*;

fn main() -> QuasarResult<()> {
    App::new()
        .add_plugin(RenderPlugin::default())
        .add_plugin(PhysicsPlugin::default())
        .add_system(SystemStage::Update, move_player)
        .run()
}

fn move_player(world: &mut World) {
    for (_, (pos, vel)) in world.query_iter_2::<Position, Velocity>() {
        // Update position based on velocity
    }
}
```

## Project Structure

```
quasar/
├── crates/
│   ├── quasar-core/      # ECS, events, assets, networking
│   ├── quasar-render/    # Rendering pipeline, materials
│   ├── quasar-physics/   # Physics simulation (Rapier3D)
│   ├── quasar-audio/     # Spatial audio (Kira)
│   ├── quasar-scripting/ # Lua scripting (mlua)
│   ├── quasar-window/    # Window management (winit)
│   ├── quasar-ui/        # UI framework (egui)
│   └── quasar-engine/    # Main engine orchestration
├── examples/
│   ├── spinning_cube/    # Minimal rendering
│   ├── physics_sandbox/  # Physics playground
│   ├── audio_demo/       # Audio examples
│   └── scripting_demo/   # Lua scripting
└── docs/                 # Documentation
```

## Core Concepts

### Entities and Components

Quasar uses an ECS architecture. Entities are simple IDs, and components are data:

```rust,ignore
#[derive(Component, Clone)]
struct Position { x: f32, y: f32, z: f32 }

#[derive(Component, Clone)]
struct Velocity { dx: f32, dy: f32, dz: f32 }

// Spawn an entity
let entity = world.spawn();
world.insert(entity, Position { x: 0.0, y: 0.0, z: 0.0 });
world.insert(entity, Velocity { dx: 1.0, dy: 0.0, dz: 0.0 });
```

### Systems

Systems are functions that operate on the world:

```rust,ignore
fn physics_system(world: &mut World) {
    for (_, (pos, vel)) in world.query_iter_2::<Position, Velocity>() {
        pos.x += vel.dx;
        pos.y += vel.dy;
        pos.z += vel.dz;
    }
}
```

### Plugins

Plugins organize related systems and resources:

```rust,ignore
struct MyGamePlugin;

impl Plugin for MyGamePlugin {
    fn name(&self) -> &str { "my_game" }

    fn build(&self, app: &mut App) {
        app.world.insert_resource(GameState::default());
        app.schedule.add_system(SystemStage::Update, game_logic);
    }
}
```

## Features

| Feature    | Status | Description                     |
| ---------- | ------ | ------------------------------- |
| ECS        | Stable | Archetype-based with queries    |
| Rendering  | Stable | PBR, shadows, post-processing   |
| Physics    | Stable | Rapier3D integration            |
| Audio      | Stable | Spatial audio, DSP effects      |
| Networking | Beta   | QUIC/UDP, rollback netcode      |
| Scripting  | Beta   | Lua with sandboxing             |
| Mobile     | Alpha  | Android/iOS support             |
| Editor     | Alpha  | Entity inspector, asset browser |

## Configuration

### Engine Config (`quasar.toml`)

```toml
[engine]
name = "My Game"
version = "0.1.0"

[window]
title = "My Game"
width = 1920
height = 1080
vsync = true

[rendering]
msaa = 4
shadow_quality = "high"
post_processing = true

[physics]
gravity = [0.0, -9.81, 0.0]
timestep = 1.0 / 60.0
```

## Platform-Specific Notes

### Windows

- Requires Visual Studio 2019+ with C++ tools
- DirectX 12 backend available

### macOS

- Metal backend is default
- Requires macOS 10.15+

### Linux

- Vulkan backend recommended
- Install: `sudo apt install build-essential pkg-config libx11-dev libasound2-dev`

### Web (WASM)

```bash
rustup target add wasm32-unknown-unknown
cargo build -p web-demo --target wasm32-unknown-unknown --release
```

## Next Steps

- [Architecture Overview](architecture.md) - Understand the engine design
- [ECS Documentation](ecs/entities.md) - Deep dive into the ECS
- [Rendering](rendering/render-graph.md) - Learn about the render graph
- [Networking](networking/protocol.md) - Multiplayer networking
- [Examples](examples/multiplayer.md) - Sample projects

## Getting Help

- **GitHub Issues**: [quasar/issues](https://github.com/dev25bansal-ops/quasar/issues)
- **Documentation**: [docs/](./)
- **Examples**: [examples/](../examples/)
