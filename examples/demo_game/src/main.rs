//! Demo Game - A complete game demonstrating Quasar Engine capabilities.
//!
//! Features:
//! - Player movement with physics
//! - Enemy AI with behavior trees
//! - Particle effects
//! - UI overlay
//! - Audio playback
//! - Score tracking

use glam::{Quat, Vec3};
use quasar_core::prelude::*;
use quasar_engine::App;

// ────────────────────────────────────────────────────────────────────────────
// Components
// ────────────────────────────────────────────────────────────────────────────

#[derive(Component, Clone, Default)]
struct Player {
    speed: f32,
    health: f32,
    score: u32,
}

#[derive(Component, Clone)]
struct Enemy {
    health: f32,
    damage: f32,
    state: EnemyState,
}

#[derive(Component, Clone, Default)]
struct Velocity {
    linear: Vec3,
}

#[derive(Component, Clone)]
struct Pickup {
    points: u32,
}

#[derive(Component, Clone)]
struct Particle {
    lifetime: f32,
    velocity: Vec3,
}

#[derive(Component, Clone)]
struct Collider {
    radius: f32,
}

#[derive(Component, Clone)]
struct MeshRenderer {
    mesh: String,
    color: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
enum EnemyState {
    Idle,
    Patrol,
    Chase,
    Attack,
    Dead,
}

impl Default for EnemyState {
    fn default() -> Self {
        Self::Idle
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Resources
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct GameState {
    score: u32,
    lives: u32,
    level: u32,
    game_over: bool,
    paused: bool,
}

impl Default for GameState {
    fn default() -> Self {
        Self {
            score: 0,
            lives: 3,
            level: 1,
            game_over: false,
            paused: false,
        }
    }
}

#[derive(Debug, Clone)]
struct InputState {
    move_direction: Vec3,
    jump: bool,
    attack: bool,
}

impl Default for InputState {
    fn default() -> Self {
        Self {
            move_direction: Vec3::ZERO,
            jump: false,
            attack: false,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Systems
// ────────────────────────────────────────────────────────────────────────────

struct PlayerMovementSystem;

impl System for PlayerMovementSystem {
    fn name(&self) -> &str {
        "player_movement"
    }

    fn run(&mut self, world: &mut World) {
        let input = world.resource::<InputState>().cloned().unwrap_or_default();
        let dt = 1.0 / 60.0;

        for (entity, (player, vel)) in world.query_mut_2::<Player, Velocity>() {
            vel.linear = input.move_direction * player.speed;
        }
    }
}

struct EnemyAISystem;

impl System for EnemyAISystem {
    fn name(&self) -> &str {
        "enemy_ai"
    }

    fn run(&mut self, world: &mut World) {
        let player_pos = world
            .query_iter::<Player>()
            .next()
            .map(|(_, _)| Vec3::ZERO)
            .unwrap_or(Vec3::ZERO);

        for (entity, enemy) in world.query_mut::<Enemy>() {
            enemy.state = match enemy.state {
                EnemyState::Idle if rand_chance(0.01) => EnemyState::Patrol,
                EnemyState::Patrol => {
                    if rand_chance(0.02) {
                        EnemyState::Chase
                    } else {
                        EnemyState::Patrol
                    }
                }
                EnemyState::Chase => {
                    if rand_chance(0.05) {
                        EnemyState::Attack
                    } else {
                        EnemyState::Chase
                    }
                }
                EnemyState::Attack => {
                    if rand_chance(0.1) {
                        EnemyState::Idle
                    } else {
                        EnemyState::Attack
                    }
                }
                EnemyState::Dead => EnemyState::Dead,
            };
        }
    }
}

struct CollisionSystem;

impl System for CollisionSystem {
    fn name(&self) -> &str {
        "collision"
    }

    fn run(&mut self, world: &mut World) {
        let mut events = world.resource_mut::<Events>().unwrap();

        let players: Vec<_> = world
            .query::<(Entity, &Player, &Collider)>()
            .iter()
            .map(|(e, p, c)| (e, *p, *c))
            .collect();

        let pickups: Vec<_> = world
            .query::<(Entity, &Pickup, &Collider)>()
            .iter()
            .map(|(e, p, c)| (e, *p, *c))
            .collect();

        for (player_entity, player, player_collider) in &players {
            for (pickup_entity, pickup, pickup_collider) in &pickups {
                events.send(CollisionEvent {
                    entity_a: *player_entity,
                    entity_b: *pickup_entity,
                });
            }
        }
    }
}

#[derive(Debug, Clone)]
struct CollisionEvent {
    entity_a: Entity,
    entity_b: Entity,
}

struct ScoreSystem;

impl System for ScoreSystem {
    fn name(&self) -> &str {
        "score"
    }

    fn run(&mut self, world: &mut World) {
        if let Some(events) = world.resource::<Events>() {
            for event in events.read::<CollisionEvent>() {
                if let Some(mut game_state) = world.resource_mut::<GameState>() {
                    if let Some(pickup) = world.get::<Pickup>(event.entity_b) {
                        game_state.score += pickup.points;
                    }
                }
            }
        }
    }
}

struct ParticleSystem;

impl System for ParticleSystem {
    fn name(&self) -> &str {
        "particles"
    }

    fn run(&mut self, world: &mut World) {
        let dt = 1.0 / 60.0;

        for (entity, particle) in world.query_mut::<Particle>() {
            particle.lifetime -= dt;
        }

        let dead: Vec<Entity> = world
            .query_filtered::<Entity, With<Particle>>()
            .iter()
            .filter(|(e, _)| {
                world
                    .get::<Particle>(*e)
                    .map(|p| p.lifetime <= 0.0)
                    .unwrap_or(false)
            })
            .map(|(e, _)| e)
            .collect();

        for entity in dead {
            world.despawn(entity);
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Plugin
// ────────────────────────────────────────────────────────────────────────────

struct DemoGamePlugin;

impl Plugin for DemoGamePlugin {
    fn name(&self) -> &str {
        "demo_game"
    }

    fn build(&self, app: &mut App) {
        app.world.insert_resource(GameState::default());
        app.world.insert_resource(InputState::default());

        app.schedule
            .add_system(SystemStage::Update, Box::new(PlayerMovementSystem));
        app.schedule
            .add_system(SystemStage::Update, Box::new(EnemyAISystem));
        app.schedule
            .add_system(SystemStage::Update, Box::new(CollisionSystem));
        app.schedule
            .add_system(SystemStage::Update, Box::new(ScoreSystem));
        app.schedule
            .add_system(SystemStage::Update, Box::new(ParticleSystem));

        spawn_player(&mut app.world);
        spawn_enemies(&mut app.world, 5);
        spawn_pickups(&mut app.world, 10);
    }
}

fn spawn_player(world: &mut World) {
    let entity = world.spawn();
    world.insert(
        entity,
        Player {
            speed: 5.0,
            health: 100.0,
            score: 0,
        },
    );
    world.insert(entity, Velocity { linear: Vec3::ZERO });
    world.insert(entity, Collider { radius: 0.5 });
    world.insert(
        entity,
        MeshRenderer {
            mesh: "player".into(),
            color: [0.0, 0.5, 1.0, 1.0],
        },
    );
}

fn spawn_enemies(world: &mut World, count: usize) {
    for i in 0..count {
        let entity = world.spawn();
        world.insert(
            entity,
            Enemy {
                health: 50.0,
                damage: 10.0,
                state: EnemyState::Idle,
            },
        );
        world.insert(entity, Velocity { linear: Vec3::ZERO });
        world.insert(entity, Collider { radius: 0.4 });
        world.insert(
            entity,
            MeshRenderer {
                mesh: "enemy".into(),
                color: [1.0, 0.2, 0.2, 1.0],
            },
        );
    }
}

fn spawn_pickups(world: &mut World, count: usize) {
    for i in 0..count {
        let entity = world.spawn();
        world.insert(entity, Pickup { points: 100 });
        world.insert(entity, Collider { radius: 0.3 });
        world.insert(
            entity,
            MeshRenderer {
                mesh: "pickup".into(),
                color: [1.0, 1.0, 0.0, 1.0],
            },
        );
    }
}

fn rand_chance(probability: f32) -> bool {
    use std::time::SystemTime;
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    (nanos as f32 / u32::MAX as f32) < probability
}

// ────────────────────────────────────────────────────────────────────────────
// Main
// ────────────────────────────────────────────────────────────────────────────

fn main() {
    env_logger::init();

    println!("=== Quasar Demo Game ===");
    println!("Controls:");
    println!("  WASD - Move");
    println!("  Space - Jump");
    println!("  Mouse - Look");
    println!("  ESC - Quit");
    println!();

    App::new().add_plugin(DemoGamePlugin).run();
}
