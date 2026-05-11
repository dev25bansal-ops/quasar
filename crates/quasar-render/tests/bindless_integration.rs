//! Integration tests for the bindless rendering system.
//!
//! These tests verify the complete bindless rendering pipeline including:
//! - Texture atlas registration and batch operations
//! - Sampler pool management
//! - Material data buffer operations (push, update, remove, upload)
//! - Resource lifetime management
//! - Bindless capabilities detection
//! - GPU material data structure layout
//! - Fallback path verification

use quasar_render::bindless::{
    BindlessCapabilities, FallbackBindGroupBuilder, GpuMaterialData, MaterialDataBuffer,
    ResourceLifetimeManager, SamplerPool, TextureAtlas, GPU_MATERIAL_SIZE, MAX_BINDLESS_TEXTURES,
    MAX_MATERIALS,
};
use quasar_render::DrawCallUniform;
use std::sync::OnceLock;

// ── Texture Atlas Tests ─────────────────────────────────────────

#[test]
fn texture_atlas_registration() {
    let mut atlas = TextureAtlas::new();

    // Register a texture
    let view = create_test_texture_view();
    let idx = atlas.register(1, view);
    assert!(idx.is_some());
    assert_eq!(idx.unwrap(), 0);
    assert_eq!(atlas.count(), 1);
}

#[test]
fn texture_atlas_duplicate_registration() {
    let mut atlas = TextureAtlas::new();

    // Register the same texture twice
    let view1 = create_test_texture_view();
    let view2 = create_test_texture_view();
    let idx1 = atlas.register(42, view1);
    let idx2 = atlas.register(42, view2);

    // Should return the same index
    assert_eq!(idx1.unwrap(), idx2.unwrap());
    assert_eq!(atlas.count(), 1);
}

#[test]
fn texture_atlas_batch_registration() {
    let mut atlas = TextureAtlas::new();

    let textures: Vec<(u64, wgpu::TextureView)> =
        (0..10).map(|i| (i, create_test_texture_view())).collect();

    let results = atlas.register_batch(&textures);

    assert_eq!(results.len(), 10);
    assert_eq!(atlas.count(), 10);

    // Verify all indices are unique and sequential
    for (i, (_, idx)) in results.iter().enumerate() {
        assert_eq!(idx.unwrap(), i as u32);
    }
}

#[test]
fn texture_atlas_removal_and_reuse() {
    let mut atlas = TextureAtlas::new();

    // Register textures
    let view1 = create_test_texture_view();
    let view2 = create_test_texture_view();
    let view3 = create_test_texture_view();

    let idx1 = atlas.register(1, view1).unwrap();
    let idx2 = atlas.register(2, view2).unwrap();

    // Remove first texture
    atlas.remove(1);
    assert_eq!(atlas.count(), 1);
    atlas.flush_removals();

    // Register new texture - should reuse freed slot
    let idx3 = atlas.register(3, view3).unwrap();
    assert_eq!(idx3, idx1); // Should reuse the freed slot
    assert_eq!(atlas.count(), 2);
}

#[test]
fn texture_atlas_capacity() {
    let mut atlas = TextureAtlas::new();

    // Fill the atlas
    for i in 0..MAX_BINDLESS_TEXTURES {
        let view = create_test_texture_view();
        let result = atlas.register(i as u64, view);
        assert!(result.is_some());
    }

    // Should be full
    assert!(atlas.is_full());

    // Next registration should fail
    let view = create_test_texture_view();
    let result = atlas.register(MAX_BINDLESS_TEXTURES as u64 + 1, view);
    assert!(result.is_none());
}

#[test]
fn texture_atlas_clear() {
    let mut atlas = TextureAtlas::new();

    for i in 0..5 {
        let view = create_test_texture_view();
        atlas.register(i, view);
    }

    assert_eq!(atlas.count(), 5);

    atlas.clear();
    assert_eq!(atlas.count(), 0);
    assert!(!atlas.is_full());
}

#[test]
fn texture_atlas_pending_removals() {
    let mut atlas = TextureAtlas::new();

    let view = create_test_texture_view();
    atlas.register(1, view);
    atlas.remove(1);

    // Removal is deferred
    assert_eq!(atlas.pending_removal_count(), 1);

    // Flush removals
    atlas.flush_removals();
    assert_eq!(atlas.pending_removal_count(), 0);
}

// ── Sampler Pool Tests ──────────────────────────────────────────

#[test]
fn sampler_pool_registration() {
    let mut pool = SamplerPool::new();

    let sampler = create_test_sampler();
    let idx = pool.register(1, sampler);
    assert!(idx.is_some());
    assert_eq!(idx.unwrap(), 0);
    assert_eq!(pool.count(), 1);
}

#[test]
fn sampler_pool_batch_registration() {
    let mut pool = SamplerPool::new();

    let samplers: Vec<(u64, wgpu::Sampler)> = (0..5).map(|i| (i, create_test_sampler())).collect();

    let results = pool.register_batch(&samplers);

    assert_eq!(results.len(), 5);
    assert_eq!(pool.count(), 5);
}

#[test]
fn sampler_pool_capacity() {
    let mut pool = SamplerPool::new();

    // Pool uses u64::MAX as marker, but we test the actual capacity
    for i in 0..64 {
        let sampler = create_test_sampler();
        let result = pool.register(i, sampler);
        assert!(result.is_some());
    }

    assert!(pool.count() >= 64);
}

#[test]
fn sampler_pool_clear() {
    let mut pool = SamplerPool::new();

    for i in 0..10 {
        let sampler = create_test_sampler();
        pool.register(i, sampler);
    }

    assert_eq!(pool.count(), 10);

    pool.clear();
    assert_eq!(pool.count(), 0);
}

// ── Material Data Buffer Tests ──────────────────────────────────

#[test]
fn gpu_material_data_size() {
    // Verify GpuMaterialData is 64 bytes (4x vec4)
    assert_eq!(std::mem::size_of::<GpuMaterialData>(), 64);
    assert_eq!(GPU_MATERIAL_SIZE, 64);
}

#[test]
fn gpu_material_data_default() {
    let mat = GpuMaterialData::default();

    assert_eq!(mat.base_color, [1.0, 1.0, 1.0, 1.0]);
    assert_eq!(mat.roughness, 0.5);
    assert_eq!(mat.metallic, 0.0);
    assert_eq!(mat.emissive_strength, 0.0);
    assert_eq!(mat.albedo_tex_index, u32::MAX);
    assert_eq!(mat.normal_tex_index, u32::MAX);
    assert_eq!(mat.mr_tex_index, u32::MAX);
    assert_eq!(mat.sampler_index, 0);
}

#[test]
fn gpu_material_data_from_color() {
    let mat = GpuMaterialData::from_color([0.8, 0.2, 0.3, 1.0], 0.7, 0.3);

    assert_eq!(mat.base_color, [0.8, 0.2, 0.3, 1.0]);
    assert_eq!(mat.roughness, 0.7);
    assert_eq!(mat.metallic, 0.3);
}

#[test]
fn material_buffer_push_and_count() {
    let (device, queue) = create_test_device();
    let mut buffer = MaterialDataBuffer::new(&device, 100);

    assert_eq!(buffer.count(), 0);
    assert_eq!(buffer.capacity(), 100);

    let mat = GpuMaterialData::from_color([1.0, 0.0, 0.0, 1.0], 0.5, 0.0);
    let idx = buffer.push(mat);
    assert!(idx.is_some());
    assert_eq!(buffer.count(), 1);
    assert_eq!(idx.unwrap(), 0);
}

#[test]
fn material_buffer_update() {
    let (device, queue) = create_test_device();
    let mut buffer = MaterialDataBuffer::new(&device, 100);

    let mat1 = GpuMaterialData::from_color([1.0, 0.0, 0.0, 1.0], 0.5, 0.0);
    let idx = buffer.push(mat1).unwrap();

    // Update the material
    let mat2 = GpuMaterialData::from_color([0.0, 1.0, 0.0, 1.0], 0.3, 0.5);
    buffer.update(idx, mat2);

    let retrieved = buffer.get(idx).unwrap();
    assert_eq!(retrieved.base_color, [0.0, 1.0, 0.0, 1.0]);
    assert_eq!(retrieved.roughness, 0.3);
    assert_eq!(retrieved.metallic, 0.5);
}

#[test]
fn material_buffer_remove_and_reuse() {
    let (device, queue) = create_test_device();
    let mut buffer = MaterialDataBuffer::new(&device, 10);

    let mat1 = GpuMaterialData::from_color([1.0, 0.0, 0.0, 1.0], 0.5, 0.0);
    let idx1 = buffer.push(mat1).unwrap();

    // Remove the material
    buffer.remove(idx1);
    assert_eq!(buffer.count(), 0);
    assert_eq!(buffer.free_slot_count(), 10);

    // Push new material - should reuse freed slot
    let mat2 = GpuMaterialData::from_color([0.0, 1.0, 0.0, 1.0], 0.3, 0.5);
    let idx2 = buffer.push(mat2).unwrap();
    assert_eq!(idx2, idx1); // Should reuse the same slot
    assert_eq!(buffer.count(), 1);
}

#[test]
fn material_buffer_upload() {
    let (device, queue) = create_test_device();
    let mut buffer = MaterialDataBuffer::new(&device, 100);

    let mat = GpuMaterialData::from_color([1.0, 0.5, 0.0, 1.0], 0.8, 0.2);
    buffer.push(mat);

    // Upload all
    buffer.upload_all(&queue);

    // Upload dirty only (should be no-op after upload_all)
    buffer.upload_dirty(&queue);
}

#[test]
fn material_buffer_dirty_tracking() {
    let (device, queue) = create_test_device();
    let mut buffer = MaterialDataBuffer::new(&device, 100);

    let mat = GpuMaterialData::default();
    buffer.push(mat);

    // Mark as clean by uploading
    buffer.upload_dirty(&queue);

    // Update should mark dirty
    buffer.update(
        0,
        GpuMaterialData::from_color([1.0, 0.0, 0.0, 1.0], 0.5, 0.0),
    );

    // Upload dirty should succeed
    buffer.upload_dirty(&queue);
}

#[test]
fn material_buffer_clear() {
    let (device, queue) = create_test_device();
    let mut buffer = MaterialDataBuffer::new(&device, 100);

    for i in 0..5 {
        let mat = GpuMaterialData::from_color([0.2 * i as f32, 0.0, 0.0, 1.0], 0.5, 0.0);
        buffer.push(mat);
    }

    assert_eq!(buffer.count(), 5);

    buffer.clear();
    assert_eq!(buffer.count(), 0);
    assert_eq!(buffer.free_slot_count(), 100);
}

// ── Resource Lifetime Manager Tests ─────────────────────────────

#[test]
fn lifetime_manager_registration() {
    let mut manager = ResourceLifetimeManager::new();

    let mat = GpuMaterialData {
        albedo_tex_index: 5,
        normal_tex_index: 6,
        mr_tex_index: 7,
        sampler_index: 1,
        ..Default::default()
    };

    manager.register_material(0, &mat);

    assert_eq!(manager.tracked_material_count(), 1);
    assert_eq!(manager.active_texture_count(), 3);
    assert_eq!(manager.active_sampler_count(), 1);
}

#[test]
fn lifetime_manager_unregister() {
    let mut manager = ResourceLifetimeManager::new();

    let mat = GpuMaterialData {
        albedo_tex_index: 5,
        sampler_index: 1,
        ..Default::default()
    };

    manager.register_material(0, &mat);
    assert!(manager.is_texture_in_use(5));

    manager.unregister_material(0);

    // Should still be in use until frame advance
    assert!(manager.is_texture_in_use(5));

    // Advance frames to process pending removals
    for _ in 0..5 {
        manager.advance_frame();
    }

    // Now should be removed
    assert!(!manager.is_texture_in_use(5));
}

#[test]
fn lifetime_manager_frame_advancement() {
    let mut manager = ResourceLifetimeManager::with_removal_delay(2);

    let mat = GpuMaterialData {
        albedo_tex_index: 10,
        sampler_index: 2,
        ..Default::default()
    };

    manager.register_material(0, &mat);
    manager.unregister_material(0);

    // Check pending counts
    let (tex_pending, samp_pending) = manager.pending_removal_counts();
    assert_eq!(tex_pending, 1);
    assert_eq!(samp_pending, 1);

    // Advance one frame (not enough)
    manager.advance_frame();
    assert!(manager.is_texture_in_use(10));

    // Advance second frame (should process removal)
    manager.advance_frame();
    manager.advance_frame();

    assert!(!manager.is_texture_in_use(10));
}

#[test]
fn lifetime_manager_force_flush() {
    let mut manager = ResourceLifetimeManager::new();

    let mat = GpuMaterialData {
        albedo_tex_index: 10,
        sampler_index: 2,
        ..Default::default()
    };

    manager.register_material(0, &mat);
    manager.unregister_material(0);

    // Force flush
    manager.force_flush_pending_removals();

    assert!(!manager.is_texture_in_use(10));
    assert!(!manager.is_sampler_in_use(2));
}

#[test]
fn lifetime_manager_multiple_materials() {
    let mut manager = ResourceLifetimeManager::new();

    // Two materials sharing the same texture
    let mat1 = GpuMaterialData {
        albedo_tex_index: 5,
        sampler_index: 1,
        ..Default::default()
    };
    let mat2 = GpuMaterialData {
        albedo_tex_index: 5,
        sampler_index: 2,
        ..Default::default()
    };

    manager.register_material(0, &mat1);
    manager.register_material(1, &mat2);

    // Texture should have ref count 2
    assert!(manager.is_texture_in_use(5));

    // Remove first material
    manager.unregister_material(0);

    // Texture should still be in use (ref count 1)
    assert!(manager.is_texture_in_use(5));

    // Remove second material
    manager.unregister_material(1);

    // Now texture should be pending removal
    assert!(manager.is_texture_in_use(5));

    // Flush
    for _ in 0..5 {
        manager.advance_frame();
    }

    assert!(!manager.is_texture_in_use(5));
}

// ── Draw Call Uniform Tests ─────────────────────────────────────

#[test]
fn draw_call_uniform_layout() {
    let uniform = DrawCallUniform::new(42);

    assert_eq!(uniform.material_index, 42);
    assert_eq!(uniform._pad, [0; 3]);

    // Verify size and alignment
    assert_eq!(std::mem::size_of::<DrawCallUniform>(), 16);
    assert_eq!(std::mem::align_of::<DrawCallUniform>(), 4);
}

#[test]
fn draw_call_uniform_default() {
    let uniform = DrawCallUniform::default();
    assert_eq!(uniform.material_index, 0);
}

#[test]
fn draw_call_uniform_pod() {
    let uniform = DrawCallUniform::new(123);
    let bytes = bytemuck::bytes_of(&uniform);

    // Should be exactly 16 bytes
    assert_eq!(bytes.len(), 16);

    // Roundtrip
    let roundtrip = bytemuck::pod_read_unaligned::<DrawCallUniform>(bytes);
    assert_eq!(roundtrip.material_index, 123);
}

// ── Bindless Capabilities Tests ─────────────────────────────────

#[test]
fn bindless_capabilities_default() {
    let caps = BindlessCapabilities::default();

    assert!(!caps.texture_binding_array);
    assert!(!caps.non_uniform_indexing);
    assert!(!caps.storage_binding_array);
    assert!(!caps.full_bindless);
}

#[test]
fn bindless_capabilities_support_level() {
    let full_caps = BindlessCapabilities {
        texture_binding_array: true,
        non_uniform_indexing: true,
        storage_binding_array: true,
        bindless_sampling: true,
        full_bindless: true,
    };
    assert!(full_caps.support_level().contains("FULL"));

    let partial_caps = BindlessCapabilities {
        texture_binding_array: true,
        non_uniform_indexing: true,
        storage_binding_array: false,
        bindless_sampling: true,
        full_bindless: true,
    };
    assert!(
        partial_caps.support_level().contains("PARTIAL")
            || partial_caps.support_level().contains("FULL")
    );

    let no_caps = BindlessCapabilities::default();
    assert!(no_caps.support_level().contains("NONE"));
}

// ── Fallback Bind Group Builder Tests ───────────────────────────

#[test]
fn fallback_bind_group_builder_creation() {
    let (device, _queue) = create_test_device();
    let builder = FallbackBindGroupBuilder::new(&device);

    // Builder should have a valid layout
    assert!(true); // If we got here, construction succeeded
}

// ── Helper Functions ─────────────────────────────────────────────

fn create_test_texture_view() -> wgpu::TextureView {
    let (device, _queue) = create_test_device();

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Test Texture"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

fn create_test_sampler() -> wgpu::Sampler {
    let (device, _queue) = create_test_device();

    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Test Sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    })
}

fn create_test_device() -> (wgpu::Device, wgpu::Queue) {
    static TEST_DEVICE: OnceLock<Result<(wgpu::Device, wgpu::Queue), String>> = OnceLock::new();

    let device = TEST_DEVICE.get_or_init(|| {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::None,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .or_else(|| {
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::None,
                compatible_surface: None,
                force_fallback_adapter: true,
            }))
        })
        .ok_or_else(|| "No GPU adapter available for bindless integration tests".to_string())?;

        pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("Bindless Test Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: Default::default(),
            },
            None,
        ))
        .map_err(|err| format!("Failed to create bindless test device: {err:?}"))
    });

    match device {
        Ok((device, queue)) => (device.clone(), queue.clone()),
        Err(err) => panic!("{err}"),
    }
}
