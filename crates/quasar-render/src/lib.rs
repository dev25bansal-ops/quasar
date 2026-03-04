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
pub mod camera_controller;
pub mod culling;
pub mod components;
pub mod gltf_loader;
pub mod loader;
pub mod material;
pub mod mesh;
pub mod pipeline;
pub mod renderer;
pub mod shadow;
pub mod texture;
pub mod vertex;
pub mod asset_loader;

pub use camera::Camera;
pub use camera_controller::{FpsCameraController, OrbitController};
pub use components::TextureHandle;
pub use culling::{Aabb, Frustum};
pub use gltf_loader::load_gltf;
pub use loader::load_obj;
pub use material::{LightUniform, Material, MaterialOverride, MaterialUniform};
pub use mesh::{Mesh, MeshCache, MeshData, MeshShape};
pub use renderer::{RenderConfig, Renderer};
pub use shadow::{ShadowMap, ShadowCamera};
pub use texture::Texture;
pub use vertex::Vertex;
pub use asset_loader::{AssetLoader, GpuTexture, GpuMesh, GpuMaterial, RenderAssetManager};
