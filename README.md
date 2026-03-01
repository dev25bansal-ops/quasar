# 🚀 Quasar Engine

**A modular, data-driven 3D game engine written entirely in Rust.**

Built for [FOSS Hack 2026](https://fossunited.org/fosshack/2026) — the month-long open source hackathon by FOSS United.

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.75%2B-orange.svg)](https://www.rust-lang.org/)
[![Tests](https://img.shields.io/badge/tests-34%20passing-brightgreen.svg)](#testing)

---

## Highlights

- **Custom ECS** — generational entity IDs, typed component storage, system scheduling
- **GPU Rendering** — wgpu-powered forward renderer with depth buffering and directional lighting
- **Scene Graph** — parent-child entity hierarchies with named entities and traversal
- **Rigid Body Physics** — full Rapier3D integration (bodies, colliders, forces, raycasting)
- **Audio Playback** — Kira-backed audio with play/pause/stop/volume/looping
- **Lua Scripting** — embedded Lua 5.4 VM with hot-reload and ECS bridge functions
- **Editor UI** — egui-based scene editor with hierarchy, inspector, and console panels
- **PBR-lite Materials** — base color, roughness, metallic, emissive properties
- **Texture Loading** — PNG/JPEG loading with GPU upload and bind groups
- **Mesh Primitives** — built-in cube, sphere (UV), cylinder, and plane generators

## Features

| Module | Description | Status |
|--------|-------------|--------|
| **quasar-core** | ECS, app lifecycle, events, time, scene graph | ✅ Complete |
| **quasar-math** | Transforms, colors, glam re-exports | ✅ Complete |
| **quasar-render** | wgpu renderer, camera, mesh, textures, materials | ✅ Complete |
| **quasar-window** | Window management & input via winit | ✅ Complete |
| **quasar-physics** | Rigid bodies, colliders, forces, raycasting (Rapier3D) | ✅ Complete |
| **quasar-audio** | Audio playback with controls (Kira) | ✅ Complete |
| **quasar-scripting** | Lua 5.4 VM with hot-reload & ECS bridge (mlua) | ✅ Complete |
| **quasar-editor** | Scene hierarchy, inspector, console (egui) | ✅ Complete |
| **quasar-engine** | Meta-crate combining all modules | ✅ Complete |

## Architecture

```
quasar-engine (meta-crate / prelude)
├── quasar-core       # ECS, App, Events, Time, Plugins, Scene Graph
├── quasar-math       # Vec3, Mat4, Quat, Transform, Color
├── quasar-render     # wgpu renderer, camera, mesh, texture, material, shaders
├── quasar-window     # winit window, keyboard/mouse input
├── quasar-physics    # Rapier3D rigid bodies, colliders, forces, raycasting
├── quasar-audio      # Kira audio playback & controls
├── quasar-scripting  # Lua 5.4 VM, hot-reload, ECS bridge
└── quasar-editor     # egui hierarchy, inspector, console panels
```

## Quick Start

### Prerequisites

- **Rust 1.75+** (install via [rustup](https://rustup.rs/))
- A GPU with Vulkan, Metal, or DX12 support

### Build & Run

```bash
# Clone the repository
git clone https://github.com/Dev2506/quasar.git
cd quasar

# Run the showcase demo (multiple shapes, scene graph, camera orbit)
cargo run -p showcase

# Run the spinning cube demo
cargo run -p spinning-cube

# Run all tests
cargo test --workspace
```

### Using Quasar in Your Project

```toml
[dependencies]
quasar-engine = { path = "path/to/quasar/crates/quasar-engine" }
```

```rust
use quasar_engine::prelude::*;

fn main() {
    let mut world = World::new();
    let mut scene = SceneGraph::new();

    // Spawn entities with transforms
    let player = world.spawn();
    world.insert(player, Transform::from_position(Vec3::new(0.0, 1.0, 0.0)));
    scene.set_name(player, "Player");

    // Parent-child relationships
    let weapon = world.spawn();
    scene.set_parent(weapon, player);
}
```

## Examples

| Example | Description | Run |
|---------|-------------|-----|
| **showcase** | Multi-shape scene with orbiting objects, animations, camera orbit | `cargo run -p showcase` |
| **spinning_cube** | Classic single-cube with directional lighting | `cargo run -p spinning-cube` |

## Testing

34 tests across 9 crates, all passing:

```bash
cargo test --workspace
```

| Crate | Tests |
|-------|-------|
| quasar-core | 8 (ECS + scene graph) |
| quasar-math | 13 (transform + color) |
| quasar-physics | 6 (bodies, colliders, raycasting) |
| quasar-scripting | 3 (Lua bridge functions) |
| quasar-editor | 2 (console panel) |
| doc-tests | 2 |

## Tech Stack

| Category | Library | Version |
|----------|---------|---------|
| **Language** | [Rust](https://www.rust-lang.org/) 🦀 | Edition 2021 |
| **GPU** | [wgpu](https://wgpu.rs/) | 24 |
| **Windowing** | [winit](https://docs.rs/winit) | 0.30 |
| **Math** | [glam](https://docs.rs/glam) | 0.29 |
| **Physics** | [Rapier3D](https://rapier.rs/) | 0.22 |
| **Audio** | [Kira](https://docs.rs/kira) | 0.9 |
| **Scripting** | [mlua](https://docs.rs/mlua) (Lua 5.4) | 0.10 |
| **Editor GUI** | [egui](https://www.egui.rs/) | 0.30 |
| **GPU Casting** | [bytemuck](https://docs.rs/bytemuck) | 1 |
| **Images** | [image](https://docs.rs/image) | 0.25 |

## Project Structure

```
quasar/
├── crates/
│   ├── quasar-core/       # ECS framework + scene graph
│   ├── quasar-math/       # Math types (Transform, Color)
│   ├── quasar-render/     # GPU renderer + textures + materials
│   ├── quasar-window/     # Window & input management
│   ├── quasar-physics/    # Physics simulation
│   ├── quasar-audio/      # Audio playback
│   ├── quasar-scripting/  # Lua scripting engine
│   ├── quasar-editor/     # Scene editor UI
│   └── quasar-engine/     # Meta-crate (prelude)
├── examples/
│   ├── showcase/          # Multi-feature demo
│   └── spinning_cube/     # Basic cube demo
├── Cargo.toml             # Workspace definition
├── LICENSE
└── README.md
```

## Contributing

Contributions welcome! This project is licensed under MIT.

1. Fork the repository
2. Create a feature branch (`git checkout -b feat/my-feature`)
3. Commit your changes (`git commit -m 'feat: add my feature'`)
4. Push to the branch (`git push origin feat/my-feature`)
5. Open a Pull Request

## License

Licensed under the [MIT License](LICENSE).

---

*Built with ❤️ for [FOSS Hack 2026](https://fossunited.org/fosshack/2026)*
