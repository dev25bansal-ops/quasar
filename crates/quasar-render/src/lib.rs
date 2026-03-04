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
//! - Multi-light support (directional, point, spot)
//! - Shadow mapping
//! - HDR rendering with tonemapping
//! - Render graph for pass composition
//! - Instanced rendering for batching
//! - PBR with Cook-Torrance BRDF and IBL
//! - Cascade Shadow Maps
//! - GPU skinning
//! - Post-processing (FXAA, Bloom, SSAO)
//! - Particle system
//! - Basic WGSL shader compilation

pub mod camera;
pub mod camera_controller;
pub mod culling;
pub mod components;
pub mod gltf_loader;
pub mod hdr;
pub mod instanced;
pub mod light;
pub mod loader;
pub mod material;
pub mod mesh;
pub mod pipeline;
pub mod render_graph;
pub mod renderer;
pub mod shadow;
pub mod texture;
pub mod vertex;
pub mod asset_loader;
pub mod environment;
pub mod cascade_shadow;
pub mod skinning;
pub mod post_process;
pub mod particle;

pub use camera::Camera;
pub use camera_controller::{FpsCameraController, OrbitController};
pub use components::TextureHandle;
pub use culling::{Aabb, Frustum};
pub use gltf_loader::load_gltf;
pub use hdr::{HdrRenderTarget, TonemappingPass, Tonemapping, ColorGrading};
pub use instanced::{InstancedMesh, InstanceData, InstanceBatch, InstanceCollector, MAX_INSTANCES};
pub use light::{DirectionalLight, PointLight, SpotLight, AmbientLight, LightData, LightsUniform, MAX_LIGHTS};
pub use loader::load_obj;
pub use material::{LightUniform, Material, MaterialOverride, MaterialUniform};
pub use mesh::{Mesh, MeshCache, MeshData, MeshShape};
pub use render_graph::{RenderGraph, RenderPass, RenderContext, PassId, AttachmentId, Attachment, PassNode, pass_ids, attachment_ids};
pub use renderer::{RenderConfig, Renderer};
pub use shadow::{ShadowMap, ShadowCamera};
pub use texture::Texture;
pub use vertex::Vertex;
pub use asset_loader::{AssetLoader, GpuTexture, GpuMesh, GpuMaterial, RenderAssetManager};
pub use environment::{EnvironmentMap, EnvironmentMapLoader, IBL_MIP_LEVELS};
pub use cascade_shadow::{CascadeShadowMap, Cascade, CASCADE_COUNT, SHADOW_MAP_SIZE};
pub use skinning::{Skeleton, Bone, SkinnedVertex, BoneMatricesBuffer, SkinnedMesh, MAX_BONES, MAX_BONE_INFLUENCES};
pub use post_process::{PostProcessPass, PostProcessSettings, SSAO_KERNEL_SIZE, SSAO_NOISE_SIZE};
pub use particle::{ParticleEmitter, ParticleEmitterConfig, ParticleData, GpuParticleSystem, MAX_PARTICLES};
