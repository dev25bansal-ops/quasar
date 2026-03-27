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
//! - **AI**: Behavior tree system for game AI
//! - **Localization**: Internationalization (i18n) support

#![deny(clippy::unwrap_used, clippy::expect_used)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod ai;
pub mod animation;
pub mod app;
pub mod asset;
pub mod asset_server;
pub mod delta_compression;
pub mod ecs;
pub mod error;
pub mod event;
pub mod interest;
pub mod localization;
pub mod navigation;
#[cfg(feature = "quinn-transport")]
pub mod net_quinn;
pub mod network;
pub mod plugin;
pub mod prediction;
pub mod prefab;
pub mod profiler;
pub mod reflect;
pub mod save_load;
pub mod scene;
pub mod scene_serde;
pub mod time;
#[cfg(target_arch = "wasm32")]
pub mod wasm_platform;

pub use ai::{
    BehaviorTree, BehaviorTreePlugin, BehaviorTreeRunner, BehaviorTreeSystem, Blackboard,
    BlackboardValue, Node, NodeResult,
};
pub use animation::{
    AnimationBlendTree, AnimationClip, AnimationPlayer, AnimationPlugin, AnimationResource,
    AnimationState, AnimationStateMachine, AnimationStateMachineSystem, AnimationStateNode,
    AnimationTransition, BlendTreeNode, SkeletalAnimationClip, TransformKeyframe,
    TransitionCondition,
};
pub use app::{simulation_active, App, SimulationState, TimeSnapshot};
pub use asset_server::{
    AssetError, AssetEvent, AssetHandle as NetworkAssetHandle, AssetPlugin, AssetReloadSystem,
    AssetReloadedEvent, AssetServer, HotReloadHandlerSystem, ReloadKind,
};
// Unified asset manager is accessible via AssetServer::manager()
pub use asset::{
    Asset, AssetDepGraph, AssetHandle, AssetManager, AsyncHandle, AsyncState, ContentHash,
    LoadingState,
};
pub use ecs::{
    flush_commands, Component, Entity, EntityBuilder, QueryFilter, QueryState, World, WorldQuery,
};
pub use error::{QuasarError, QuasarResult};
pub use event::{Events, EventsChannel};
pub use interest::InterestManager;
pub use localization::{
    plural_category, Localization, LocalizationPlugin, LocalizationResource, LocalizedString,
    PluralForms, StringTable,
};
pub use navigation::{
    find_path, path_to_waypoints, DynamicNavMesh, NavMesh, NavMeshAgent, NavMeshAgentSystem,
    NavObstacle, NavObstacleShape, NavPoly,
};
pub use network::{
    replication_system, rollback_system, ConnectionMetrics, DeltaCompressor, DeltaFlags,
    EncodedDelta, HistoryBuffer, InputHistory, LagCompensationManager, Misprediction,
    NetworkConfig, NetworkMetrics, NetworkPlugin, NetworkReplication, NetworkRole, NetworkState,
    PendingServerSnapshot, QuicChannel, QuicConfig, QuicEvent, QuicTransport, QuicTransportBackend,
    RelayServer, RelayServerConfig, RelaySession, ReplicateDescriptor, Replicated, ReplicatedField,
    ReplicationMode, ReplicationResource, SendChannel, SnapshotInterpolation, TickAccumulator,
    Transport, TransportEvent, TransportProtocol, TransportType, UdpTransport,
};
pub use plugin::Plugin;
pub use prefab::{
    apply_overrides, diff_instance_transform, instantiate_prefab, is_field_overridden,
    propagate_prefab_changes, ComponentOverride, OverrideHandlerFn, OverrideRegistry, Prefab,
    PrefabEntity, PrefabFieldDiff, PrefabInstance, PrefabLibrary, PrefabMeshTag, PrefabProperties,
    PrefabProperty,
};
pub use profiler::{AllocTracker, FrameBudget, FrameStats, Profiler, ProfilerPlugin};
pub use save_load::{capture_game_save, load_game_save, GameSave, SaveMeta, SavedEntity};
pub use scene::{Scene, SceneGraph};
pub use scene_serde::{EntityData, SceneData};
pub use time::{FixedUpdateAccumulator, Time};
