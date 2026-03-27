# quasar-scripting

Lua scripting support for the Quasar Engine.

## Features

- **Lua 5.4**: Full Lua VM via mlua
- **Hot Reload**: Automatic script reloading
- **ECS Bridge**: Spawn, despawn, modify entities
- **Component Registry**: String-based component access
- **Sandboxing**: Restricted, full, and readonly modes
- **WASM Scripting**: wasmtime-based WebAssembly scripts

## Usage

```rust
use quasar_scripting::ScriptingPlugin;

app.add_plugin(ScriptingPlugin);
```

## Lua API

```lua
-- Access entities
local entity = spawn_entity()
set_position(entity, 0, 1, 0)

-- Read blackboard
local health = get_value("health")

-- Log messages
log.info("Hello from Lua!")
```

## Security

The sandbox mode restricts:

- File system access
- OS commands
- Network access
