# quasar-ui

Retained-mode UI system for the Quasar Engine.

## Features

- **Flexbox Layout**: CSS-like layout engine
- **Widgets**: Button, checkbox, slider, progress bar, text input
- **Text Rendering**: fontdue-based text rendering
- **GPU Batching**: Efficient quad batching

## Usage

```rust
use quasar_ui::{UiPlugin, UiTree, Button};

app.add_plugin(UiPlugin);

let mut ui = UiTree::new();
ui.add(Button::new("Click Me"));
```

## Widgets

- `Button` - Clickable button
- `Checkbox` - Boolean toggle
- `Slider` - Value range selector
- `ProgressBar` - Progress indicator
- `TextInput` - Text entry field
- `Panel` - Container widget
- `Text` - Label widget
