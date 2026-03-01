//! # Quasar Engine
//!
//! **Quasar** is a modular 3D game engine written in Rust.
//!
//! This meta-crate re-exports all engine subsystems for convenient access:
//!
//! ```ignore
//! use quasar_engine::prelude::*;
//! ```

/// Re-export all engine crates.
pub use quasar_core as core;
pub use quasar_math as math;
pub use quasar_render as render;
pub use quasar_window as window;
pub use quasar_physics as physics;
pub use quasar_audio as audio;
pub use quasar_scripting as scripting;
pub use quasar_editor as editor;

/// Commonly used types — star-import this in your game code.
pub mod prelude {
    // Core ECS
    pub use quasar_core::{App, World, Entity, Component, Events, Time, Plugin};
    pub use quasar_core::ecs::{Schedule, SystemStage, System};

    // Math
    pub use quasar_math::{Transform, Color, Vec2, Vec3, Vec4, Mat4, Quat};

    // Rendering
    pub use quasar_render::{Renderer, Camera, Mesh, MeshData, Vertex};

    // Window & Input
    pub use quasar_window::{QuasarWindow, Input, KeyState};

    // Physics
    pub use quasar_physics::{
        PhysicsWorld, PhysicsPlugin, BodyType,
        RigidBodyComponent, ColliderComponent, ColliderShape,
    };

    // Audio
    pub use quasar_audio::{AudioSystem, AudioSource, AudioListener, AudioPlugin};

    // Scripting
    pub use quasar_scripting::{ScriptEngine, ScriptComponent, ScriptingPlugin};

    // Editor
    pub use quasar_editor::Editor;
}
