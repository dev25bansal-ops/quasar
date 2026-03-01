//! # Quasar Core
//!
//! The foundation of the Quasar Engine, providing:
//! - **ECS (Entity-Component-System)**: A lightweight, type-safe ECS framework
//! - **Application lifecycle**: App builder pattern and main loop management
//! - **Events**: A typed event bus for decoupled communication
//! - **Time**: Delta time tracking and fixed timestep support
//! - **Plugins**: Modular engine extension system

pub mod ecs;
pub mod event;
pub mod time;
pub mod plugin;
pub mod app;
pub mod scene;

pub use app::App;
pub use ecs::{World, Entity, Component, EntityBuilder};
pub use event::Events;
pub use time::Time;
pub use plugin::Plugin;
pub use scene::{SceneGraph, Scene};
