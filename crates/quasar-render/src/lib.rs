//! # Quasar Render
//!
//! GPU-accelerated 3D rendering powered by [`wgpu`].
//!
//! Provides:
//! - Forward rendering pipeline with depth testing
//! - Perspective camera
//! - Mesh and vertex buffer management
//! - Texture loading (PNG, JPEG)
//! - Material system (PBR-lite)
//! - Directional lighting
//! - Basic WGSL shader compilation

pub mod renderer;
pub mod camera;
pub mod mesh;
pub mod vertex;
pub mod pipeline;
pub mod texture;
pub mod material;

pub use renderer::Renderer;
pub use camera::Camera;
pub use mesh::{Mesh, MeshData};
pub use vertex::Vertex;
pub use texture::Texture;
pub use material::{Material, MaterialUniform, LightUniform};
