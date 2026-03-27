# Scripting Demo

Demonstrates Lua scripting in the Quasar Engine.

## Features

- Lua 5.4 scripting
- Hot reload support
- Entity manipulation
- Blackboard access

## Running

```bash
cargo run -p scripting-demo
```

## Scripts

Located in `scripts/`:

- `demo.lua` - Basic demo
- `physics.lua` - Physics interaction
- `ai.lua` - AI behavior

## Hot Reload

Edit scripts while the game runs - changes apply automatically!

## Lua API

```lua
-- Logging
log.info("Hello!")

-- Entity access
local entity = spawn_entity()
set_position(entity, 0, 1, 0)

-- Blackboard
set_value("health", 100)
local hp = get_value("health")
```
