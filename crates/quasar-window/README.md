# quasar-window

Window creation and input handling for the Quasar Engine.

## Features

- **Window Creation**: winit-based cross-platform windows
- **Keyboard Input**: Per-frame state tracking
- **Mouse Input**: Position, delta, scroll, buttons
- **Gamepad Support**: Controller input with dead zones
- **Action Maps**: Named input bindings
- **Input Rebinding**: Runtime key remapping

## Usage

```rust
use quasar_window::{WindowPlugin, ActionMap};

app.add_plugin(WindowPlugin);

let actions = ActionMap::new()
    .bind("jump", KeyCode::Space)
    .bind("move", Axis::WASD);
```

## Input Rebinding

```rust
use quasar_window::InputRebinding;

let config = InputRebinding::load("input_config.json");
config.save("input_config.json");
```
