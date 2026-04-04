//! Game Templates for Quasar Engine
//!
//! Ready-to-use starter kits for common game genres:
//! - **FPS**: First-person shooter with weapons, health, ammo
//! - **RPG**: Role-playing with inventory, quests, leveling
//! - **RTS**: Real-time strategy with units, buildings, resources
//! - **Platformer**: 2D/3D platformer with jumping, collectibles
//!
//! Each template provides:
//! - Pre-built components and systems
//! - Example entity spawning
//! - Default input handling
//! - UI integration points

use glam::Vec3;
use quasar_core::ecs::{System, World};
use quasar_core::Entity;
use serde::{Deserialize, Serialize};

/// Common transform component used across templates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateTransform {
    pub position: Vec3,
    pub rotation: glam::Quat,
    pub scale: Vec3,
}

impl Default for TemplateTransform {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            rotation: glam::Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }
}

/// Common velocity component.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TemplateVelocity {
    pub linear: Vec3,
    pub angular: Vec3,
}

/// Health component used by damageable entities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Health {
    pub current: f32,
    pub max: f32,
    pub regen_rate: f32,
    pub invulnerable: bool,
    pub invulnerability_time: f32,
}

impl Health {
    pub fn new(max: f32) -> Self {
        Self {
            current: max,
            max,
            regen_rate: 0.0,
            invulnerable: false,
            invulnerability_time: 0.0,
        }
    }

    pub fn is_alive(&self) -> bool {
        self.current > 0.0
    }

    pub fn take_damage(&mut self, amount: f32) -> bool {
        if self.invulnerable {
            return false;
        }
        self.current = (self.current - amount).max(0.0);
        self.current <= 0.0
    }

    pub fn heal(&mut self, amount: f32) {
        self.current = (self.current + amount).min(self.max);
    }
}

impl Default for Health {
    fn default() -> Self {
        Self::new(100.0)
    }
}

/// Team identifier for faction-based games.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Team {
    #[default]
    Neutral,
    Player,
    Enemy,
    Ally,
    Team1,
    Team2,
    Team3,
    Team4,
}

impl Team {
    pub fn is_hostile_to(&self, other: &Team) -> bool {
        match (self, other) {
            (Team::Player, Team::Enemy) => true,
            (Team::Enemy, Team::Player) => true,
            (Team::Team1, Team::Team2) => true,
            (Team::Team2, Team::Team1) => true,
            _ => false,
        }
    }
}

/// Collider shape for physics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ColliderShape {
    Sphere { radius: f32 },
    Box { half_extents: Vec3 },
    Capsule { radius: f32, height: f32 },
}

impl Default for ColliderShape {
    fn default() -> Self {
        Self::Sphere { radius: 0.5 }
    }
}

/// Timer component for cooldowns and durations.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Timer {
    pub duration: f32,
    pub elapsed: f32,
    pub repeating: bool,
    pub finished: bool,
}

impl Timer {
    pub fn new(duration: f32) -> Self {
        Self {
            duration,
            elapsed: 0.0,
            repeating: false,
            finished: false,
        }
    }

    pub fn repeating(duration: f32) -> Self {
        Self {
            duration,
            elapsed: 0.0,
            repeating: true,
            finished: false,
        }
    }

    pub fn tick(&mut self, dt: f32) -> bool {
        if self.finished && !self.repeating {
            return true;
        }
        self.elapsed += dt;
        if self.elapsed >= self.duration {
            self.finished = true;
            if self.repeating {
                self.elapsed -= self.duration;
            }
            return true;
        }
        false
    }

    pub fn reset(&mut self) {
        self.elapsed = 0.0;
        self.finished = false;
    }

    pub fn progress(&self) -> f32 {
        if self.duration <= 0.0 {
            return 1.0;
        }
        (self.elapsed / self.duration).min(1.0)
    }
}

/// Input state for player control.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InputState {
    pub move_axis: Vec3,
    pub look_axis: Vec2,
    pub jump: bool,
    pub attack: bool,
    pub interact: bool,
    pub sprint: bool,
    pub crouch: bool,
    pub reload: bool,
    pub ability1: bool,
    pub ability2: bool,
    pub ability3: bool,
    pub ability4: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

/// Base trait for game templates.
pub trait GameTemplate {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn setup(&self, world: &mut World);
    fn get_systems(&self) -> Vec<Box<dyn System>>;
}

/// Utility function to spawn entity with default transform.
pub fn spawn_entity(world: &mut World) -> Entity {
    world.spawn()
}

/// Utility function to spawn entity at position.
pub fn spawn_entity_at(world: &mut World, position: Vec3) -> Entity {
    let entity = world.spawn();
    world.insert(
        entity,
        TemplateTransform {
            position,
            ..Default::default()
        },
    );
    entity
}

// Module declarations must come after the types they use
pub mod fps;
pub mod platformer;
pub mod rpg;
pub mod rts;

pub mod prelude {
    pub use crate::fps::*;
    pub use crate::platformer::*;
    pub use crate::rpg::*;
    pub use crate::rts::*;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_take_damage() {
        let mut health = Health::new(100.0);
        assert!(health.is_alive());
        health.take_damage(30.0);
        assert_eq!(health.current, 70.0);
    }

    #[test]
    fn health_death() {
        let mut health = Health::new(50.0);
        let died = health.take_damage(60.0);
        assert!(died);
        assert!(!health.is_alive());
    }

    #[test]
    fn health_invulnerable() {
        let mut health = Health::new(100.0);
        health.invulnerable = true;
        health.take_damage(50.0);
        assert_eq!(health.current, 100.0);
    }

    #[test]
    fn health_heal() {
        let mut health = Health::new(100.0);
        health.current = 50.0;
        health.heal(30.0);
        assert_eq!(health.current, 80.0);
        health.heal(50.0);
        assert_eq!(health.current, 100.0);
    }

    #[test]
    fn team_hostility() {
        assert!(Team::Player.is_hostile_to(&Team::Enemy));
        assert!(Team::Enemy.is_hostile_to(&Team::Player));
        assert!(!Team::Player.is_hostile_to(&Team::Player));
    }

    #[test]
    fn timer_tick() {
        let mut timer = Timer::new(1.0);
        assert!(!timer.tick(0.5));
        assert!(!timer.finished);
        assert!(timer.tick(0.6));
        assert!(timer.finished);
    }

    #[test]
    fn timer_repeating() {
        let mut timer = Timer::repeating(1.0);
        assert!(timer.tick(1.0));
        assert!(timer.finished);
        timer.finished = false;
        assert!(timer.tick(1.0));
        assert!(timer.finished);
    }

    #[test]
    fn timer_progress() {
        let mut timer = Timer::new(2.0);
        timer.elapsed = 1.0;
        assert!((timer.progress() - 0.5).abs() < 0.001);
    }

    #[test]
    fn timer_reset() {
        let mut timer = Timer::new(1.0);
        timer.tick(1.0);
        assert!(timer.finished);
        timer.reset();
        assert!(!timer.finished);
        assert_eq!(timer.elapsed, 0.0);
    }

    #[test]
    fn transform_default() {
        let t = TemplateTransform::default();
        assert_eq!(t.position, Vec3::ZERO);
        assert_eq!(t.scale, Vec3::ONE);
    }
}
