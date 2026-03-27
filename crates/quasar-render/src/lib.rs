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

pub mod asset_loader;
pub mod bindless;
pub mod camera;
pub mod camera_controller;
pub mod cascade_shadow;
#[cfg(feature = "clustered-lighting")]
pub mod clustered;
pub mod components;
pub mod culling;
pub mod debug_wireframe;
#[cfg(feature = "decals")]
pub mod decal;
#[cfg(feature = "deferred")]
pub mod deferred;
pub mod environment;
pub mod gltf_loader;
pub mod gpu_memory;
pub mod gpu_profiler;
pub mod hdr;
pub mod hot_reload;
pub mod instanced;
pub mod light;
#[cfg(feature = "lightmap")]
pub mod lightmap;
pub mod loader;
pub mod lod;
pub mod material;
pub mod mesh;
#[cfg(feature = "meshlet")]
pub mod meshlet;
pub mod motion_vector_pass;
#[cfg(feature = "gpu-culling")]
pub mod occlusion;
#[cfg(feature = "particles")]
pub mod particle;
pub mod pipeline;
pub mod pipeline_cache;
#[cfg(feature = "post-process")]
pub mod post_process;
pub mod radiance_cache;
#[cfg(feature = "reflection-probes")]
pub mod reflection_probe;
pub mod render_2d;
pub mod render_graph;
pub mod render_plugin;
pub mod renderer;
#[cfg(feature = "raytracing")]
pub mod rt;
#[cfg(feature = "shader-graph")]
pub mod shader_graph;
pub mod shadow;
pub mod skinning;
#[cfg(feature = "sprites")]
pub mod sprite;
pub mod ssgi;
#[cfg(feature = "ssr")]
pub mod ssr;
pub mod staging_belt;
pub mod streaming;
pub mod svt;
pub mod taa;
#[cfg(feature = "terrain")]
pub mod terrain;
pub mod texture;
pub mod vertex;
pub mod virtual_shadow;
#[cfg(feature = "volumetric")]
pub mod volumetric;

pub use asset_loader::{AssetLoader, GpuMaterial, GpuMesh, GpuTexture, RenderAssetManager};
pub use bindless::{
    GpuMaterialData, MaterialDataBuffer, TextureAtlas, MAX_BINDLESS_TEXTURES, MAX_MATERIALS,
};
pub use camera::Camera;
pub use camera_controller::{FpsCameraController, OrbitController};
pub use cascade_shadow::{
    Cascade, CascadeShadowMap, CascadeUniform, CASCADE_COUNT, SHADOW_MAP_SIZE,
};
pub use components::TextureHandle;
pub use culling::{Aabb, Frustum};
pub use environment::{EnvironmentMap, EnvironmentMapLoader, IBL_MIP_LEVELS};
pub use gltf_loader::{
    load_gltf, load_gltf_animations, load_gltf_morph_targets, sample_quat, sample_vec3,
    GltfAnimationChannel, GltfAnimationClip, GltfChannelProperty, GltfChannelValues,
    GltfInterpolation,
};
pub use gpu_memory::{AllocationId, GpuMemoryTracker, GpuResourceKind, MemoryBudget};
pub use hdr::{ColorGrading, HdrRenderTarget, Tonemapping, TonemappingPass};
pub use hot_reload::HotReloadSystem;
pub use instanced::{InstanceBatch, InstanceCollector, InstanceData, InstancedMesh, MAX_INSTANCES};
pub use light::{
    AmbientLight, DirectionalLight, LightData, LightsUniform, PointLight, SpotLight, MAX_LIGHTS,
};
pub use loader::load_obj;
pub use lod::{
    bayer_threshold, LodCrossFade, LodGroup, LodLevel, LodSystem, BAYER_4X4, LOD_CROSSFADE_BAND,
    LOD_CROSSFADE_WGSL,
};
pub use material::{LightUniform, Material, MaterialOverride, MaterialUniform};
pub use mesh::{Mesh, MeshCache, MeshData, MeshShape};
pub use motion_vector_pass::{MotionVectorPass, MotionVectorUniforms};
pub use pipeline_cache::PipelineCache;
pub use radiance_cache::{RadianceCache, RadianceCacheSettings, RadianceProbe, SH_COEFF_COUNT};
pub use render_graph::{
    attachment_ids, pass_ids, Attachment, AttachmentId, PassId, PassNode, PassNodeExt, PassQueue,
    RenderContext, RenderGraph, RenderGraphError, RenderPass, ResourceState, ResourceTransition,
    TextureBarrier,
};
pub use render_plugin::{
    resource_keys, MeshDrawItem, MeshDrawList, RenderPlugin, RenderSyncOutput,
};
pub use renderer::{RenderConfig, Renderer};
#[cfg(feature = "raytracing")]
pub use rt::{Blas, RtGiPass, RtGiSettings, Tlas, TlasInstance};
pub use shadow::{ShadowCamera, ShadowMap};
pub use skinning::{
    Bone, BoneMatricesBuffer, MorphTarget, MorphTargetSet, Skeleton, SkinnedMesh, SkinnedVertex,
    MAX_BONES, MAX_BONE_INFLUENCES, MAX_MORPH_TARGETS,
};
pub use ssgi::{SsgiPass, SsgiSettings};
pub use staging_belt::StagingBelt;
pub use streaming::{StreamingPool, StreamingPriority, StreamingRequest};
pub use svt::{
    FeedbackEntry, GpuFeedbackPass, GpuFeedbackTexel, PageTableEntry, PhysicalSlot, SvtSystem,
    TilePool, TileStreamer, VirtualTexture2D, VirtualTileId, FEEDBACK_RT_SIZE, SVT_TILE_SIZE,
};
pub use taa::TaaPass;
pub use texture::Texture;
pub use vertex::Vertex;

#[cfg(feature = "clustered-lighting")]
pub use clustered::{
    Cluster, ClusterAabb, LightClusterGrid, CLUSTERED_LIGHT_WGSL, CLUSTER_X, CLUSTER_Y, CLUSTER_Z,
    MAX_LIGHTS_PER_CLUSTER, TOTAL_CLUSTERS,
};
#[cfg(feature = "decals")]
pub use decal::{
    Decal, DecalBatch, DecalUniform, DECAL_PROJECTION_WGSL, DECAL_RENDER_WGSL, MAX_DECALS,
};
#[cfg(feature = "deferred")]
pub use deferred::{
    DeferredLightingPass, GBuffer, InverseCameraUniforms, LightVolumeUniform,
    StencilLightVolumePass, GBUFFER_TARGET_COUNT,
};
#[cfg(feature = "lightmap")]
pub use lightmap::{
    BakeConfig, GpuBakeUniform, GpuBakerTriangle, GpuLightmapBaker, GpuPathTraceBaker,
    GpuPathTraceUniform, Lightmap, LightmapBaker, LightmapMaterial, PathTraceBakeConfig, SHProbe,
    SHProbeGrid,
};
#[cfg(feature = "gpu-culling")]
pub use occlusion::{
    BindlessResources, DrawIndexedIndirectArgs, DrawInstanceData, GpuAabb, GpuCullPass,
    GpuCullUniforms, HiZBuffer, HiZMip, IndirectDrawManager, MeshDrawCommand,
    MultiDrawIndirectCount, BINDLESS_MAX_MATERIALS, BINDLESS_MAX_TEXTURES, GPU_CULL_MAX_OBJECTS,
    GPU_CULL_WGSL, HIZ_MIP_LEVELS,
};
#[cfg(feature = "particles")]
pub use particle::{
    GpuParticleSystem, ParticleData, ParticleEmitter, ParticleEmitterConfig, MAX_PARTICLES,
};
#[cfg(feature = "post-process")]
pub use post_process::{PostProcessPass, PostProcessSettings, SSAO_KERNEL_SIZE, SSAO_NOISE_SIZE};
#[cfg(feature = "reflection-probes")]
pub use reflection_probe::{
    GpuDevice, ReflectionProbe, ReflectionProbeManager, ReflectionProbeSystem,
    ReflectionProbeUniform, MAX_REFLECTION_PROBES, PROBE_FACE_SIZE, PROBE_MIP_LEVELS,
};
pub use render_2d::{
    AnimationFrame, AnimationSequence, Camera2D, NineSlice, ParallaxLayer, Particle2D,
    ParticleEmitter2D, ParticleEmitterConfig as ParticleEmitterConfig2D, Shape2D, ShapeBatch2D,
    SmoothFollow, SpriteAnimator, Tile, TileChunk, Tilemap, Tileset,
};
#[cfg(feature = "shader-graph")]
pub use shader_graph::{
    BlendMode, CompileResult, DiagnosticSeverity, MaterialDomain, MaterialGraph, ShaderConnection,
    ShaderGraph, ShaderGraphCache, ShaderGraphCompiler, ShaderGraphDiagnostic, ShaderNode,
    ShaderNodeKind,
};
#[cfg(feature = "sprites")]
pub use sprite::{
    FontAtlas, OrthographicCamera, Sprite, SpriteBatch, SpriteRect, SpriteVertex, TextRenderer,
};
#[cfg(feature = "ssr")]
pub use ssr::{SsrPass, SsrSettings, SsrUniform};
#[cfg(feature = "terrain")]
pub use terrain::{
    HeightFieldColliderDesc, TerrainConfig, TerrainLodLevel, TerrainMesh, TerrainSplatmap,
    MAX_SPLAT_LAYERS, MAX_TERRAIN_LODS, TERRAIN_SPLATMAP_WGSL,
};
#[cfg(feature = "volumetric")]
pub use volumetric::{VolumetricFogPass, VolumetricFogSettings, VolumetricFogUniform};
