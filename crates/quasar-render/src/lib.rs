//! # Quasar Render
//!
//! GPU-accelerated 3D rendering powered by [`wgpu`].
//!
//! Provides:
//! - Forward rendering pipeline with depth testing
//! - Perspective camera
//! - Mesh and vertex buffer management
//! - Basic WGSL shader compilation
//! - Material and texture support (WIP)

pub mod renderer;
pub mod camera;
pub mod mesh;
pub mod vertex;
pub mod pipeline;

pub use renderer::Renderer;
pub use camera::Camera;
pub use mesh::{Mesh, MeshData};
pub use vertex::Vertex;
