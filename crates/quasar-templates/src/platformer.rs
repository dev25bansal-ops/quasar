//! Platformer game template
//!
//! Provides components and systems for building 2D/3D platformer games including:
//! - Player controller with jumping, double jump, wall jump
//! - Collectibles (coins, gems, power-ups)
//! - Moving platforms
//! - Hazards (spikes, lava, enemies)
//! - Checkpoints and level progression
//! - Camera following

use glam::Vec2;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;


// ============================================================================
// Player Components
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformerPlayer {
    pub move_speed: f32,
    pub jump_force: f32,
    pub gravity: f32,
    pub max_fall_speed: f32,
    pub coyote_time: f32,
    pub coyote_timer: f32,
    pub jump_buffer_time: f32,
    pub jump_buffer_timer: f32,
    pub jumps_remaining: u32,
    pub max_jumps: u32,
    pub is_grounded: bool,
    pub is_jumping: bool,
    pub is_falling: bool,
    pub facing_right: bool,
    pub velocity: [f32; 2],
}

impl Default for PlatformerPlayer {
    fn default() -> Self {
        Self {
            move_speed: 200.0,
            jump_force: 400.0,
            gravity: 800.0,
            max_fall_speed: 600.0,
            coyote_time: 0.1,
            coyote_timer: 0.0,
            jump_buffer_time: 0.1,
            jump_buffer_timer: 0.0,
            jumps_remaining: 1,
            max_jumps: 1,
            is_grounded: false,
            is_jumping: false,
            is_falling: false,
            facing_right: true,
            velocity: [0.0, 0.0],
        }
    }
}

impl PlatformerPlayer {
    pub fn can_jump(&self) -> bool {
        self.jumps_remaining > 0 || self.coyote_timer > 0.0
    }

    pub fn jump(&mut self) {
        if self.can_jump() {
            self.velocity[1] = self.jump_force;
            self.is_jumping = true;
            self.is_grounded = false;
            if self.coyote_timer <= 0.0 {
                self.jumps_remaining -= 1;
            }
            self.coyote_timer = 0.0;
        }
    }

    pub fn land(&mut self) {
        self.is_grounded = true;
        self.is_jumping = false;
        self.is_falling = false;
        self.jumps_remaining = self.max_jumps;
        self.coyote_timer = self.coyote_time;
    }

    pub fn update_timers(&mut self, delta: f32) {
        if !self.is_grounded && self.coyote_timer > 0.0 {
            self.coyote_timer -= delta;
        }
        if self.jump_buffer_timer > 0.0 {
            self.jump_buffer_timer -= delta;
        }
    }

    pub fn apply_gravity(&mut self, delta: f32) {
        if !self.is_grounded {
            self.velocity[1] -= self.gravity * delta;
            self.velocity[1] = self.velocity[1].max(-self.max_fall_speed);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WallSlide {
    pub slide_speed: f32,
    pub wall_jump_force: [f32; 2],
    pub wall_stick_time: f32,
    pub wall_stick_timer: f32,
    pub is_touching_wall: bool,
    pub wall_direction: f32,
}

impl Default for WallSlide {
    fn default() -> Self {
        Self {
            slide_speed: 50.0,
            wall_jump_force: [300.0, 400.0],
            wall_stick_time: 0.2,
            wall_stick_timer: 0.0,
            is_touching_wall: false,
            wall_direction: 0.0,
        }
    }
}

impl WallSlide {
    pub fn wall_jump(&mut self, player: &mut PlatformerPlayer) {
        if self.is_touching_wall {
            player.velocity[0] = -self.wall_direction * self.wall_jump_force[0];
            player.velocity[1] = self.wall_jump_force[1];
            player.is_jumping = true;
            player.is_grounded = false;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dash {
    pub dash_speed: f32,
    pub dash_duration: f32,
    pub dash_timer: f32,
    pub dash_cooldown: f32,
    pub cooldown_timer: f32,
    pub is_dashing: bool,
    pub dash_direction: [f32; 2],
}

impl Default for Dash {
    fn default() -> Self {
        Self {
            dash_speed: 600.0,
            dash_duration: 0.2,
            dash_timer: 0.0,
            dash_cooldown: 1.0,
            cooldown_timer: 0.0,
            is_dashing: false,
            dash_direction: [1.0, 0.0],
        }
    }
}

impl Dash {
    pub fn can_dash(&self) -> bool {
        self.cooldown_timer <= 0.0 && !self.is_dashing
    }

    pub fn start_dash(&mut self, direction: [f32; 2]) {
        if self.can_dash() {
            self.is_dashing = true;
            self.dash_timer = self.dash_duration;
            let len = (direction[0].powi(2) + direction[1].powi(2)).sqrt();
            if len > 0.0 {
                self.dash_direction = [direction[0] / len, direction[1] / len];
            }
        }
    }

    pub fn update(&mut self, delta: f32) {
        if self.is_dashing {
            self.dash_timer -= delta;
            if self.dash_timer <= 0.0 {
                self.is_dashing = false;
                self.cooldown_timer = self.dash_cooldown;
            }
        } else if self.cooldown_timer > 0.0 {
            self.cooldown_timer -= delta;
        }
    }
}

// ============================================================================
// Collectibles
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collectible {
    pub collectible_type: CollectibleType,
    pub value: u32,
    pub spawn_weight: f32,
    pub bob_speed: f32,
    pub bob_amount: f32,
    pub rotation_speed: f32,
    pub bob_offset: f32,
}

impl Collectible {
    pub fn coin(value: u32) -> Self {
        Self {
            collectible_type: CollectibleType::Coin,
            value,
            spawn_weight: 1.0,
            bob_speed: 2.0,
            bob_amount: 5.0,
            rotation_speed: 180.0,
            bob_offset: 0.0,
        }
    }

    pub fn gem(value: u32) -> Self {
        Self {
            collectible_type: CollectibleType::Gem,
            value,
            spawn_weight: 0.3,
            bob_speed: 1.5,
            bob_amount: 8.0,
            rotation_speed: 90.0,
            bob_offset: 0.0,
        }
    }

    pub fn power_up(power_up_type: PowerUpType) -> Self {
        Self {
            collectible_type: CollectibleType::PowerUp(power_up_type),
            value: 1,
            spawn_weight: 0.1,
            bob_speed: 1.0,
            bob_amount: 10.0,
            rotation_speed: 45.0,
            bob_offset: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CollectibleType {
    Coin,
    Gem,
    PowerUp(PowerUpType),
    Key { door_id: u32 },
    Health { amount: u32 },
    ExtraLife,
    Checkpoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PowerUpType {
    SpeedBoost,
    DoubleJump,
    Invincibility,
    Magnet,
    Shield,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlayerInventory {
    pub coins: u32,
    pub gems: u32,
    pub keys: HashMap<u32, u32>,
    pub lives: u32,
    pub score: u64,
}

impl PlayerInventory {
    pub fn add_collectible(&mut self, collectible: &Collectible) {
        match collectible.collectible_type {
            CollectibleType::Coin => self.coins += collectible.value,
            CollectibleType::Gem => self.gems += collectible.value,
            CollectibleType::Key { door_id } => {
                *self.keys.entry(door_id).or_insert(0) += 1;
            }
            CollectibleType::Health { amount: _ } => {}
            CollectibleType::ExtraLife => self.lives += 1,
            CollectibleType::Checkpoint => {}
            CollectibleType::PowerUp(_) => {}
        }
        self.score += collectible.value as u64 * 100;
    }

    pub fn has_key(&self, door_id: u32) -> bool {
        self.keys.get(&door_id).copied().unwrap_or(0) > 0
    }

    pub fn use_key(&mut self, door_id: u32) -> bool {
        if let Some(count) = self.keys.get_mut(&door_id) {
            if *count > 0 {
                *count -= 1;
                return true;
            }
        }
        false
    }
}

// ============================================================================
// Platforms
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Platform {
    pub platform_type: PlatformType,
    pub size: [f32; 2],
    pub is_one_way: bool,
}

impl Platform {
    pub fn solid(size: [f32; 2]) -> Self {
        Self {
            platform_type: PlatformType::Static,
            size,
            is_one_way: false,
        }
    }

    pub fn one_way(size: [f32; 2]) -> Self {
        Self {
            platform_type: PlatformType::Static,
            size,
            is_one_way: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlatformType {
    Static,
    Moving(MovementPattern),
    Falling { fall_delay: u16, respawn_time: u16 },
    Crumbling { stages: u8, time_per_stage: u16 },
    Bouncy { bounce_force: u16 },
    Conveyor { speed: u16, direction: i16 },
    Ice { friction: u16 },
    Sticky { speed_multiplier: u16 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MovementPattern {
    Linear {
        start: [i32; 2],
        end: [i32; 2],
        speed: u16,
    },
    Circular {
        center: [i32; 2],
        radius: u16,
        angular_speed: u16,
    },
    Sine {
        axis: Axis,
        amplitude: u16,
        frequency: u16,
    },
    Path {
        path_id: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Axis {
    X,
    Y,
}

#[derive(Debug, Clone)]
pub struct MovingPlatformState {
    pub current_time: f32,
    pub is_waiting: bool,
    pub wait_timer: f32,
    pub wait_duration: f32,
    pub direction: i32,
    pub start_position: Vec2,
    pub current_position: Vec2,
}

impl MovingPlatformState {
    pub fn new(start: Vec2) -> Self {
        Self {
            current_time: 0.0,
            is_waiting: false,
            wait_timer: 0.0,
            wait_duration: 1.0,
            direction: 1,
            start_position: start,
            current_position: start,
        }
    }

    pub fn update_linear(&mut self, delta: f32, start: Vec2, end: Vec2, speed: f32) {
        if self.is_waiting {
            self.wait_timer -= delta;
            if self.wait_timer <= 0.0 {
                self.is_waiting = false;
            }
            return;
        }

        let distance = (end - start).length();
        let travel_time = distance / speed;

        self.current_time += delta * self.direction as f32;

        if self.current_time >= travel_time {
            self.current_time = travel_time;
            self.direction = -1;
            self.is_waiting = true;
            self.wait_timer = self.wait_duration;
        } else if self.current_time <= 0.0 {
            self.current_time = 0.0;
            self.direction = 1;
            self.is_waiting = true;
            self.wait_timer = self.wait_duration;
        }

        let t = self.current_time / travel_time;
        self.current_position = start + (end - start) * t;
    }
}

#[derive(Debug, Clone)]
pub struct FallingPlatform {
    pub is_triggered: bool,
    pub trigger_timer: f32,
    pub fall_delay: f32,
    pub is_falling: bool,
    pub fall_speed: f32,
    pub respawn_timer: f32,
    pub respawn_time: f32,
    pub original_position: Vec2,
}

impl FallingPlatform {
    pub fn new(position: Vec2, fall_delay: f32, respawn_time: f32) -> Self {
        Self {
            is_triggered: false,
            trigger_timer: 0.0,
            fall_delay,
            is_falling: false,
            fall_speed: 0.0,
            respawn_timer: 0.0,
            respawn_time,
            original_position: position,
        }
    }

    pub fn trigger(&mut self) {
        if !self.is_triggered && !self.is_falling {
            self.is_triggered = true;
            self.trigger_timer = self.fall_delay;
        }
    }

    pub fn update(&mut self, delta: f32, gravity: f32) {
        if self.is_triggered && !self.is_falling {
            self.trigger_timer -= delta;
            if self.trigger_timer <= 0.0 {
                self.is_falling = true;
            }
        }

        if self.is_falling {
            self.fall_speed += gravity * delta;
            self.respawn_timer += delta;
            if self.respawn_timer >= self.respawn_time {
                self.is_falling = false;
                self.is_triggered = false;
                self.respawn_timer = 0.0;
                self.fall_speed = 0.0;
            }
        }
    }
}

// ============================================================================
// Hazards
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hazard {
    pub hazard_type: HazardType,
    pub damage: u32,
    pub knockback: f32,
    pub knockback_direction: [f32; 2],
    pub cooldown: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HazardType {
    Spikes,
    Lava,
    Pit,
    Sawblade {
        rotation_speed: u16,
    },
    Laser {
        is_active: bool,
        active_duration: u16,
        inactive_duration: u16,
    },
    Enemy,
}

#[derive(Debug, Clone)]
pub struct EnemyAi {
    pub behavior: EnemyBehavior,
    pub patrol_points: Vec<Vec2>,
    pub current_patrol_index: usize,
    pub detection_range: f32,
    pub attack_range: f32,
    pub is_chasing: bool,
    pub is_attacking: bool,
    pub attack_cooldown: f32,
    pub attack_timer: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnemyBehavior {
    Patrol,
    Chase,
    Guard { position: [i32; 2] },
    Jumping { jump_interval: u16 },
    Flying { height: u16 },
}

impl EnemyAi {
    pub fn patrol(patrol_points: Vec<Vec2>) -> Self {
        Self {
            behavior: EnemyBehavior::Patrol,
            patrol_points,
            current_patrol_index: 0,
            detection_range: 200.0,
            attack_range: 50.0,
            is_chasing: false,
            is_attacking: false,
            attack_cooldown: 1.0,
            attack_timer: 0.0,
        }
    }
}

// ============================================================================
// Checkpoints & Progression
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub checkpoint_id: u32,
    pub is_activated: bool,
    pub is_level_end: bool,
    pub spawn_point: [f32; 2],
    pub next_level: Option<String>,
}

impl Checkpoint {
    pub fn new(id: u32, spawn_point: [f32; 2]) -> Self {
        Self {
            checkpoint_id: id,
            is_activated: false,
            is_level_end: false,
            spawn_point,
            next_level: None,
        }
    }

    pub fn level_end(id: u32, spawn_point: [f32; 2], next_level: String) -> Self {
        Self {
            checkpoint_id: id,
            is_activated: false,
            is_level_end: true,
            spawn_point,
            next_level: Some(next_level),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct LevelProgress {
    pub current_checkpoint: u32,
    pub furthest_checkpoint: u32,
    pub time_played: f32,
    pub deaths: u32,
    pub collectibles_found: u32,
    pub total_collectibles: u32,
}

impl LevelProgress {
    pub fn activate_checkpoint(&mut self, checkpoint_id: u32) -> bool {
        if checkpoint_id > self.furthest_checkpoint {
            self.furthest_checkpoint = checkpoint_id;
            self.current_checkpoint = checkpoint_id;
            true
        } else {
            self.current_checkpoint = checkpoint_id;
            false
        }
    }
}

// ============================================================================
// Camera
// ============================================================================

#[derive(Debug, Clone)]
pub struct CameraFollow {
    pub follow_speed: f32,
    pub offset: Vec2,
    pub look_ahead: f32,
    pub look_ahead_smoothing: f32,
    pub dead_zone: Vec2,
    pub bounds: Option<CameraBounds>,
    pub current_look_ahead: f32,
}

impl Default for CameraFollow {
    fn default() -> Self {
        Self {
            follow_speed: 5.0,
            offset: Vec2::ZERO,
            look_ahead: 50.0,
            look_ahead_smoothing: 0.1,
            dead_zone: Vec2::new(20.0, 20.0),
            bounds: None,
            current_look_ahead: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CameraBounds {
    pub min: Vec2,
    pub max: Vec2,
}

impl CameraBounds {
    pub fn new(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Self {
        Self {
            min: Vec2::new(min_x, min_y),
            max: Vec2::new(max_x, max_y),
        }
    }

    pub fn clamp(&self, position: Vec2) -> Vec2 {
        Vec2::new(
            position.x.clamp(self.min.x, self.max.x),
            position.y.clamp(self.min.y, self.max.y),
        )
    }
}

// ============================================================================
// Power-Up Effects
// ============================================================================

#[derive(Debug, Clone)]
#[derive(Default)]
pub struct ActivePowerUps {
    pub active: Vec<ActivePowerUp>,
}


#[derive(Debug, Clone)]
pub struct ActivePowerUp {
    pub power_up_type: PowerUpType,
    pub remaining_duration: f32,
    pub max_duration: f32,
}

impl ActivePowerUps {
    pub fn add(&mut self, power_up_type: PowerUpType, duration: f32) {
        if let Some(existing) = self.active.iter_mut().find(|p| {
            std::mem::discriminant(&p.power_up_type) == std::mem::discriminant(&power_up_type)
        }) {
            existing.remaining_duration = duration;
            existing.max_duration = duration;
        } else {
            self.active.push(ActivePowerUp {
                power_up_type,
                remaining_duration: duration,
                max_duration: duration,
            });
        }
    }

    pub fn update(&mut self, delta: f32) {
        for power_up in &mut self.active {
            power_up.remaining_duration -= delta;
        }
        self.active.retain(|p| p.remaining_duration > 0.0);
    }

    pub fn has(&self, power_up_type: PowerUpType) -> bool {
        self.active.iter().any(|p| p.power_up_type == power_up_type)
    }
}

// ============================================================================
// Doors & Interactables
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedDoor {
    pub door_id: u32,
    pub is_open: bool,
    pub open_animation_duration: f32,
    pub open_timer: f32,
}

impl LockedDoor {
    pub fn new(door_id: u32) -> Self {
        Self {
            door_id,
            is_open: false,
            open_animation_duration: 0.5,
            open_timer: 0.0,
        }
    }

    pub fn try_open(&mut self, inventory: &mut PlayerInventory) -> bool {
        if inventory.use_key(self.door_id) {
            self.is_open = true;
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Interactable {
    pub interaction_type: InteractionType,
    pub interaction_range: f32,
    pub is_interactable: bool,
    pub oneshot: bool,
    pub used: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InteractionType {
    Sign { text: String },
    Lever { target_id: u32 },
    Button { target_id: u32 },
    PressurePlate { target_id: u32 },
    Portal { destination: String },
    Npc { dialogue_id: String },
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platformer_player_jump() {
        let mut player = PlatformerPlayer::default();
        assert!(player.can_jump());

        player.jump();
        assert!(player.velocity[1] > 0.0);
        assert!(player.is_jumping);
        assert!(!player.is_grounded);
    }

    #[test]
    fn test_platformer_player_land() {
        let mut player = PlatformerPlayer::default();
        player.jump();
        player.land();

        assert!(player.is_grounded);
        assert!(!player.is_jumping);
        assert_eq!(player.jumps_remaining, player.max_jumps);
    }

    #[test]
    fn test_double_jump() {
        let mut player = PlatformerPlayer {
            max_jumps: 2,
            jumps_remaining: 2,
            ..Default::default()
        };

        player.jump();
        let first_jump_velocity = player.velocity[1];
        player.jumps_remaining = player.jumps_remaining.saturating_sub(1);

        player.jump();
        assert!(
            player.velocity[1] > first_jump_velocity
                || (player.velocity[1] - first_jump_velocity).abs() < 1.0
        );
    }

    #[test]
    fn test_dash_cooldown() {
        let mut dash = Dash::default();

        assert!(dash.can_dash());

        dash.start_dash([1.0, 0.0]);
        assert!(dash.is_dashing);
        assert!(!dash.can_dash());

        dash.update(0.3);
        assert!(!dash.is_dashing);
        assert!(!dash.can_dash());

        dash.update(1.0);
        assert!(dash.can_dash());
    }

    #[test]
    fn test_collectible_coin() {
        let coin = Collectible::coin(10);
        assert_eq!(coin.value, 10);
        assert_eq!(coin.collectible_type, CollectibleType::Coin);
    }

    #[test]
    fn test_collectible_gem() {
        let gem = Collectible::gem(50);
        assert_eq!(gem.value, 50);
        assert_eq!(gem.collectible_type, CollectibleType::Gem);
    }

    #[test]
    fn test_player_inventory() {
        let mut inventory = PlayerInventory::default();

        inventory.add_collectible(&Collectible::coin(10));
        assert_eq!(inventory.coins, 10);
        assert_eq!(inventory.score, 1000);

        inventory.add_collectible(&Collectible::gem(5));
        assert_eq!(inventory.gems, 5);
        assert_eq!(inventory.score, 1500);
    }

    #[test]
    fn test_player_inventory_keys() {
        let mut inventory = PlayerInventory::default();
        let key = Collectible {
            collectible_type: CollectibleType::Key { door_id: 1 },
            value: 1,
            spawn_weight: 0.1,
            bob_speed: 1.0,
            bob_amount: 5.0,
            rotation_speed: 45.0,
            bob_offset: 0.0,
        };

        inventory.add_collectible(&key);
        assert!(inventory.has_key(1));
        assert!(!inventory.has_key(2));

        assert!(inventory.use_key(1));
        assert!(!inventory.has_key(1));
    }

    #[test]
    fn test_falling_platform() {
        let mut platform = FallingPlatform::new(Vec2::new(0.0, 100.0), 0.5, 2.0);

        assert!(!platform.is_triggered);
        platform.trigger();
        assert!(platform.is_triggered);

        platform.update(0.6, 800.0);
        assert!(platform.is_falling);

        platform.update(2.0, 800.0);
        assert!(!platform.is_falling);
        assert!(!platform.is_triggered);
    }

    #[test]
    fn test_level_progress() {
        let mut progress = LevelProgress::default();

        assert!(progress.activate_checkpoint(1));
        assert_eq!(progress.furthest_checkpoint, 1);

        assert!(!progress.activate_checkpoint(0));
        assert_eq!(progress.furthest_checkpoint, 1);
        assert_eq!(progress.current_checkpoint, 0);
    }

    #[test]
    fn test_checkpoint() {
        let checkpoint = Checkpoint::new(1, [100.0, 50.0]);
        assert_eq!(checkpoint.checkpoint_id, 1);
        assert!(!checkpoint.is_activated);
        assert!(!checkpoint.is_level_end);
        assert!(checkpoint.next_level.is_none());

        let level_end = Checkpoint::level_end(10, [500.0, 0.0], "level2".to_string());
        assert!(level_end.is_level_end);
        assert!(level_end.next_level.is_some());
    }

    #[test]
    fn test_active_power_ups() {
        let mut power_ups = ActivePowerUps::default();

        power_ups.add(PowerUpType::SpeedBoost, 5.0);
        assert!(power_ups.has(PowerUpType::SpeedBoost));

        power_ups.update(3.0);
        assert!(power_ups.has(PowerUpType::SpeedBoost));

        power_ups.update(3.0);
        assert!(!power_ups.has(PowerUpType::SpeedBoost));
    }

    #[test]
    fn test_locked_door() {
        let mut door = LockedDoor::new(1);
        let mut inventory = PlayerInventory::default();

        assert!(!door.try_open(&mut inventory));

        inventory.add_collectible(&Collectible {
            collectible_type: CollectibleType::Key { door_id: 1 },
            value: 1,
            spawn_weight: 0.1,
            bob_speed: 1.0,
            bob_amount: 5.0,
            rotation_speed: 45.0,
            bob_offset: 0.0,
        });

        assert!(door.try_open(&mut inventory));
        assert!(door.is_open);
    }

    #[test]
    fn test_camera_bounds() {
        let bounds = CameraBounds::new(0.0, 0.0, 100.0, 100.0);

        let clamped = bounds.clamp(Vec2::new(50.0, 50.0));
        assert!((clamped.x - 50.0).abs() < 0.01);
        assert!((clamped.y - 50.0).abs() < 0.01);

        let clamped = bounds.clamp(Vec2::new(150.0, 150.0));
        assert!((clamped.x - 100.0).abs() < 0.01);
        assert!((clamped.y - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_moving_platform_state() {
        let mut state = MovingPlatformState::new(Vec2::ZERO);
        let start = Vec2::ZERO;
        let end = Vec2::new(100.0, 0.0);

        state.update_linear(0.5, start, end, 100.0);
        assert!((state.current_position.x - 50.0).abs() < 1.0);
    }
}
