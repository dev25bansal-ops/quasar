-- AI behavior test script for Quasar Engine
-- Demonstrates behavior tree and blackboard usage

ai_config = {
    detection_range = 10.0,
    attack_range = 2.0,
    patrol_speed = 2.0,
    chase_speed = 4.0
}

blackboard = {
    target = nil,
    last_known_position = { x = 0, y = 0, z = 0 },
    state = "patrol",
    health = 100
}

function can_see_target()
    if blackboard.target == nil then
        return false
    end
    
    local distance = get_distance(get_self_entity(), blackboard.target)
    return distance < ai_config.detection_range
end

function patrol(delta_time)
    -- Move along patrol path
    local waypoints = get_patrol_waypoints()
    if waypoints then
        move_toward(waypoints[current_waypoint], ai_config.patrol_speed * delta_time)
    end
end

function chase(delta_time)
    if blackboard.target then
        move_toward(blackboard.target, ai_config.chase_speed * delta_time)
    end
end

function attack()
    if blackboard.target then
        local distance = get_distance(get_self_entity(), blackboard.target)
        if distance < ai_config.attack_range then
            -- Perform attack
            log.debug("Attacking target!")
            return true
        end
    end
    return false
end

function update_ai(delta_time)
    if blackboard.health <= 0 then
        blackboard.state = "dead"
        return
    end
    
    if can_see_target() then
        blackboard.state = "chase"
        chase(delta_time)
        
        if attack() then
            blackboard.state = "attack"
        end
    else
        blackboard.state = "patrol"
        patrol(delta_time)
    end
end

log.info("AI behavior script loaded!")
