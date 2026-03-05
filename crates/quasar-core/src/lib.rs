//! # Quasar Core
//!
//! The foundation of the Quasar Engine, providing:
//! - **ECS (Entity-Component-System)**: A lightweight, type-safe ECS framework
//! - **Application lifecycle**: App builder pattern and main loop management
//! - **Events**: A typed event bus for decoupled communication
//! - **Time**: Delta time tracking and fixed timestep support
//! - **Plugins**: Modular engine extension system
//! - **Animation**: Keyframe-based animation system
//! - **Archetype ECS**: High-performance archetype-based storage
//! - **Parallel Systems**: Concurrent system execution with dependency graph
//! - **Asset Server**: Hot-reload capable asset pipeline
//! - **Networking**: QUIC/UDP game networking with rollback support
//! - **Profiling**: puffin/tracy instrumentation

pub mod animation;
pub mod app;
pub mod asset;
pub mod asset_server;
pub mod ecs;
pub mod error;
pub mod event;
pub mod network;
pub mod plugin;
pub mod profiler;
pub mod scene;
pub mod scene_serde;
pub mod time;

pub use animation::{
    AnimationClip, AnimationPlayer, AnimationPlugin, AnimationResource, AnimationState,
    SkeletalAnimationClip, TransformKeyframe,
};
pub use app::{App, TimeSnapshot};
pub use asset::{Asset, AssetHandle, AssetManager, AsyncHandle, AsyncState, LoadingState};
pub use asset_server::{
    AssetError, AssetEvent, AssetHandle as NetworkAssetHandle, AssetPlugin, AssetServer,
};
pub use ecs::{Component, Entity, EntityBuilder, World};
pub use error::{QuasarError, QuasarResult};
pub use event::Events;
pub use network::{NetworkConfig, NetworkPlugin, NetworkReplication, NetworkRole, NetworkState};
pub use plugin::Plugin;
pub use profiler::{FrameStats, Profiler, ProfilerPlugin};
pub use scene::{Scene, SceneGraph};
pub use scene_serde::{EntityData, SceneData};
pub use time::Time;
