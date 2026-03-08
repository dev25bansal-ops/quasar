# Quasar Engine — User Guide

## Overview

Quasar is a modular 3D game engine written in Rust, powered by `wgpu` for
cross-platform GPU rendering. It targets desktop (Windows/Linux/macOS),
web (WebGPU/WebGL via WASM), and mobile (Android/iOS).

## Crate Architecture

| Crate | Purpose |
|---|---|
| `quasar-core` | ECS, asset pipeline, networking, scenes, time, events, profiling |
| `quasar-math` | Transform, Vec3, Quat, Color (re-exports `glam`) |
| `quasar-render` | wgpu renderer, cameras, lights, materials, meshes, PBR, shadows, post-processing |
| `quasar-window` | winit window abstraction, input handling |
| `quasar-physics` | Rapier3D physics integration |
| `quasar-audio` | Kira audio with DSP effects |
| `quasar-scripting` | Lua 5.4 scripting via mlua |
| `quasar-editor` | egui-based editor overlay (hierarchy, inspector, console) |
| `quasar-engine` | Top-level runner that wires all crates together |
| `quasar-ui` | Widget toolkit (buttons, sliders, panels, text) |
| `quasar-mobile` | Android/iOS runner with touch, gestures, gyroscope, haptics |
| `quasar-derive` | Procedural macros (`#[derive(Component)]`) |
| `quasar-build` | CLI asset pipeline (texture compression, packaging) |

## Quick Start

```rust
use quasar_engine::prelude::*;

fn main() {
    let mut app = App::new();

    // Spawn an entity with a transform and mesh.
    let cube = app.world.spawn();
    app.world.insert(cube, Transform::IDENTITY);
    app.world.insert(cube, MeshShape::Cube);

    // Add a system that runs every frame.
    app.add_system("spin", |world: &mut World| {
        let dt = world.resource::<TimeSnapshot>()
            .map(|t| t.delta_seconds)
            .unwrap_or(1.0 / 60.0);
        world.for_each_mut(|_e: Entity, t: &mut Transform| {
            t.rotate(Vec3::Y, dt);
        });
    });

    // Run with default window.
    run(app, WindowConfig::default());
}
```

## ECS

### Entities & Components

```rust
let entity = world.spawn();
world.insert(entity, Transform::IDENTITY);
world.insert(entity, PointLight { color: Color::WHITE, intensity: 2.0, range: 10.0 });
world.despawn(entity);
```

### Queries

```rust
// Single component
for (entity, transform) in world.query::<Transform>() { }

// Two components
for (entity, mesh, transform) in world.query2::<MeshShape, Transform>() { }

// Up to five components
for (e, a, b, c, d, e_comp) in world.query5::<A, B, C, D, E>() { }

// Filtered queries
for (e, t) in world.query_with::<Transform, PointLight>() { }
for (e, t) in world.query_without::<Transform, PointLight>() { }
```

### Resources

```rust
world.insert_resource(MyGlobalState { score: 0 });
if let Some(state) = world.resource::<MyGlobalState>() { }
if let Some(state) = world.resource_mut::<MyGlobalState>() { }
```

### Systems

Systems are functions `fn(&mut World)` registered on the `App`:

```rust
app.add_system("my_system", my_system_fn);
```

## Rendering

### Feature Flags

`quasar-render` uses feature flags to control compilation of advanced subsystems.
The `full` feature (default) enables everything. Disable for smaller builds:

| Feature | Modules |
|---|---|
| `deferred` | G-buffer, deferred lighting, stencil light volumes |
| `ssr` | Screen-space reflections (requires `deferred`) |
| `terrain` | Heightfield terrain with LOD and splatmaps |
| `gpu-culling` | Hi-Z occlusion and GPU-driven culling |
| `particles` | GPU compute particle system |
| `volumetric` | Volumetric fog |
| `lightmap` | CPU and GPU lightmap baking, SH probes |
| `clustered-lighting` | Clustered forward lighting |
| `reflection-probes` | Cubemap reflection probes |
| `decals` | Projected decals |
| `shader-graph` | Visual shader graph compiler |
| `sprites` | 2D sprite batching and text rendering |
| `post-process` | FXAA, bloom, SSAO |

### Lights

```rust
world.insert(entity, DirectionalLight {
    direction: Vec3::new(-1.0, -1.0, -0.5),
    color: Color::WHITE,
    intensity: 1.0,
});
world.insert(entity, PointLight { color: Color::RED, intensity: 5.0, range: 15.0 });
```

### Materials

```rust
world.insert(entity, MaterialOverride {
    base_color: [0.8, 0.2, 0.2, 1.0],
    metallic: 0.0,
    roughness: 0.5,
});
```

## Physics (Rapier3D)

```rust
app.add_plugin(PhysicsPlugin);

// Add a rigid body
world.insert(entity, RigidBodyDesc::Dynamic);
world.insert(entity, ColliderDesc::Cuboid { half_extents: Vec3::splat(0.5) });
```

## Audio (Kira)

```rust
app.add_plugin(AudioPlugin);

// Play a sound
if let Some(audio) = world.resource_mut::<AudioManager>() {
    audio.play("assets/sounds/hit.ogg", PlaybackSettings::default());
}
```

## Scripting (Lua)

Place `.lua` scripts in the project's `scripts/` directory. The scripting
plugin exposes engine functions to Lua:

```lua
local entity = spawn_entity()
set_position(entity, 0, 5, 0)
set_scale(entity, 2, 2, 2)
log_info("Entity spawned at y=5")
```

## Asset Pipeline

### AssetServer (hot-reload)

```rust
app.add_plugin(AssetPlugin::new("assets"));
// AssetServer watches the directory and reloads changed files.
```

### AssetManager (typed handles)

The `AssetManager` is embedded in `AssetServer` and accessible via:

```rust
let server = world.resource::<AssetServer>().unwrap();
let handle = server.add_asset(my_texture);
let tex = server.get_asset::<Texture>(handle);
```

### Build Tool

```bash
cargo run -p quasar-build -- --target windows --compress-textures --gpu-format bc7
```

Supported targets: `windows`, `linux`, `macos`, `web`, `android`, `ios`.

## Networking

### UDP Transport + Rollback

```rust
let config = NetworkConfig {
    role: NetworkRole::Server,
    port: 7777,
    ..Default::default()
};
app.add_plugin(NetworkPlugin::new(config));
```

### QUIC Transport (optional)

Enable the `quinn-transport` feature on `quasar-core`:

```toml
quasar-core = { workspace = true, features = ["quinn-transport"] }
```

## Scene Graph

```rust
let parent = world.spawn();
let child = world.spawn();
let mut scene = SceneGraph::new();
scene.set_parent(child, parent);
scene.set_name(parent, "Root");
scene.propagate_transforms(&mut world);
```

## Editor

Press **F12** at runtime to toggle the editor overlay:

- **Hierarchy** — entity tree with drag-and-drop parenting
- **Inspector** — transform, material, and component editing
- **Console** — log output with filtering
- **Asset Browser** — filesystem view of project assets

## Profiling

Enable profiling features in `quasar-core`:

```toml
quasar-core = { workspace = true, features = ["puffin"] }
# or
quasar-core = { workspace = true, features = ["tracy"] }
```

## Web Builds

```bash
cd examples/web_demo
trunk serve   # requires `trunk` installed
```

The web demo initializes a WebGPU surface on an HTML canvas and runs the
engine's ECS tick + render loop via `requestAnimationFrame`.

## Mobile Builds

```rust
use quasar_mobile::{run_mobile, MobileConfig};

let app = App::new();
run_mobile(app, WindowConfig::default(), MobileConfig::default());
```

The mobile runner initializes the GPU renderer on resume, handles touch input
and gestures, and drives the render loop identically to the desktop runner
(minus the editor overlay).
