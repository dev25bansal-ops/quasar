//! # Quasar Engine
//!
//! **Quasar** is a modular 3D game engine written in Rust.
//!
//! This meta-crate re-exports all engine subsystems for convenient access:
//!
//! ```ignore
//! use quasar_engine::prelude::*;
//! ```

pub use quasar_audio as audio;
/// Re-export all engine crates.
pub use quasar_core as core;
pub use quasar_editor as editor;
pub use quasar_math as math;
pub use quasar_physics as physics;
pub use quasar_render as render;
pub use quasar_scripting as scripting;
pub use quasar_window as window;

/// Commonly used types — star-import this in your game code.
pub mod prelude {
    // Core ECS
    pub use quasar_core::ecs::{Schedule, System, SystemStage};
    pub use quasar_core::{App, Component, Entity, EntityBuilder, Events, Plugin, Time, World};
    pub use quasar_core::{Scene, SceneGraph};

    // Math
    pub use quasar_math::{Color, GlobalTransform, Mat4, Quat, Transform, Vec2, Vec3, Vec4};

    // Rendering
    pub use quasar_render::{
        Camera, LightUniform, Material, MaterialUniform, Mesh, MeshData, Renderer, Texture, Vertex,
    };

    // Window & Input
    pub use quasar_window::{Input, KeyState, QuasarWindow};

    // Physics
    pub use quasar_physics::{
        BodyType, ColliderComponent, ColliderShape, PhysicsPlugin, PhysicsWorld, RigidBodyComponent,
    };

    // Audio
    pub use quasar_audio::{AudioListener, AudioPlugin, AudioSource, AudioSystem};

    // Scripting
    pub use quasar_scripting::{ScriptComponent, ScriptEngine, ScriptingPlugin};

    // Editor
    pub use quasar_editor::Editor;
}
