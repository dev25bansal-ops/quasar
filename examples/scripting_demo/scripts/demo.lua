-- Demo Lua script for Quasar Engine
-- This file is loaded by the scripting_demo example

-- Animation parameters (can be changed and will hot-reload)
animation_speed = 1.5
animation_amplitude = 0.8

-- Called when script loads
log.info("Demo Lua script loaded!")
log.info("Animation speed: " .. animation_speed)
log.info("Amplitude: " .. animation_amplitude)

-- You can define utility functions
function lerp(a, b, t)
    return a + (b - a) * t
end

function clamp(val, min, max)
    if val < min then return min end
    if val > max then return max end
    return val
end

-- Entity update function (called from Rust)
function update_cubes(time)
    -- This would be called from the engine
    -- to update entity positions
    for i = 0, 4 do
        local phase = i * 3.14159 / 5
        local y = animation_amplitude * math.sin(time * animation_speed + phase)
        -- update_entity("Cube_" .. i, 0, y, 0)
    end
end

log.info("Script initialization complete!")
