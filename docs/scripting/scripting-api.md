# Scripting API

Quasar provides a Lua scripting system powered by mlua, allowing game logic to be written in Lua with safe access to engine features.

## Overview

```lua
-- scripts/player.lua

function on_init()
    quasar.log.info("Player initialized!")
end

function on_update(dt)
    local pos = self:get_position()
    pos.x = pos.x + 1.0 * dt
    self:set_position(pos)
end

function on_collision(other)
    if other:has_tag("enemy") then
        quasar.log.warn("Hit enemy!")
        self:take_damage(10)
    end
end
```

## Sandbox Security

By default, scripts run in a sandboxed environment:

```rust,ignore
// Allowed libraries
mlua::StdLib::COROUTINE
| mlua::StdLib::TABLE
| mlua::StdLib::STRING
| mlua::StdLib::UTF8
| mlua::StdLib::MATH

// Blocked libraries
// os.*   - No system access
// io.*   - No file access
// debug.* - No introspection
// package.* - No module loading
```

### Custom Capabilities

```rust,ignore
let caps = ScriptCapabilities {
    sandbox_mode: true,
    can_access_files: false,  // Enable for trusted scripts
    can_load_modules: false,
    max_memory: 1024 * 1024,  // 1MB limit
    max_instructions: 1_000_000,
};
```

## Engine API

### quasar Module

```lua
-- Version info
print(quasar.version)  -- "0.1.0"

-- Logging
quasar.log.info("Info message")
quasar.log.warn("Warning message")
quasar.log.error("Error message")
```

### Entity Operations

```lua
-- Get entity by name
local player = quasar.get_entity_by_name("Player")

-- Spawn new entity
local enemy = quasar.spawn_entity("Enemy")

-- Despawn entity
quasar.despawn_entity(enemy)
```

### Transform

```lua
-- Get position
local pos = entity:get_position()
print(pos.x, pos.y, pos.z)

-- Set position
entity:set_position({ x = 10.0, y = 0.0, z = 5.0 })

-- Get rotation
local rot = entity:get_rotation()
print(rot.x, rot.y, rot.z, rot.w)

-- Set rotation (quaternion)
entity:set_rotation({ x = 0, y = 0, z = 0, w = 1 })

-- Get scale
local scale = entity:get_scale()

-- Set scale
entity:set_scale({ x = 1.0, y = 1.0, z = 1.0 })
```

### Components

```lua
-- Check if entity has component
if entity:has("Health") then
    print("Entity has health")
end

-- Get component
local health = entity:get("Health")
print(health.current, health.max)

-- Set component
entity:set("Health", { current = 100, max = 100 })

-- Remove component
entity:remove("Health")
```

### Physics

```lua
-- Apply force
entity:apply_force({ x = 0, y = 100, z = 0 })

-- Apply impulse
entity:apply_impulse({ x = 10, y = 0, z = 0 })

-- Raycast
local hit = quasar.raycast(
    { x = 0, y = 0, z = 0 },    -- origin
    { x = 0, y = -1, z = 0 },   -- direction
    100.0                        -- max distance
)

if hit then
    print("Hit entity: " .. hit.entity)
    print("Distance: " .. hit.distance)
end
```

### Input

```lua
function on_update(dt)
    -- Check key state
    if quasar.input.is_key_pressed("W") then
        entity:apply_force({ x = 0, y = 0, z = 10 })
    end

    -- Get mouse position
    local mouse = quasar.input.get_mouse_position()
    print(mouse.x, mouse.y)

    -- Check mouse button
    if quasar.input.is_mouse_pressed(0) then
        -- Left click
    end
end
```

### Audio

```lua
-- Play sound effect
quasar.audio.play("sounds/jump.ogg")

-- Play at position (3D spatial audio)
quasar.audio.play_at("sounds/explosion.ogg", { x = 10, y = 0, z = 0 })

-- Set listener position
quasar.audio.set_listener_position({ x = player_pos.x, y = player_pos.y, z = player_pos.z })

-- Set volume (0.0 - 1.0)
quasar.audio.set_volume(0.8)
```

### Events

```lua
-- Subscribe to event
quasar.events.subscribe("OnDamage", function(event)
    print("Damage: " .. event.amount)
    print("Source: " .. event.source)
end)

-- Emit event
quasar.events.emit("OnDamage", {
    target = player,
    amount = 25,
    source = enemy
})
```

## Lifecycle Hooks

### Script Lifecycle

```lua
-- Called when script is loaded
function on_init()
    print("Script loaded!")
end

-- Called every frame
function on_update(dt)
    -- dt: delta time in seconds
end

-- Called at fixed rate (default 60Hz)
function on_fixed_update(dt)
    -- Physics calculations
end

-- Called when entity is destroyed
function on_destroy()
    -- Cleanup
end
```

### Collision Hooks

```lua
-- Called on collision enter
function on_collision_enter(other)
    print("Collision with: " .. other:get_name())
end

-- Called every frame during collision
function on_collision_stay(other)
    -- Continuous collision
end

-- Called when collision ends
function on_collision_exit(other)
    print("Collision ended")
end
```

### Trigger Hooks

```lua
-- For trigger colliders (no physics response)
function on_trigger_enter(other)
    if other:has_tag("pickup") then
        other:destroy()
        inventory:add_item(other:get("Item"))
    end
end

function on_trigger_exit(other)
    -- Left trigger zone
end
```

## Custom Types

### Vector3

```lua
local v = vec3.new(1.0, 2.0, 3.0)

-- Operations
local sum = v + vec3.new(1, 0, 0)
local scaled = v * 2.0
local length = v:length()
local normalized = v:normalize()

-- Component access
v.x = 10.0
v.y = 20.0
v.z = 30.0
```

### Quaternion

```lua
local q = quat.from_euler(0, math.rad(90), 0)

-- Operations
local combined = q * quat.from_axis_angle(vec3.UP, math.rad(45))
local rotated = q:rotate_vector(vec3.new(1, 0, 0))
```

### Color

```lua
local color = color.new(1.0, 0.5, 0.0, 1.0)  -- RGBA

-- Presets
color.RED
color.GREEN
color.BLUE
color.WHITE
color.BLACK
```

## Debugging

### Debug Draw

```lua
-- Draw line
quasar.debug.draw_line(
    { x = 0, y = 0, z = 0 },
    { x = 10, y = 10, z = 10 },
    color.RED
)

-- Draw sphere
quasar.debug.draw_sphere({ x = 5, y = 0, z = 0 }, 1.0, color.GREEN)

-- Draw box
quasar.debug.draw_box({ x = 0, y = 0, z = 0 }, { x = 2, y = 2, z = 2 }, color.BLUE)
```

### Console

```lua
-- Print to console
print("Debug message")

-- Log levels
quasar.log.info("Info")
quasar.log.warn("Warning")
quasar.log.error("Error")
```

## Hot Reload

Scripts are automatically reloaded when files change:

```rust,ignore
// In Rust, check for reloads
let changed = script_engine.check_hot_reload();
for path in &changed {
    log::info!("Hot-reloaded: {}", path);
}
```

## Performance Tips

### 1. Cache References

```lua
-- Bad - lookup every frame
function on_update(dt)
    local player = quasar.get_entity_by_name("Player")
    player:get_position()
end

-- Better - cache in on_init
local player

function on_init()
    player = quasar.get_entity_by_name("Player")
end

function on_update(dt)
    player:get_position()
end
```

### 2. Minimize Component Access

```lua
-- Bad - multiple get calls
local x = entity:get("Position").x
local y = entity:get("Position").y

-- Better - single get
local pos = entity:get("Position")
local x, y = pos.x, pos.y
```

### 3. Use Local Functions

```lua
-- Faster - local function
local sqrt = math.sqrt

function on_update(dt)
    local dist = sqrt(dx * dx + dy * dy)
end
```

## Rust Integration

### Registering Functions

```rust,ignore
let engine = ScriptEngine::new()?;

// Register custom function
let my_func = engine.lua.create_function(|_, (a, b): (i32, i32)| {
    Ok(a + b)
})?;

engine.lua.globals().set("my_func", my_func)?;
```

### Exposing Components

```rust,ignore
// Register component type
engine.register_component::<Health>("Health", |lua| {
    let tbl = lua.create_table()?;
    tbl.set("current", 100)?;
    tbl.set("max", 100)?;
    Ok(tbl)
})?;
```

### Calling Lua from Rust

```rust,ignore
// Call Lua function
let result: i32 = engine.eval("return 1 + 2")?;

// Execute script file
engine.exec_file("scripts/game.lua")?;
```

## Security Best Practices

### 1. Validate All Input

```lua
function on_damage(amount)
    -- Validate
    if type(amount) ~= "number" or amount < 0 then
        quasar.log.error("Invalid damage amount")
        return
    end

    health.current = math.max(0, health.current - amount)
end
```

### 2. Limit Resource Access

```lua
-- Don't expose sensitive data
function get_secret()
    return nil  -- Or error
end
```

### 3. Handle Errors

```lua
function safe_call(fn, ...)
    local ok, err = pcall(fn, ...)
    if not ok then
        quasar.log.error("Error: " .. tostring(err))
    end
    return ok
end
```

## Next Steps

- [Plugin Development](../plugin-development.md)
- [Examples](../examples/)
