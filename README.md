# 🚀 Quasar Engine

**A modular, data-driven 3D game engine written entirely in Rust.**

Built for [FOSS Hack 2026](https://fossunited.org/fosshack/2026) — the month-long open source hackathon by FOSS United.

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.75%2B-orange.svg)](https://www.rust-lang.org/)
[![Tests](https://img.shields.io/badge/tests-73%20passing-brightgreen.svg)](#testing)

---

## Highlights

- **Custom ECS** — generational entity IDs, typed component storage, system scheduling
- **GPU Rendering** — wgpu-powered forward renderer with depth buffering and directional lighting
- **GPU Instancing** — batched rendering for identical meshes with significant performance gains
- **Scene Graph** — parent-child entity hierarchies with named entities and traversal
- **Rigid Body Physics** — full Rapier3D integration (bodies, colliders, forces, raycasting)
- **Collision Events** — real-time collision detection piped through the ECS event bus
- **Audio Playback** — Kira-backed audio with play/pause/stop/volume/looping and spatial audio
- **Lua Scripting** — embedded Lua 5.4 VM with hot-reload and ECS bridge functions
- **Editor UI** — egui-based scene editor with hierarchy, inspector, and console panels
- **Animation System** — keyframe-based animation for transforms with interpolation
- **Shadow Mapping** — real-time shadows from directional lights
- **Async Asset Loading** — background loading with Pending/Ready/Failed states
- **PBR-lite Materials** — base color, roughness, metallic, emissive properties
- **Texture Loading** — PNG/JPEG loading with GPU upload and bind groups
- **Mesh Primitives** — built-in cube, sphere (UV), cylinder, and plane generators
- **Cross-Platform CI** — automated testing on Windows, macOS, and Linux
- **WASM/Web Support** — WebGPU target for browser deployment

## Features

| Module | Description | Status |
|--------|-------------|--------|
| **quasar-core** | ECS, app lifecycle, events, time, scene graph, animation | ✅ Complete |
| **quasar-math** | Transforms, colors, glam re-exports | ✅ Complete |
| **quasar-render** | wgpu renderer, camera, mesh, textures, materials, shadows | ✅ Complete |
| **quasar-window** | Window management & input via winit | ✅ Complete |
| **quasar-physics** | Rigid bodies, colliders, forces, raycasting, collision events | ✅ Complete |
| **quasar-audio** | Audio playback with controls and spatial audio (Kira) | ✅ Complete |
| **quasar-scripting** | Lua 5.4 VM with hot-reload & ECS bridge (mlua) | ✅ Complete |
| **quasar-editor** | Scene hierarchy, inspector, console (egui) | ✅ Complete |
| **quasar-engine** | Meta-crate combining all modules | ✅ Complete |

## Architecture

```
quasar-engine (meta-crate / prelude)
├── quasar-core    # ECS, App, Events, Time, Plugins, Scene Graph, Animation
├── quasar-math    # Vec3, Mat4, Quat, Transform, Color
├── quasar-render  # wgpu renderer, camera, mesh, texture, material, shaders, shadows
├── quasar-window  # winit window, keyboard/mouse input
├── quasar-physics # Rapier3D rigid bodies, colliders, forces, raycasting, collision events
├── quasar-audio   # Kira audio playback, controls, spatial audio
├── quasar-scripting # Lua 5.4 VM, hot-reload, ECS bridge
└── quasar-editor  # egui hierarchy, inspector, console panels
```

## Screenshots

<div align="center">
<img src="assets/screenshots/showcase.png" alt="Quasar Engine Showcase Demo" width="800">

<p>
<em>Multi-shape scene with orbiting objects, animations, and camera orbit controls</em>
</p>
</div>

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
| **physics_sandbox** | Rigid body dynamics with collision events | `cargo run -p physics-sandbox` |
| **audio_demo** | Sound effects and spatial audio demo | `cargo run -p audio-demo` |
| **scripting_demo** | Lua scripting with hot-reload | `cargo run -p scripting-demo` |
| **web_demo** | WebGPU browser demo | See [examples/web_demo/README.md](examples/web_demo/README.md) |

## Testing

73 tests across 11 crates, all passing:

```bash
cargo test --workspace
```

| Crate | Tests |
|-------|-------|
| quasar-core | 29 (ECS + scene graph + animation + assets) |
| quasar-math | 13 (transform + color) |
| quasar-render | 11 (camera + culling + shadow + loader) |
| quasar-physics | 7 (bodies, colliders, raycasting, events) |
| quasar-scripting | 7 (Lua bridge functions) |
| quasar-editor | 2 (console panel) |
| quasar-window | 4 (action map + input) |
| doc-tests | Various |

## CI/CD

Automated CI pipeline runs on every push and pull request:

- **Check** — Compiles on Ubuntu, Windows, and macOS
- **Test** — All 73 tests pass
- **Format** — `cargo fmt --check` on all platforms
- **Clippy** — Linting with `-D warnings` on all platforms
- **Docs** — Documentation builds without warnings

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
| **Editor GUI** | [egui](https://www.egui.rs/) | 0.31 |
| **GPU Casting** | [bytemuck](https://docs.rs/bytemuck) | 1 |
| **Images** | [image](https://docs.rs/image) | 0.25 |
| **WASM** | [wasm-bindgen](https://rustwasm.github.io/wasm-bindgen/) | 0.2 |

## Project Structure

```
quasar/
├── crates/
│   ├── quasar-core/      # ECS framework + scene graph + animation
│   ├── quasar-math/      # Math types (Transform, Color)
│   ├── quasar-render/    # GPU renderer + textures + materials + shadows
│   ├── quasar-window/    # Window & input management
│   ├── quasar-physics/   # Physics simulation + collision events
│   ├── quasar-audio/     # Audio playback + spatial audio
│   ├── quasar-scripting/ # Lua scripting engine
│   ├── quasar-editor/    # Scene editor UI
│   └── quasar-engine/    # Meta-crate (prelude)
├── examples/
│   ├── showcase/         # Multi-feature demo
│   ├── spinning_cube/    # Basic cube demo
│   ├── physics_sandbox/  # Physics demo
│   ├── audio_demo/       # Audio demo
│   ├── scripting_demo/   # Lua scripting demo
│   └── web_demo/         # WebGPU browser demo
├── assets/
│   └── shaders/          # WGSL shaders (basic.wgsl, shadow.wgsl)
├── .github/
│   └── workflows/
│       └── ci.yml        # Multi-platform CI
├── Cargo.toml            # Workspace definition
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
