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

#![deny(clippy::unwrap_used, clippy::expect_used)]

pub mod animation;
pub mod app;
pub mod asset;
pub mod asset_server;
pub mod delta_compression;
pub mod ecs;
pub mod error;
pub mod event;
pub mod interest;
pub mod navigation;
pub mod network;
#[cfg(feature = "quinn-transport")]
pub mod net_quinn;
pub mod plugin;
pub mod prediction;
pub mod prefab;
pub mod profiler;
pub mod reflect;
pub mod save_load;
pub mod scene;
pub mod scene_serde;
pub mod time;

pub use animation::{
    AnimationBlendTree, AnimationClip, AnimationPlayer, AnimationPlugin, AnimationResource,
    AnimationState, AnimationStateMachine, AnimationStateMachineSystem, AnimationStateNode,
    AnimationTransition, BlendTreeNode, SkeletalAnimationClip, TransformKeyframe,
    TransitionCondition,
};
pub use app::{App, SimulationState, TimeSnapshot, simulation_active};
pub use asset_server::{
    AssetError, AssetEvent, AssetHandle as NetworkAssetHandle, AssetPlugin, AssetReloadSystem,
    AssetReloadedEvent, AssetServer, HotReloadHandlerSystem, ReloadKind,
};
// Unified asset manager is accessible via AssetServer::manager()
pub use asset::{Asset, AssetHandle, AssetManager, AsyncHandle, AsyncState, LoadingState, ContentHash, AssetDepGraph};
pub use ecs::{Component, Entity, EntityBuilder, World, flush_commands, QueryState, WorldQuery, QueryFilter};
pub use error::{QuasarError, QuasarResult};
pub use event::{Events, EventsChannel};
pub use network::{NetworkConfig, NetworkPlugin, NetworkReplication, NetworkRole, NetworkState, TickAccumulator, SnapshotInterpolation, DeltaCompressor, InputHistory, Misprediction, DeltaFlags, EncodedDelta, TransportProtocol, TransportType, Transport, TransportEvent, SendChannel, ConnectionMetrics, NetworkMetrics, QuicConfig, QuicChannel, QuicTransport, QuicTransportBackend, QuicEvent, ReplicationResource, Replicated, replication_system, PendingServerSnapshot, rollback_system, ReplicationMode, ReplicatedField, ReplicateDescriptor, HistoryBuffer, LagCompensationManager, RelayServerConfig, RelayServer, RelaySession, UdpTransport};
pub use navigation::{NavMesh, NavMeshAgent, NavMeshAgentSystem, NavPoly, NavObstacle, NavObstacleShape, DynamicNavMesh, find_path, path_to_waypoints};
pub use plugin::Plugin;
pub use prefab::{ComponentOverride, OverrideHandlerFn, OverrideRegistry, Prefab, PrefabEntity, PrefabFieldDiff, PrefabInstance, PrefabLibrary, PrefabMeshTag, PrefabProperties, PrefabProperty, apply_overrides, diff_instance_transform, instantiate_prefab, is_field_overridden, propagate_prefab_changes};
pub use profiler::{AllocTracker, FrameBudget, FrameStats, Profiler, ProfilerPlugin};
pub use scene::{Scene, SceneGraph};
pub use save_load::{GameSave, SaveMeta, SavedEntity, capture_game_save, load_game_save};
pub use scene_serde::{EntityData, SceneData};
pub use time::{FixedUpdateAccumulator, Time};
