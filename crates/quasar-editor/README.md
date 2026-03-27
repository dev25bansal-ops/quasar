# quasar-editor

In-game editor built with egui for the Quasar Engine.

## Features

- **Hierarchy Panel**: Multi-select tree view
- **Inspector**: Auto-generated from `#[derive(Inspect)]`
- **Console**: 512-entry ring buffer
- **Asset Browser**: Grid view with type detection
- **Gizmos**: Translate, rotate, scale tools
- **Shader Graph**: Node-based WGSL generation
- **Logic Graph**: Visual Lua code generation
- **Timeline**: Keyframe animation editor
- **GPU Profiler**: Per-pass timing overlay
- **Play-in-Editor**: Snapshot/restore with undo/redo
- **Lightmap Baking**: Integrated baker
- **Lua REPL**: Interactive console

## Usage

```rust
use quasar_editor::EditorPlugin;

app.add_plugin(EditorPlugin);

// Toggle with F12
// Exit play mode with ESC
```

## Gizmos

- Translate: W
- Rotate: E
- Scale: R
