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

#![deny(clippy::unwrap_used, clippy::expect_used)]
//! - 2D sprite rendering and UI
//! - Basic WGSL shader compilation

pub mod camera;
pub mod camera_controller;
#[cfg(feature = "clustered-lighting")]
pub mod clustered;
pub mod culling;
pub mod components;
#[cfg(feature = "decals")]
pub mod decal;
#[cfg(feature = "deferred")]
pub mod deferred;
pub mod gltf_loader;
pub mod gpu_profiler;
pub mod hdr;
pub mod instanced;
pub mod light;
pub mod loader;
pub mod lod;
pub mod material;
pub mod mesh;
#[cfg(feature = "meshlet")]
pub mod meshlet;
#[cfg(feature = "gpu-culling")]
pub mod occlusion;
pub mod pipeline;
#[cfg(feature = "reflection-probes")]
pub mod reflection_probe;
pub mod render_graph;
pub mod render_plugin;
pub mod renderer;
pub mod shadow;
#[cfg(feature = "terrain")]
pub mod terrain;
pub mod texture;
pub mod vertex;
pub mod asset_loader;
pub mod environment;
pub mod cascade_shadow;
pub mod skinning;
#[cfg(feature = "post-process")]
pub mod post_process;
#[cfg(feature = "particles")]
pub mod particle;
#[cfg(feature = "sprites")]
pub mod sprite;
pub mod render_2d;
#[cfg(feature = "volumetric")]
pub mod volumetric;
#[cfg(feature = "lightmap")]
pub mod lightmap;
#[cfg(feature = "shader-graph")]
pub mod shader_graph;
pub mod gpu_memory;
pub mod hot_reload;
pub mod pipeline_cache;
#[cfg(feature = "ssr")]
pub mod ssr;
pub mod streaming;
pub mod staging_belt;
pub mod svt;
pub mod taa;
pub mod ssgi;
pub mod motion_vector_pass;
pub mod radiance_cache;
pub mod virtual_shadow;
#[cfg(feature = "raytracing")]
pub mod rt;
pub mod bindless;

pub use camera::Camera;
pub use camera_controller::{FpsCameraController, OrbitController};
pub use components::TextureHandle;
pub use culling::{Aabb, Frustum};
pub use gltf_loader::{load_gltf, load_gltf_animations, load_gltf_morph_targets, sample_vec3, sample_quat, GltfAnimationClip, GltfAnimationChannel, GltfChannelProperty, GltfChannelValues, GltfInterpolation};
pub use hdr::{HdrRenderTarget, TonemappingPass, Tonemapping, ColorGrading};
pub use instanced::{InstancedMesh, InstanceData, InstanceBatch, InstanceCollector, MAX_INSTANCES};
pub use light::{DirectionalLight, PointLight, SpotLight, AmbientLight, LightData, LightsUniform, MAX_LIGHTS};
pub use loader::load_obj;
pub use material::{LightUniform, Material, MaterialOverride, MaterialUniform};
pub use mesh::{Mesh, MeshCache, MeshData, MeshShape};
pub use render_graph::{RenderGraph, RenderGraphError, RenderPass, RenderContext, PassId, AttachmentId, Attachment, PassNode, pass_ids, attachment_ids, ResourceState, ResourceTransition, TextureBarrier, PassQueue, PassNodeExt};
pub use render_plugin::{RenderPlugin, RenderSyncOutput, MeshDrawItem, MeshDrawList, resource_keys};
pub use renderer::{RenderConfig, Renderer};
pub use shadow::{ShadowMap, ShadowCamera};
pub use taa::TaaPass;
pub use ssgi::{SsgiPass, SsgiSettings};
pub use motion_vector_pass::{MotionVectorPass, MotionVectorUniforms};
pub use radiance_cache::{RadianceCache, RadianceCacheSettings, RadianceProbe, SH_COEFF_COUNT};
pub use texture::Texture;
pub use vertex::Vertex;
pub use asset_loader::{AssetLoader, GpuTexture, GpuMesh, GpuMaterial, RenderAssetManager};
pub use environment::{EnvironmentMap, EnvironmentMapLoader, IBL_MIP_LEVELS};
pub use cascade_shadow::{CascadeShadowMap, Cascade, CascadeUniform, CASCADE_COUNT, SHADOW_MAP_SIZE};
pub use skinning::{Skeleton, Bone, SkinnedVertex, BoneMatricesBuffer, SkinnedMesh, MorphTarget, MorphTargetSet, MAX_BONES, MAX_BONE_INFLUENCES, MAX_MORPH_TARGETS};
pub use lod::{LodGroup, LodLevel, LodSystem, LodCrossFade, LOD_CROSSFADE_BAND, LOD_CROSSFADE_WGSL, BAYER_4X4, bayer_threshold};
pub use pipeline_cache::PipelineCache;
pub use hot_reload::HotReloadSystem;
pub use streaming::{StreamingPool, StreamingRequest, StreamingPriority};
pub use staging_belt::StagingBelt;
pub use svt::{SvtSystem, TilePool, TileStreamer, VirtualTileId, PhysicalSlot, PageTableEntry, FeedbackEntry, GpuFeedbackPass, GpuFeedbackTexel, VirtualTexture2D, FEEDBACK_RT_SIZE, SVT_TILE_SIZE};
pub use gpu_memory::{GpuMemoryTracker, MemoryBudget, GpuResourceKind, AllocationId};
pub use bindless::{TextureAtlas, MaterialDataBuffer, GpuMaterialData, MAX_BINDLESS_TEXTURES, MAX_MATERIALS};
#[cfg(feature = "raytracing")]
pub use rt::{Blas, Tlas, TlasInstance, RtGiPass, RtGiSettings};

#[cfg(feature = "post-process")]
pub use post_process::{PostProcessPass, PostProcessSettings, SSAO_KERNEL_SIZE, SSAO_NOISE_SIZE};
#[cfg(feature = "particles")]
pub use particle::{ParticleEmitter, ParticleEmitterConfig, ParticleData, GpuParticleSystem, MAX_PARTICLES};
#[cfg(feature = "sprites")]
pub use sprite::{SpriteBatch, Sprite, SpriteRect, SpriteVertex, OrthographicCamera, FontAtlas, TextRenderer};
pub use render_2d::{
    SpriteAnimator, AnimationFrame, AnimationSequence,
    Tilemap, Tileset, Tile, TileChunk,
    Camera2D, ParallaxLayer, SmoothFollow,
    ParticleEmitter2D, Particle2D, ParticleEmitterConfig,
    Shape2D, ShapeBatch2D,
    NineSlice,
};
#[cfg(feature = "volumetric")]
pub use volumetric::{VolumetricFogSettings, VolumetricFogPass, VolumetricFogUniform};
#[cfg(feature = "lightmap")]
pub use lightmap::{Lightmap, LightmapBaker, BakeConfig, SHProbe, SHProbeGrid, LightmapMaterial, GpuLightmapBaker, GpuBakeUniform, GpuBakerTriangle, PathTraceBakeConfig, GpuPathTraceUniform, GpuPathTraceBaker};
#[cfg(feature = "shader-graph")]
pub use shader_graph::{ShaderGraph, ShaderNode, ShaderNodeKind, ShaderConnection, ShaderGraphCompiler, ShaderGraphCache, ShaderGraphDiagnostic, DiagnosticSeverity, CompileResult, MaterialDomain, MaterialGraph, BlendMode};
#[cfg(feature = "gpu-culling")]
pub use occlusion::{HiZBuffer, HiZMip, GpuCullPass, GpuAabb, GpuCullUniforms, DrawIndexedIndirectArgs, HIZ_MIP_LEVELS, GPU_CULL_MAX_OBJECTS, GPU_CULL_WGSL, MeshDrawCommand, IndirectDrawManager, MultiDrawIndirectCount, BindlessResources, DrawInstanceData, BINDLESS_MAX_MATERIALS, BINDLESS_MAX_TEXTURES};
#[cfg(feature = "deferred")]
pub use deferred::{GBuffer, DeferredLightingPass, InverseCameraUniforms, StencilLightVolumePass, LightVolumeUniform, GBUFFER_TARGET_COUNT};
#[cfg(feature = "reflection-probes")]
pub use reflection_probe::{ReflectionProbe, ReflectionProbeManager, ReflectionProbeSystem, ReflectionProbeUniform, GpuDevice, MAX_REFLECTION_PROBES, PROBE_FACE_SIZE, PROBE_MIP_LEVELS};
#[cfg(feature = "decals")]
pub use decal::{Decal, DecalBatch, DecalUniform, MAX_DECALS, DECAL_PROJECTION_WGSL, DECAL_RENDER_WGSL};
#[cfg(feature = "terrain")]
pub use terrain::{TerrainConfig, TerrainMesh, TerrainLodLevel, TerrainSplatmap, HeightFieldColliderDesc, MAX_TERRAIN_LODS, MAX_SPLAT_LAYERS, TERRAIN_SPLATMAP_WGSL};
#[cfg(feature = "clustered-lighting")]
pub use clustered::{LightClusterGrid, Cluster, ClusterAabb, CLUSTER_X, CLUSTER_Y, CLUSTER_Z, MAX_LIGHTS_PER_CLUSTER, TOTAL_CLUSTERS, CLUSTERED_LIGHT_WGSL};
#[cfg(feature = "ssr")]
pub use ssr::{SsrPass, SsrSettings, SsrUniform};
