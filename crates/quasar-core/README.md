# quasar-core

The foundation of the Quasar Engine, providing core systems and utilities.

## Features

- **ECS (Entity-Component-System)**: Lightweight, type-safe archetype-based ECS
- **Application lifecycle**: App builder pattern and main loop management
- **Events**: Typed event bus for decoupled communication
- **Time**: Delta time tracking and fixed timestep support
- **Plugins**: Modular engine extension system
- **Animation**: Keyframe-based animation with compression
- **Asset Server**: Hot-reload capable asset pipeline
- **Networking**: QUIC/UDP game networking with rollback support
- **AI**: Behavior tree system for game AI
- **Localization**: Internationalization (i18n) support
- **Save/Load**: Binary and JSON serialization with compression

## Usage

```rust
use quasar_core::prelude::*;

let mut app = App::new();
app.add_plugin(MyPlugin)
    .add_system("update", my_system)
    .run();
```

## Modules

- `ai` - Behavior trees and blackboard
- `animation` - Keyframe animation
- `app` - Application builder
- `asset` - Asset handle management
- `asset_server` - Async asset loading
- `ecs` - Entity-Component-System
- `error` - Error types
- `event` - Event bus
- `localization` - i18n
- `network` - Game networking
- `plugin` - Plugin system
- `profiler` - Performance profiling
- `save_load` - Serialization
- `scene` - Scene graph
