# 🚀 Quasar Engine

**A modular, data-driven 3D game engine written entirely in Rust.**

Built for [FOSS Hack 2026](https://fossunited.org/fosshack/2026) — the month-long open source hackathon by FOSS United.

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.75%2B-orange.svg)](https://www.rust-lang.org/)
[![Tests](https://img.shields.io/badge/tests-73%20passing-brightgreen.svg)](#testing)

---

## Highlights

- **Archetype ECS** — SoA archetype storage with generational entity handles, typed queries with filters (`With`, `Without`, `Changed`, `Added`, `Removed`), deferred command buffers, sparse-set side storage, and entity relations (`ChildOf`, `OwnedBy` with cascade despawn)
- **Parallel System Scheduling** — explicit read/write access declarations, automatic conflict detection, topological grouping, and rayon-based parallel execution
- **Render Graph** — node-based render pipeline with 14 feature flags: deferred shading (100+ lights), clustered forward (16×9×24 froxels), PBR Cook-Torrance BRDF, IBL environment maps, and cascade + virtual shadow maps
- **Screen-Space Effects** — SSGI (ray-marched), SSR (hierarchical march), SSAO (hemisphere sampling), TAA (Halton jitter + YCoCg clamping), FXAA, and bloom
- **GPU-Driven Rendering** — Hi-Z occlusion culling (8-level depth pyramid), meshlet clustering (64 verts / 126 tris with cone culling), and compute-based indirect draw
- **Virtual Shadow Maps** — clipmap pages with LRU cache (128×128 pages, 6 levels)
- **Sparse Virtual Textures** — 128×128 tile streaming with GPU feedback pass, page table, and background tile loading
- **Streaming & LOD** — budget-based streaming pool (512 MB texture / 256 MB mesh) with LRU eviction, distance-based LOD selection, and 4×4 Bayer dithered cross-fade transitions
- **Rigid Body Physics** — full Rapier3D integration (bodies, colliders, joints with motors, character controller, sensors/triggers, raycasting, shape casting) with async physics stepping on a dedicated thread and interpolation
- **Deterministic Rollback** — frame-snapshotted physics state, per-client input history ring buffer, misprediction detection, and re-simulation for netcode
- **Spatial Audio** — Kira-backed playback with 6-bus mixer, parametric EQ / compressor / limiter / reverb DSP chain, OpenAL-style inverse-distance attenuation, Doppler tracking, and reverb zones
- **Ambisonics & GPU Reverb** — orders 1–3 spherical harmonic encoding/decoding (ACN/SN3D, up to 16 B-format channels) and GPU-accelerated partitioned convolution reverb (1024-sample overlap-add via compute shader)
- **QUIC Networking** — Quinn-based QUIC transport with unreliable/reliable/bulk channels, per-client delta compression (64-slot bitmask diffing against acknowledged baselines), spatial interest management (grid-based AoI), and client-side prediction with server reconciliation
- **Dual Scripting** — Lua 5.4 VM (mlua) with hot-reload file watcher, ECS bridge (`quasar._transforms`, force/spawn/despawn commands, component registry), and sandboxed WASM scripting via wasmtime with host API
- **Scene Editor** — egui-based editor (F12 toggle) with hierarchy panel (multi-select), component inspector (auto-generated via `#[derive(Inspect)]`), console log (512-entry ring buffer), asset browser (grid view with kind detection), gizmos (translate/rotate/scale with axis raycasting), play-in-editor with world snapshot/restore, undo/redo (100-deep command stack), and prefab override diffing
- **Visual Graph Editors** — shader graph editor (node-based WGSL generation) and logic graph editor (visual Lua code generation with data + execution flow connections)
- **GPU Profiler** — per-pass timestamp queries (64 passes max) with async readback, wired into the editor overlay alongside CPU frame stats (average, median, min, max, FPS)
- **Animation** — keyframe-based transform animation with linear interpolation and quaternion SLERP, skeletal animation clips (per-bone keyframes), animation state machine, and timeline editor panel with Step/Linear/CubicSpline interpolation modes
- **Build Pipeline** — CLI tool targeting 6 platforms (Windows, Linux, macOS, Web, Android, iOS) with parallel asset processing (rayon), BC7 texture compression (intel_tex_2), ASTC 4×4 for mobile, glTF mesh optimization (meshopt vertex cache + overdraw + fetch), SHA-256 integrity verification, and content-addressable caching
- **Navigation** — A* pathfinding on polygon nav meshes with heightmap generation (slope filtering), centroid-based waypoints, and `NavMeshAgent` component
- **Reflection** — `#[derive(Reflect)]` proc macro generating field descriptors, JSON serialization, and compact binary network serialization (little-endian)
- **Save/Load** — full world snapshot to JSON with entity transforms, names, hierarchy, and custom data; bidirectional scene ↔ save conversion
- **In-Game UI** — retained-mode flexbox layout solver with text (fontdue), image, and widget rendering (button, checkbox, slider, progress bar, text input), batched to 4096 GPU quads
- **Mobile** — Android/iOS touch input, gesture recognition (tap, swipe, pinch, rotate), gyroscope/accelerometer/magnetometer sensor abstraction, and haptic feedback engine
- **Hot Reload** — file-watcher-based live reload of shaders (WGSL), textures (PNG/JPEG), Lua scripts, scenes, prefabs, and audio assets with dirty-flag propagation
- **Cross-Platform** — Vulkan/Metal/DX12 via wgpu, WebGPU for browsers, multi-platform CI on Windows/macOS/Linux

## Features

| Module | Description | Status |
|--------|-------------|--------|
| **quasar-core** | Archetype ECS (SoA storage, generational handles, typed queries, filters, change detection, sparse sets, relations, deferred commands), parallel system scheduling (rayon), events, scene graph, animation (keyframe + skeletal + state machine), asset server (hot-reload, loaders), A* navigation (polygon nav mesh, heightmap gen), QUIC networking (delta compression, interest management, prediction, rollback), reflection, prefabs (override diffing), save/load | ✅ Complete |
| **quasar-math** | Transform (TRS, look_at, local axes), Color (linear f32, presets, u8 conversion), glam re-exports (Vec2–4, Quat, Mat3–4, Affine3A) | ✅ Complete |
| **quasar-render** | Render graph with 14 feature flags, forward + deferred + hybrid paths, PBR (Cook-Torrance BRDF, IBL), clustered lighting (16×9×24 froxels), cascade + virtual shadow maps, SSGI, SSR, SSAO, TAA, FXAA, bloom, tonemap, Hi-Z occlusion culling, meshlets (64v/126t, cone culling), SVT (128px tiles, page table), streaming pool (LRU, budgeted), LOD (Bayer dithered cross-fade), terrain (heightmap + splatmap), 100K GPU particles, volumetric fog, lightmap baking (CPU + GPU path tracer), reflection probes (parallax-corrected cubemaps), decals, sprites, skinned meshes, hot-reload, GPU profiler | ✅ Complete |
| **quasar-window** | Window config (title, resolution, vsync), per-frame keyboard/mouse input state, action map binding system with ActionEvents | ✅ Complete |
| **quasar-physics** | Rapier3D: rigid bodies (dynamic/kinematic/fixed), colliders (sphere/box/capsule/mesh/heightfield), joints (fixed/prismatic/revolute/spherical + motors), character controller (auto-step), sensors/triggers, raycasting, shape casting, collision events (start/stop), async stepping (dedicated thread + interpolation), rollback snapshots | ✅ Complete |
| **quasar-audio** | Kira playback (one-shot, looped, streaming), 6-bus mixer, DSP chain (parametric EQ, compressor, limiter, reverb), spatial audio (inverse-distance attenuation), Doppler tracking, reverb zones (AABB), ambisonics encoding/decoding (orders 1–3, ACN/SN3D), GPU convolution reverb (1024-sample partitioned overlap-add compute shader) | ✅ Complete |
| **quasar-scripting** | Lua 5.4 (mlua) with file-watcher hot-reload, ECS bridge (transform read/write, force/spawn/despawn commands), component registry (string → serialize/insert/update/remove), WASM scripting (wasmtime, host API: get/set transform, spawn, log) | ✅ Complete |
| **quasar-editor** | egui panels: hierarchy (multi-select tree), inspector (`#[derive(Inspect)]` reflection), console (512-entry ring buffer), asset browser (grid + kind detection), gizmos (translate/rotate/scale + axis raycasting), shader graph editor (WGSL gen), logic graph editor (Lua gen with data + exec flow), timeline (keyframe scrubber), GPU profiler overlay, play-in-editor (snapshot/restore), undo/redo (100 commands), prefab override diffing | ✅ Complete |
| **quasar-build** | CLI build tool (6 targets: Windows/Linux/macOS/Web/Android/iOS), parallel asset processing (rayon), BC7 compression (intel_tex_2), ASTC 4×4 (mobile), glTF mesh optimization (meshopt: vertex cache + overdraw + fetch), SHA-256 integrity, content-addressable caching | ✅ Complete |
| **quasar-derive** | `#[derive(Inspect)]` proc macro: type-directed widget generation (DragValue, Checkbox, TextEdit, Vec3 sliders, Color4 picker), `#[inspect(skip)]` attribute | ✅ Complete |
| **quasar-ui** | Retained-mode UI: flexbox layout solver, anchor positioning, widgets (button, checkbox, slider, progress bar, text input), text rendering (fontdue), batched GPU quads (4096 max), alpha blending | ✅ Complete |
| **quasar-mobile** | Touch input (multi-pointer with pressure), gesture recognition (tap, swipe, pinch, rotate), sensor abstraction (gyroscope, accelerometer, magnetometer), haptic feedback engine | ✅ Complete |
| **quasar-engine** | Meta-crate with prelude re-exporting all subsystems, winit game loop runner with HDR + tonemap pipeline | ✅ Complete |

## Architecture

```
quasar-engine (meta-crate / prelude / game loop runner)
│
├── quasar-core         # ECS (archetype SoA + sparse sets + relations + deferred commands)
│   ├── ecs/            #   Entity allocator, archetype graph, typed queries + filters,
│   │                   #   parallel schedule (rayon), change detection, command buffers
│   ├── animation       #   Keyframe + skeletal clips, state machine, AnimationPlayer
│   ├── asset_server    #   Hot-reload file watcher, pluggable loaders, dirty flags
│   ├── navigation      #   Polygon nav mesh, A* pathfinding, heightmap generation
│   ├── network         #   QUIC transport (Quinn), rollback manager, input history
│   ├── delta_compress  #   64-slot bitmask delta encoding, per-client baselines
│   ├── interest        #   Spatial grid AoI, per-client relevancy queries
│   ├── prediction      #   Client prediction + server reconciliation
│   ├── reflect         #   #[derive(Reflect)] → schema + JSON + binary serialization
│   ├── prefab          #   Blueprint instantiation, component override registry
│   ├── save_load       #   World snapshot ↔ JSON, scene interop
│   └── scene           #   Scene graph (parent/child, name lookup, transform propagation)
│
├── quasar-math         # Transform (TRS + local axes), Color (linear f32), glam re-exports
│
├── quasar-render       # wgpu 24 GPU renderer
│   ├── render_graph    #   Node-based pipeline (14 feature flags)
│   ├── pbr / deferred  #   Cook-Torrance BRDF, IBL, G-Buffer, clustered lighting
│   ├── shadow          #   Cascade maps, PCSS, virtual shadow maps (clipmap + LRU)
│   ├── ssgi / ssr      #   Screen-space GI + reflections (hierarchical ray-march)
│   ├── ssao / taa      #   Hemisphere SSAO, temporal AA (Halton jitter + YCoCg)
│   ├── occlusion       #   Hi-Z depth pyramid (8 levels), AABB rejection
│   ├── meshlet         #   64v/126t clusters, frustum + cone culling, indirect draw
│   ├── svt             #   128px tile streaming, page table, feedback pass
│   ├── streaming       #   Budget-based LRU pool (512 MB tex / 256 MB mesh)
│   ├── lod             #   Distance-based selection, Bayer dithered cross-fade
│   ├── particles       #   100K GPU particles (compute sim + instanced draw)
│   ├── volumetric      #   Ray-marched fog (Henyey-Greenstein phase)
│   ├── terrain         #   Heightmap + splatmap, adaptive LOD
│   ├── lightmap        #   CPU baker (ray-cast) + GPU path tracer
│   ├── probes          #   Reflection (cubemap, parallax) + SH irradiance (order 2)
│   ├── gpu_profiler    #   Timestamp queries (64 passes), async readback
│   ├── hot_reload      #   Shader + texture live reload with pipeline invalidation
│   └── post_process    #   FXAA, bloom, HDR tonemap
│
├── quasar-window       # winit window, keyboard/mouse input, action map binding
├── quasar-physics      # Rapier3D (bodies, colliders, joints, character controller,
│                       #   sensors, raycasting), async stepping, rollback snapshots
├── quasar-audio        # Kira (playback, 6-bus mixer, DSP chain), spatial audio,
│                       #   Doppler, reverb zones, ambisonics (1–3), GPU convolution reverb
├── quasar-scripting    # Lua 5.4 (mlua, hot-reload, ECS bridge, component registry)
│                       #   + WASM (wasmtime, sandboxed host API)
├── quasar-editor       # egui editor: hierarchy, inspector, console, asset browser,
│                       #   gizmos, shader graph, logic graph, timeline, GPU profiler,
│                       #   play-in-editor, undo/redo, prefab diffing
├── quasar-ui           # Retained-mode flexbox UI (text, images, widgets, GPU batching)
├── quasar-mobile       # Touch, gestures, sensors, haptics (Android/iOS)
├── quasar-derive       # #[derive(Inspect)] proc macro
└── quasar-build        # CLI asset pipeline: parallel processing, BC7/ASTC compression,
                        #   glTF mesh optimization, content-addressable caching
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
    let mut app = App::new();

    // Register plugins
    app.add_plugin(PhysicsPlugin);
    app.add_plugin(AudioPlugin);
    app.add_plugin(ScriptingPlugin);

    // Add systems (closures or named functions)
    app.add_system("spawn_scene", |world: &mut World| {
        let player = world.spawn();
        world.insert(player, Transform::from_position(Vec3::new(0.0, 1.0, 0.0)));
        world.insert(player, RigidBodyComponent::dynamic());
        world.insert(player, ColliderComponent::capsule(0.5, 1.8));
    });

    // Enable parallel execution with access declarations
    app.enable_parallel();
    app.add_parallel_system(
        SystemStage::Update,
        SystemNode::new(FnSystem::new("movement", movement_system))
            .with_component_access::<(Write<Transform>, Read<Velocity>)>()
    );

    // Run the engine
    run(app);
}
```

<details>
<summary><strong>More examples: queries, physics, audio, scripting</strong></summary>

```rust
// Archetype queries with filters
for (entity, (transform, health)) in world.query::<(Transform, Health)>() {
    if health.current <= 0.0 {
        commands.despawn(entity);
    }
}

// Change detection — only process entities whose Transform changed this frame
let query = QueryState::<(&Transform,), FilterChanged<Transform>>::new();
for (entity, (transform,)) in query.iter(&world) { /* ... */ }

// Physics raycasting
let hit = physics.ray_cast(origin, direction, max_distance);

// Spatial audio
audio.play("explosion.ogg", AudioBus::Sfx);
audio.set_listener_position(camera_pos);

// Lua scripting bridge
script_engine.execute("quasar.apply_force(entity_id, 0, 10, 0)");
```
</details>
```

## Examples

| Example | Description | Run |
|---------|-------------|-----|
| **showcase** | Ground plane, pedestal, spinning cube, 6 orbiting/bobbing spheres, 4 outer cubes, scene graph hierarchy, camera orbit controls | `cargo run -p showcase` |
| **spinning_cube** | Minimal starter — single cube rotating on X/Y axes with delta-time animation and editor overlay | `cargo run -p spinning-cube` |
| **physics_sandbox** | Dynamic rigid bodies with gravity, static/rotating platforms, collision event logging, mouse-click raycast spawning | `cargo run -p physics-sandbox` |
| **audio_demo** | One-shot SFX (Space), looped music toggle (M), 4 orbiting spatial audio sources with distance attenuation | `cargo run -p audio-demo` |
| **scripting_demo** | Lua hot-reload (auto + R key), Rust↔Lua function bridging, 5 Lua-driven animated cubes with tunable globals | `cargo run -p scripting-demo` |
| **web_demo** | WebGPU browser deployment via Trunk | See [examples/web_demo/README.md](examples/web_demo/README.md) |

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

| Category | Library | Version | Used In |
|----------|---------|---------|----------|
| **Language** | [Rust](https://www.rust-lang.org/) 🦀 | Edition 2021 | — |
| **GPU** | [wgpu](https://wgpu.rs/) | 24 | render, editor, audio (compute), ui |
| **Windowing** | [winit](https://docs.rs/winit) | 0.30 | window, engine |
| **Math** | [glam](https://docs.rs/glam) | 0.29 | math (re-exported everywhere) |
| **Physics** | [Rapier3D](https://rapier.rs/) | 0.22 | physics |
| **Audio** | [Kira](https://docs.rs/kira) | 0.9 | audio |
| **Scripting** | [mlua](https://docs.rs/mlua) (Lua 5.4, vendored) | 0.10 | scripting |
| **WASM Runtime** | [wasmtime](https://wasmtime.dev/) | 28 | scripting (feature: wasm) |
| **Networking** | [Quinn](https://docs.rs/quinn) (QUIC) + [tokio](https://tokio.rs/) | 0.11 / 1 | core (feature: quinn-transport) |
| **Editor GUI** | [egui](https://www.egui.rs/) + egui-wgpu + egui-winit | 0.31 | editor |
| **Parallelism** | [rayon](https://docs.rs/rayon) | 1.10 | core (ECS), build |
| **Concurrency** | [parking_lot](https://docs.rs/parking_lot) + [crossbeam-channel](https://docs.rs/crossbeam-channel) | 0.12 / latest | core |
| **Serialization** | [serde](https://serde.rs/) + serde_json + [bincode](https://docs.rs/bincode) | 1 / 1 / 1 | core, build |
| **File Watching** | [notify](https://docs.rs/notify) | latest | core (asset server), scripting |
| **Texture Compression** | [intel_tex_2](https://docs.rs/intel_tex_2) (BC7) | 0.2 | build |
| **Mesh Optimization** | [meshopt](https://docs.rs/meshopt) | 0.3 | build |
| **glTF** | [gltf](https://docs.rs/gltf) | 1 | build |
| **Text Rendering** | [fontdue](https://docs.rs/fontdue) | latest | ui |
| **GPU Data** | [bytemuck](https://docs.rs/bytemuck) | 1 | render, math, ui, audio |
| **Images** | [image](https://docs.rs/image) | 0.25 | render, build |
| **Hashing** | [blake3](https://docs.rs/blake3) + [sha2](https://docs.rs/sha2) | 1 / 0.10 | build |
| **Proc Macros** | [syn](https://docs.rs/syn) + [quote](https://docs.rs/quote) | 2 / 1 | derive |
| **Profiling** | [puffin](https://docs.rs/puffin) + [tracy-client](https://docs.rs/tracy-client) | 0.19 / 0.17 | core (optional) |
| **ECS Internals** | [smallvec](https://docs.rs/smallvec) + [rustc-hash](https://docs.rs/rustc-hash) | latest | core |
| **WASM** | [wasm-bindgen](https://rustwasm.github.io/wasm-bindgen/) + web-sys | 0.2 | engine (wasm target) |

## Render Feature Flags

`quasar-render` ships with 14 individually toggleable features (all enabled in the `full` default):

| Flag | Description |
|------|-------------|
| `deferred` | G-Buffer + deferred light accumulation (100+ dynamic lights) |
| `clustered-lighting` | 16×9×24 froxel-based light binning (128 lights/cluster) |
| `ssr` | Screen-space reflections (hierarchical ray-march + roughness blend) |
| `terrain` | Heightmap terrain with splatmap texturing and adaptive LOD |
| `gpu-culling` | Hi-Z depth pyramid occlusion culling (8 mip levels) |
| `meshlet` | Meshlet clustering + per-meshlet frustum/cone culling (requires `gpu-culling`) |
| `particles` | 100K GPU particle system (compute sim + instanced draw) |
| `volumetric` | Ray-marched volumetric fog (Henyey-Greenstein phase function) |
| `lightmap` | Baked GI — CPU ray-cast baker + GPU path tracer |
| `reflection-probes` | 128³ cubemaps with parallax correction (up to 16 active) |
| `decals` | Deferred decal projection |
| `shader-graph` | Visual node-based material editor (WGSL output) |
| `sprites` | 2D sprite rendering + UI layer |
| `post-process` | FXAA, bloom, SSAO post-processing passes |

## WGSL Shaders

| Shader | Purpose |
|--------|---------|
| `basic.wgsl` | Forward PBR geometry pass (vertex + fragment) |
| `pbr.wgsl` | Cook-Torrance BRDF material evaluation |
| `shadow.wgsl` | Shadow map depth rendering |
| `skinned.wgsl` | GPU skeletal animation (bone matrices) |
| `sprite.wgsl` | 2D sprite rendering |
| `particle.wgsl` | Particle billboard rendering |
| `particle_compute.wgsl` | GPU particle simulation (compute) |
| `ssgi.wgsl` | Screen-space global illumination |
| `ssr.wgsl` | Screen-space reflections |
| `ssao.wgsl` | Screen-space ambient occlusion |
| `bloom.wgsl` | Multi-pass bloom blur |
| `fxaa.wgsl` | FXAA anti-aliasing |
| `tonemap.wgsl` | HDR → LDR filmic tonemapping |
| `hiz_build.wgsl` | Hierarchical Z-buffer construction (compute) |
| `lightmap_bake.wgsl` | Lightmap UV rendering |
| `lightmap_pathtrace.wgsl` | Path-traced GI baking (compute) |
| `gizmo.wgsl` | Editor transform gizmos |
| `convolution_reverb.wgsl` | Audio GPU convolution reverb (compute) |

## Project Structure

```
quasar/
├── crates/
│   ├── quasar-core/        # Archetype ECS, events, scene graph, animation, asset server,
│   │   └── src/ecs/        #   A* navigation, networking, delta compression, interest mgmt,
│   │                       #   prediction, reflection, prefabs, save/load
│   ├── quasar-math/        # Transform, Color, glam re-exports
│   ├── quasar-render/      # wgpu renderer (14 feature flags), GPU profiler, hot-reload
│   ├── quasar-window/      # winit window creation, input handling, action map
│   ├── quasar-physics/     # Rapier3D integration, async stepping, rollback snapshots
│   ├── quasar-audio/       # Kira audio, DSP chain, ambisonics, GPU convolution reverb
│   ├── quasar-scripting/   # Lua 5.4 (mlua) + WASM (wasmtime), component registry
│   ├── quasar-editor/      # egui panels, gizmos, shader/logic graph editors, timeline
│   ├── quasar-build/       # CLI asset pipeline (BC7, ASTC, meshopt, content-addressed)
│   ├── quasar-derive/      # #[derive(Inspect)] proc macro
│   ├── quasar-ui/          # Retained-mode flexbox UI, widget library, GPU text rendering
│   ├── quasar-mobile/      # Touch, gestures, sensors, haptics (Android/iOS)
│   └── quasar-engine/      # Meta-crate: prelude + winit game loop runner
├── examples/
│   ├── showcase/           # Multi-shape scene graph demo with camera orbit
│   ├── spinning_cube/      # Minimal single-cube starter
│   ├── physics_sandbox/    # Rigid body dynamics with raycast spawning
│   ├── audio_demo/         # Spatial audio with orbiting sources
│   ├── scripting_demo/     # Lua hot-reload with ECS bridge
│   └── web_demo/           # WebGPU browser deployment (Trunk)
├── assets/
│   ├── shaders/            # 18 WGSL shaders (PBR, shadows, post-fx, compute, gizmos)
│   └── lua/                # Lua type definitions (quasar.d.lua)
├── .github/workflows/      # Multi-platform CI (check, test, fmt, clippy, docs)
├── Cargo.toml              # Workspace: 13 crates + 5 examples
├── CONTRIBUTING.md
├── LICENSE                  # MIT
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
