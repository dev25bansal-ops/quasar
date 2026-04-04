//! First-Person Shooter Template
//!
//! Provides components and systems for FPS games:
//! - Player controller with WASD movement and mouse look
//! - Weapon system with multiple weapon types
//! - Ammo and reload mechanics
//! - Health and damage system
//! - Enemy AI with basic combat behavior

use glam::Vec3;
use quasar_core::ecs::{System, World};
use serde::{Deserialize, Serialize};

use super::{Health, InputState, Team, TemplateTransform, TemplateVelocity};

pub mod prelude {
    pub use super::{
        Ammo, EnemyAI, FpsCamera, FpsPlayer, FpsPlugin, Weapon, WeaponConfig, WeaponSlot,
        WeaponSystem, WeaponType,
    };
}

// ─────────────────────────────────────────────────────────────────────────────
// Components
// ─────────────────────────────────────────────────────────────────────────────

/// FPS player controller.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FpsPlayer {
    pub move_speed: f32,
    pub sprint_multiplier: f32,
    pub jump_force: f32,
    pub mouse_sensitivity: f32,
    pub is_grounded: bool,
    pub is_sprinting: bool,
    pub is_crouching: bool,
    pub crouch_height: f32,
    pub stand_height: f32,
}

impl Default for FpsPlayer {
    fn default() -> Self {
        Self {
            move_speed: 5.0,
            sprint_multiplier: 1.5,
            jump_force: 8.0,
            mouse_sensitivity: 0.002,
            is_grounded: true,
            is_sprinting: false,
            is_crouching: false,
            crouch_height: 0.6,
            stand_height: 1.8,
        }
    }
}

/// Camera controller for FPS view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FpsCamera {
    pub pitch: f32,
    pub yaw: f32,
    pub fov: f32,
    pub near: f32,
    pub far: f32,
    pub bob_time: f32,
    pub bob_amount: f32,
}

impl Default for FpsCamera {
    fn default() -> Self {
        Self {
            pitch: 0.0,
            yaw: 0.0,
            fov: 75.0,
            near: 0.1,
            far: 1000.0,
            bob_time: 0.0,
            bob_amount: 0.05,
        }
    }
}

/// Weapon slot for inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WeaponSlot {
    Primary,
    Secondary,
    Melee,
    Explosive,
}

/// Weapon type enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WeaponType {
    Pistol,
    Rifle,
    Shotgun,
    Sniper,
    SMG,
    LMG,
    RocketLauncher,
    Melee,
}

impl WeaponType {
    pub fn config(&self) -> WeaponConfig {
        match self {
            WeaponType::Pistol => WeaponConfig {
                damage: 25.0,
                fire_rate: 0.25,
                magazine_size: 12,
                reload_time: 1.5,
                spread: 0.02,
                range: 50.0,
                recoil: 0.5,
            },
            WeaponType::Rifle => WeaponConfig {
                damage: 30.0,
                fire_rate: 0.1,
                magazine_size: 30,
                reload_time: 2.5,
                spread: 0.03,
                range: 100.0,
                recoil: 0.7,
            },
            WeaponType::Shotgun => WeaponConfig {
                damage: 15.0,
                fire_rate: 0.8,
                magazine_size: 8,
                reload_time: 3.0,
                spread: 0.15,
                range: 25.0,
                recoil: 1.2,
            },
            WeaponType::Sniper => WeaponConfig {
                damage: 100.0,
                fire_rate: 1.5,
                magazine_size: 5,
                reload_time: 3.5,
                spread: 0.001,
                range: 500.0,
                recoil: 2.0,
            },
            WeaponType::SMG => WeaponConfig {
                damage: 18.0,
                fire_rate: 0.07,
                magazine_size: 35,
                reload_time: 2.0,
                spread: 0.05,
                range: 40.0,
                recoil: 0.4,
            },
            WeaponType::LMG => WeaponConfig {
                damage: 28.0,
                fire_rate: 0.08,
                magazine_size: 100,
                reload_time: 5.0,
                spread: 0.06,
                range: 80.0,
                recoil: 0.6,
            },
            WeaponType::RocketLauncher => WeaponConfig {
                damage: 150.0,
                fire_rate: 2.0,
                magazine_size: 1,
                reload_time: 4.0,
                spread: 0.01,
                range: 200.0,
                recoil: 2.5,
            },
            WeaponType::Melee => WeaponConfig {
                damage: 50.0,
                fire_rate: 0.5,
                magazine_size: 0,
                reload_time: 0.0,
                spread: 0.0,
                range: 2.0,
                recoil: 0.0,
            },
        }
    }
}

/// Weapon configuration parameters.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct WeaponConfig {
    pub damage: f32,
    pub fire_rate: f32,
    pub magazine_size: u32,
    pub reload_time: f32,
    pub spread: f32,
    pub range: f32,
    pub recoil: f32,
}

/// Weapon instance with current state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Weapon {
    pub weapon_type: WeaponType,
    pub config: WeaponConfig,
    pub ammo_in_magazine: u32,
    pub is_reloading: bool,
    pub reload_timer: f32,
    pub fire_timer: f32,
}

impl Weapon {
    pub fn new(weapon_type: WeaponType) -> Self {
        let config = weapon_type.config();
        Self {
            weapon_type,
            config,
            ammo_in_magazine: config.magazine_size,
            is_reloading: false,
            reload_timer: 0.0,
            fire_timer: 0.0,
        }
    }

    pub fn can_fire(&self) -> bool {
        !self.is_reloading && self.fire_timer <= 0.0 && self.ammo_in_magazine > 0
    }

    pub fn fire(&mut self) -> bool {
        if self.can_fire() {
            self.ammo_in_magazine -= 1;
            self.fire_timer = self.config.fire_rate;
            return true;
        }
        false
    }

    pub fn start_reload(&mut self) {
        if !self.is_reloading && self.ammo_in_magazine < self.config.magazine_size {
            self.is_reloading = true;
            self.reload_timer = self.config.reload_time;
        }
    }

    pub fn tick(&mut self, dt: f32) -> bool {
        let mut finished_reload = false;
        if self.fire_timer > 0.0 {
            self.fire_timer -= dt;
        }
        if self.is_reloading {
            self.reload_timer -= dt;
            if self.reload_timer <= 0.0 {
                self.ammo_in_magazine = self.config.magazine_size;
                self.is_reloading = false;
                finished_reload = true;
            }
        }
        finished_reload
    }
}

/// Ammo reserve for player.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Ammo {
    pub reserve: std::collections::HashMap<WeaponType, u32>,
}

impl Ammo {
    pub fn get(&self, weapon_type: WeaponType) -> u32 {
        self.reserve.get(&weapon_type).copied().unwrap_or(0)
    }

    pub fn add(&mut self, weapon_type: WeaponType, amount: u32) {
        *self.reserve.entry(weapon_type).or_insert(0) += amount;
    }

    pub fn consume(&mut self, weapon_type: WeaponType, amount: u32) -> u32 {
        let current = self.get(weapon_type);
        let consumed = amount.min(current);
        if let Some(reserve) = self.reserve.get_mut(&weapon_type) {
            *reserve -= consumed;
        }
        consumed
    }
}

/// Enemy AI for FPS combat.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnemyAI {
    pub state: EnemyState,
    pub target: Option<u64>,
    pub detection_range: f32,
    pub attack_range: f32,
    pub attack_cooldown: f32,
    pub attack_timer: f32,
    pub patrol_points: Vec<Vec3>,
    pub current_patrol_index: usize,
    pub move_speed: f32,
    pub turn_speed: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum EnemyState {
    #[default]
    Idle,
    Patrol,
    Chase,
    Attack,
    Flee,
    Dead,
}

impl Default for EnemyAI {
    fn default() -> Self {
        Self {
            state: EnemyState::Idle,
            target: None,
            detection_range: 30.0,
            attack_range: 15.0,
            attack_cooldown: 1.0,
            attack_timer: 0.0,
            patrol_points: Vec::new(),
            current_patrol_index: 0,
            move_speed: 3.0,
            turn_speed: 5.0,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Systems
// ─────────────────────────────────────────────────────────────────────────────

pub struct FpsMovementSystem;

impl System for FpsMovementSystem {
    fn name(&self) -> &str {
        "fps_movement"
    }

    fn run(&mut self, world: &mut World) {
        let input = world.resource::<InputState>().cloned().unwrap_or_default();

        world.for_each_mut2::<FpsPlayer, TemplateVelocity, _>(|_entity, player, vel| {
            let speed = if player.is_sprinting {
                player.move_speed * player.sprint_multiplier
            } else if player.is_crouching {
                player.move_speed * 0.5
            } else {
                player.move_speed
            };

            vel.linear.x = input.move_axis.x * speed;
            vel.linear.z = -input.move_axis.z * speed;

            if input.jump && player.is_grounded {
                vel.linear.y = player.jump_force;
                player.is_grounded = false;
            }
        });
    }
}

pub struct FpsCameraSystem;

impl System for FpsCameraSystem {
    fn name(&self) -> &str {
        "fps_camera"
    }

    fn run(&mut self, world: &mut World) {
        let input = world.resource::<InputState>().cloned().unwrap_or_default();
        let dt = 1.0 / 60.0;

        world.for_each_mut::<FpsCamera, _>(|_entity, camera| {
            camera.yaw += input.look_axis.x * 0.1;
            camera.pitch = (camera.pitch + input.look_axis.y * 0.1).clamp(-1.54, 1.54);
            camera.bob_time += dt;
        });
    }
}

pub struct WeaponSystem;

impl System for WeaponSystem {
    fn name(&self) -> &str {
        "weapon"
    }

    fn run(&mut self, world: &mut World) {
        let input = world.resource::<InputState>().cloned().unwrap_or_default();
        let dt = 1.0 / 60.0;

        world.for_each_mut::<Weapon, _>(|_entity, weapon| {
            weapon.tick(dt);

            if input.attack && weapon.can_fire() {
                weapon.fire();
            }

            if input.reload && !weapon.is_reloading {
                weapon.start_reload();
            }
        });
    }
}

pub struct EnemyAISystem;

impl System for EnemyAISystem {
    fn name(&self) -> &str {
        "enemy_ai"
    }

    fn run(&mut self, world: &mut World) {
        let dt = 1.0 / 60.0;

        world.for_each_mut::<EnemyAI, _>(|_entity, ai| {
            ai.attack_timer -= dt;

            match ai.state {
                EnemyState::Idle => {
                    if ai.patrol_points.len() > 1 {
                        ai.state = EnemyState::Patrol;
                    }
                }
                EnemyState::Patrol => {
                    if ai.patrol_points.len() > 1 {
                        ai.current_patrol_index =
                            (ai.current_patrol_index + 1) % ai.patrol_points.len();
                    }
                }
                EnemyState::Chase => {
                    if ai.attack_timer <= 0.0 {
                        ai.state = EnemyState::Attack;
                    }
                }
                EnemyState::Attack => {
                    ai.attack_timer = ai.attack_cooldown;
                    ai.state = EnemyState::Chase;
                }
                EnemyState::Flee => {}
                EnemyState::Dead => {}
            }
        });
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Plugin
// ─────────────────────────────────────────────────────────────────────────────

pub struct FpsPlugin;

impl quasar_core::Plugin for FpsPlugin {
    fn name(&self) -> &str {
        "fps_template"
    }

    fn build(&self, app: &mut quasar_core::App) {
        app.world.insert_resource(InputState::default());

        app.schedule.add_system(
            quasar_core::ecs::SystemStage::Update,
            Box::new(FpsMovementSystem),
        );
        app.schedule.add_system(
            quasar_core::ecs::SystemStage::Update,
            Box::new(FpsCameraSystem),
        );
        app.schedule.add_system(
            quasar_core::ecs::SystemStage::Update,
            Box::new(WeaponSystem),
        );
        app.schedule.add_system(
            quasar_core::ecs::SystemStage::Update,
            Box::new(EnemyAISystem),
        );

        spawn_fps_player(&mut app.world);
    }
}

fn spawn_fps_player(world: &mut World) {
    let entity = world.spawn();
    world.insert(entity, FpsPlayer::default());
    world.insert(entity, FpsCamera::default());
    world.insert(entity, TemplateTransform::default());
    world.insert(entity, TemplateVelocity::default());
    world.insert(entity, Health::new(100.0));
    world.insert(entity, Team::Player);
    world.insert(entity, Weapon::new(WeaponType::Rifle));
}

pub fn spawn_fps_enemy(world: &mut World, position: Vec3) {
    let entity = world.spawn();
    world.insert(entity, EnemyAI::default());
    world.insert(
        entity,
        TemplateTransform {
            position,
            ..Default::default()
        },
    );
    world.insert(entity, TemplateVelocity::default());
    world.insert(entity, Health::new(50.0));
    world.insert(entity, Team::Enemy);
}

pub fn spawn_fps_weapon_pickup(world: &mut World, position: Vec3, weapon_type: WeaponType) {
    let entity = world.spawn();
    world.insert(
        entity,
        TemplateTransform {
            position,
            ..Default::default()
        },
    );
    world.insert(entity, Weapon::new(weapon_type));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weapon_fire() {
        let mut weapon = Weapon::new(WeaponType::Rifle);
        assert!(weapon.can_fire());
        assert!(weapon.fire());
        assert_eq!(weapon.ammo_in_magazine, 29);
    }

    #[test]
    fn weapon_empty() {
        let mut weapon = Weapon::new(WeaponType::Rifle);
        weapon.ammo_in_magazine = 0;
        assert!(!weapon.can_fire());
        assert!(!weapon.fire());
    }

    #[test]
    fn weapon_reload() {
        let mut weapon = Weapon::new(WeaponType::Rifle);
        weapon.ammo_in_magazine = 10;
        weapon.start_reload();
        assert!(weapon.is_reloading);
    }

    #[test]
    fn weapon_config_damage() {
        assert_eq!(WeaponType::Pistol.config().damage, 25.0);
        assert_eq!(WeaponType::Sniper.config().damage, 100.0);
    }

    #[test]
    fn ammo_management() {
        let mut ammo = Ammo::default();
        ammo.add(WeaponType::Rifle, 60);
        assert_eq!(ammo.get(WeaponType::Rifle), 60);
        ammo.consume(WeaponType::Rifle, 20);
        assert_eq!(ammo.get(WeaponType::Rifle), 40);
    }

    #[test]
    fn fps_player_default() {
        let player = FpsPlayer::default();
        assert!(player.is_grounded);
        assert!(!player.is_sprinting);
    }

    #[test]
    fn enemy_ai_default() {
        let ai = EnemyAI::default();
        assert_eq!(ai.state, EnemyState::Idle);
        assert_eq!(ai.detection_range, 30.0);
    }
}
