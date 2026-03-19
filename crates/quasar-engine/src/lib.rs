//! # Quasar Engine
//!
//! **Quasar** is a modular 3D game engine written in Rust.
//!
//! This meta-crate re-exports all engine subsystems for convenient access:
//!
//! ```ignore
//! use quasar_engine::prelude::*;
//!
//! let mut app = App::new();
//! app.add_plugin(PhysicsPlugin);
//! app.add_plugin(AudioPlugin);
//! run(app, WindowConfig::default());
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used)]

pub mod runner;

#[cfg(feature = "audio")]
pub use quasar_audio as audio;
/// Re-export all engine crates.
pub use quasar_core as core;
#[cfg(feature = "editor")]
pub use quasar_editor as editor;
pub use quasar_math as math;
#[cfg(feature = "physics")]
pub use quasar_physics as physics;
pub use quasar_render as render;
#[cfg(feature = "scripting")]
pub use quasar_scripting as scripting;
pub use quasar_window as window;

pub use runner::run;

/// Commonly used types — star-import this in your game code.
pub mod prelude {
    // Core ECS
    pub use quasar_core::asset::{Asset, AssetHandle, AssetManager};
    pub use quasar_core::ecs::{Schedule, System, SystemStage};
    pub use quasar_core::{
        App, Component, Entity, EntityBuilder, Events, Plugin, Time, TimeSnapshot, World,
    };
    pub use quasar_core::{Scene, SceneGraph};

    // Math
    pub use quasar_math::{
        Color, EulerRot, GlobalTransform, Mat4, Quat, Transform, Vec2, Vec3, Vec4,
    };

    // Rendering
    pub use quasar_render::{
        Aabb, Camera, FpsCameraController, Frustum, LightUniform, Material, MaterialOverride,
        MaterialUniform, Mesh, MeshCache, MeshData, MeshShape, OrbitController, Renderer, Texture,
        Vertex,
    };

    // Window & Input
    pub use quasar_window::{ActionMap, Input, InputBinding, KeyState, QuasarWindow, WindowConfig};

    // Physics
    #[cfg(feature = "physics")]
    pub use quasar_physics::{
        BodyType, ColliderComponent, ColliderShape, PhysicsPlugin, PhysicsResource, PhysicsWorld,
        RigidBodyComponent,
    };

    // Audio
    #[cfg(feature = "audio")]
    pub use quasar_audio::{
        AudioListener, AudioPlugin, AudioResource, AudioSource, AudioSystem, SpatialAudioSystem,
    };

    // Scripting
    #[cfg(feature = "scripting")]
    pub use quasar_scripting::{ScriptComponent, ScriptEngine, ScriptingPlugin, ScriptingResource};

    // Editor
    #[cfg(feature = "editor")]
    pub use quasar_editor::Editor;

    // Runner
    pub use crate::runner::run;
}
