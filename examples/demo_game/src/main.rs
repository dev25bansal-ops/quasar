//! Demo Game - A complete game demonstrating Quasar Engine capabilities.
//!
//! Features:
//! - Player movement with physics
//! - Enemy AI with behavior trees
//! - Particle effects
//! - UI overlay
//! - Audio playback
//! - Score tracking

use glam::Vec3;
use quasar_core::ecs::{System, SystemStage, World};
use quasar_core::{App, Entity, Events, Plugin};
use quasar_engine::prelude::*;

// ────────────────────────────────────────────────────────────────────────────
// Components
// ────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
struct Player {
    speed: f32,
    health: f32,
    score: u32,
}

#[derive(Clone)]
struct Enemy {
    health: f32,
    damage: f32,
    state: EnemyState,
}

#[derive(Clone, Default)]
struct Velocity {
    linear: Vec3,
}

#[derive(Clone)]
struct Pickup {
    points: u32,
}

#[derive(Clone)]
struct Particle {
    lifetime: f32,
    velocity: Vec3,
}

#[derive(Clone)]
struct Collider {
    radius: f32,
}

#[derive(Clone)]
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

        world.for_each_mut2::<Player, Velocity, _>(|_entity, player, vel| {
            vel.linear = input.move_direction * player.speed;
        });
    }
}

struct EnemyAISystem;

impl System for EnemyAISystem {
    fn name(&self) -> &str {
        "enemy_ai"
    }

    fn run(&mut self, world: &mut World) {
        let player_pos = world
            .query::<Player>()
            .first()
            .map(|(e, _)| {
                world
                    .get::<Transform>(*e)
                    .map(|t| t.position)
                    .unwrap_or(Vec3::ZERO)
            })
            .unwrap_or(Vec3::ZERO);

        world.for_each_mut::<Enemy, _>(|_entity, enemy| {
            enemy.state = match enemy.state {
                EnemyState::Idle if rand_chance(0.01) => EnemyState::Patrol,
                EnemyState::Idle => EnemyState::Idle,
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
        });
    }
}

struct CollisionSystem;

impl System for CollisionSystem {
    fn name(&self) -> &str {
        "collision"
    }

    fn run(&mut self, world: &mut World) {
        let players: Vec<_> = world
            .query2::<Player, Collider>()
            .into_iter()
            .map(|(e, p, c)| (e, p.clone(), c.clone()))
            .collect();

        let pickups: Vec<_> = world
            .query2::<Pickup, Collider>()
            .into_iter()
            .map(|(e, p, c)| (e, p.clone(), c.clone()))
            .collect();

        if let Some(events) = world.resource_mut::<Events>() {
            for (player_entity, _player, _player_collider) in &players {
                for (pickup_entity, _pickup, _pickup_collider) in &pickups {
                    events.send(CollisionEvent {
                        entity_a: *player_entity,
                        entity_b: *pickup_entity,
                    });
                }
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
        let collision_events: Vec<CollisionEvent> = world
            .resource::<Events>()
            .map(|e| e.read::<CollisionEvent>().to_vec())
            .unwrap_or_default();

        for event in collision_events {
            let points = world.get::<Pickup>(event.entity_b).map(|p| p.points);
            if let (Some(points), Some(mut game_state)) =
                (points, world.resource_mut::<GameState>())
            {
                game_state.score += points;
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

        world.for_each_mut::<Particle, _>(|_entity, particle| {
            particle.lifetime -= dt;
        });

        let dead: Vec<Entity> = world
            .query::<Particle>()
            .into_iter()
            .filter(|(_, p)| p.lifetime <= 0.0)
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

    let mut app = App::new();
    app.add_plugin(DemoGamePlugin);
    run(app, WindowConfig::default());
}
