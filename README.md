# 🚀 Quasar Engine

**A modular 3D game engine written in Rust.**

Built for [FOSS Hack 2026](https://fossunited.org/fosshack/2026) — the month-long open source hackathon by FOSS United.

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%2FApache--2.0-blue.svg)](LICENSE-MIT)

---

## Features

| Module | Description | Status |
|--------|-------------|--------|
| **quasar-core** | ECS framework, app lifecycle, events, time | ✅ Core |
| **quasar-math** | Transforms, colors, glam re-exports | ✅ Core |
| **quasar-render** | 3D rendering via wgpu (forward pipeline) | ✅ Core |
| **quasar-window** | Window management & input via winit | ✅ Core |
| **quasar-physics** | Rigid body physics via Rapier3D | 🔧 In Progress |
| **quasar-audio** | Spatial audio via Kira | 🔧 In Progress |
| **quasar-scripting** | Lua scripting via mlua | 📋 Planned |
| **quasar-editor** | Visual scene editor via egui | 📋 Planned |

## Architecture

```
quasar-engine (meta-crate)
├── quasar-core       # ECS, App, Events, Time, Plugins
├── quasar-math       # Vec3, Mat4, Quat, Transform, Color
├── quasar-render     # wgpu renderer, camera, mesh, shaders
├── quasar-window     # winit window, keyboard/mouse input
├── quasar-physics    # Rapier3D rigid bodies & colliders
├── quasar-audio      # Kira audio playback
├── quasar-scripting  # Lua VM & script execution
└── quasar-editor     # egui-based scene editor
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

# Run the spinning cube demo
cargo run -p spinning-cube

# Run all tests
cargo test --workspace
```

### Using Quasar in Your Game

Add to your `Cargo.toml`:

```toml
[dependencies]
quasar-engine = { path = "path/to/quasar/crates/quasar-engine" }
```

```rust
use quasar_engine::prelude::*;

fn main() {
    // Your game starts here!
    let mut world = World::new();
    let player = world.spawn();
    world.insert(player, Transform::from_position(Vec3::new(0.0, 1.0, 0.0)));
}
```

## Examples

| Example | Description |
|---------|-------------|
| `spinning_cube` | A colored cube rotating with directional lighting |

Run any example:
```bash
cargo run -p spinning-cube
```

## Development Roadmap

### Week 1 (March 1–7): Foundation
- [x] Project scaffolding & workspace setup
- [x] ECS core (Entity, Component, World, System, Schedule)
- [x] wgpu 3D renderer with depth buffer
- [x] Perspective camera
- [x] Basic WGSL shader with lighting
- [x] Spinning cube demo

### Week 2 (March 8–14): Core Systems
- [ ] Physics integration (Rapier3D)
- [ ] Audio system (Kira)
- [ ] Asset loading (meshes, textures, models)
- [ ] Scene graph & parent-child transforms
- [ ] Multiple mesh rendering

### Week 3 (March 15–21): Advanced Features
- [ ] Lua scripting integration
- [ ] Material & multi-shader support
- [ ] Point/spot lighting
- [ ] Editor foundation (egui overlay)
- [ ] Camera controller (orbit, FPS)

### Week 4 (March 22–31): Polish
- [ ] Scene editor (hierarchy, inspector)
- [ ] Demo game / showcase scene
- [ ] Documentation & API docs
- [ ] Performance profiling
- [ ] Video demo

## Tech Stack

- **Language**: Rust 🦀
- **GPU**: [wgpu](https://wgpu.rs/) (Vulkan/Metal/DX12/WebGPU)
- **Windowing**: [winit](https://docs.rs/winit)
- **Math**: [glam](https://docs.rs/glam)
- **Physics**: [Rapier3D](https://rapier.rs/)
- **Audio**: [Kira](https://docs.rs/kira)
- **Scripting**: [mlua](https://docs.rs/mlua) (Lua 5.4)
- **Editor GUI**: [egui](https://www.egui.rs/)

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.

---

*Built with ❤️ for FOSS Hack 2026*
