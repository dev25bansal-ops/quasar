# quasar-engine

Meta-crate that bundles all Quasar Engine components.

## Features

This crate re-exports all Quasar crates for convenient usage:

- `quasar-core` - ECS, app, networking
- `quasar-math` - Math types
- `quasar-render` - Rendering pipeline
- `quasar-physics` - Physics simulation
- `quasar-audio` - Audio system
- `quasar-window` - Input handling
- `quasar-scripting` - Lua scripting
- `quasar-editor` - In-game editor
- `quasar-ui` - UI widgets

## Usage

```rust
use quasar_engine::prelude::*;

let mut app = App::new();
app.add_plugin(WindowPlugin)
    .add_plugin(RenderPlugin)
    .add_plugin(PhysicsPlugin)
    .run();
```

## Platform Support

- Windows, Linux, macOS
- Web (WASM/WebGPU)
- Android, iOS
