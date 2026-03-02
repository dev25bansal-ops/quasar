---@meta

--- Quasar Engine Lua API Types

quasar = {}

---@class Quasar
--- Engine version
quasar.version = string

--- Internal use - transforms table (entity index -> transform data)
---@type table<number, {px: number, py: number, pz: number, rx: number, ry: number, rz: number, rw: number, sx: number, sy: number, sz: number}>
quasar._transforms = {}

--- Internal use - delta time in seconds
---@type number
quasar._dt = 0

--- Internal use - total elapsed time in seconds
---@type number
quasar._time = 0

--- Internal use - pressed keys table
---@type table<string, boolean>
quasar._pressed_keys = {}

--- Internal use - pressed mouse buttons
---@type table<string, boolean>
quasar._pressed_mouse = {}

--- Internal use - commands queue
---@type table<number, {type: string, entity?: number, x?: number, y?: number, z?: number, w?: number}>
quasar._commands = {}

log = {}

--- Log an info message
---@param msg string
function log.info(msg) end

--- Log a warning message
---@param msg string
function log.warn(msg) end

--- Log an error message
---@param msg string
function log.error(msg) end

--- Called once per frame with delta time
---@param dt number Delta time in seconds
function on_update(dt) end

--- Per-entity script initialization callback
---@param entity_id number The entity ID this script is attached to
function on_init(entity_id) end

--- Per-entity script update callback
---@param entity_id number The entity ID this script is attached to
---@param dt number Delta time in seconds
function on_update(entity_id, dt) end

--- Command types for quasar._commands
---@alias CommandType "set_position" | "set_rotation" | "set_scale" | "spawn" | "despawn"

--- Push a set position command
---@param entity_id number
---@param x number
---@param y number
---@param z number
function quasar.set_position(entity_id, x, y, z) end

--- Push a set rotation command
---@param entity_id number
---@param x number Quaternion x component
---@param y number Quaternion y component
---@param z number Quaternion z component
---@param w number Quaternion w component
function quasar.set_rotation(entity_id, x, y, z, w) end

--- Push a set scale command
---@param entity_id number
---@param x number
---@param y number
---@param z number
function quasar.set_scale(entity_id, x, y, z) end

--- Push a spawn command
function quasar.spawn() end

--- Push a despawn command
---@param entity_id number
function quasar.despawn(entity_id) end

return quasar
