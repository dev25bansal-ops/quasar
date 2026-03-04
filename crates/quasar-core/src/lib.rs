//! # Quasar Core
//!
//! The foundation of the Quasar Engine, providing:
//! - **ECS (Entity-Component-System)**: A lightweight, type-safe ECS framework
//! - **Application lifecycle**: App builder pattern and main loop management
//! - **Events**: A typed event bus for decoupled communication
//! - **Time**: Delta time tracking and fixed timestep support
//! - **Plugins**: Modular engine extension system
//! - **Animation**: Keyframe-based animation system

pub mod animation;
pub mod app;
pub mod asset;
pub mod ecs;
pub mod error;
pub mod event;
pub mod plugin;
pub mod scene;
pub mod scene_serde;
pub mod time;

pub use animation::{
    AnimationClip, AnimationPlayer, AnimationPlugin, AnimationResource, AnimationState,
    SkeletalAnimationClip, TransformKeyframe,
};
pub use app::{App, TimeSnapshot};
pub use asset::{Asset, AssetHandle, AssetManager, LoadingState, AsyncState, AsyncHandle};
pub use ecs::{Component, Entity, EntityBuilder, World};
pub use error::{QuasarError, QuasarResult};
pub use event::Events;
pub use plugin::Plugin;
pub use scene::{Scene, SceneGraph};
pub use scene_serde::{EntityData, SceneData};
pub use time::Time;
