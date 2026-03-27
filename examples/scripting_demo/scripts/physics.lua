-- Physics test script for Quasar Engine
-- Demonstrates physics interaction through scripting

physics_config = {
    gravity = -9.81,
    timestep = 1.0 / 60.0,
    iterations = 4
}

function on_collision(entity_a, entity_b, contact_point)
    log.debug("Collision: " .. entity_a .. " <-> " .. entity_b)
    
    -- Apply impulse at contact point
    local impulse = {
        x = 0,
        y = 5.0,
        z = 0
    }
    
    -- apply_impulse(entity_a, impulse)
    -- apply_impulse(entity_b, impulse)
    
    return true
end

function spawn_physics_cube(x, y, z)
    local entity = spawn_entity()
    set_position(entity, x, y, z)
    add_rigidbody(entity, 1.0)  -- mass = 1.0
    add_collider(entity, "cube", 0.5)
    return entity
end

function spawn_physics_sphere(x, y, z, radius)
    local entity = spawn_entity()
    set_position(entity, x, y, z)
    add_rigidbody(entity, 1.0)
    add_collider(entity, "sphere", radius)
    return entity
end

log.info("Physics script loaded!")
