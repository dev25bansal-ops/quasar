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

pub mod camera;
pub mod material;
pub mod mesh;
pub mod pipeline;
pub mod renderer;
pub mod texture;
pub mod vertex;

pub use camera::Camera;
pub use material::{LightUniform, Material, MaterialUniform};
pub use mesh::{Mesh, MeshData};
pub use renderer::Renderer;
pub use texture::Texture;
pub use vertex::Vertex;
