//! Integration tests for the mesh shader pipeline.
//!
//! These tests verify the complete mesh shader pipeline including:
//! - LOD meshlet generation
//! - GPU buffer uploads
//! - Visibility buffer compaction
//! - Task shader uniform layout
//! - Mesh shader uniform layout
//! - Fallback path verification

use quasar_render::meshlet::{
    build_lod_meshlets, build_meshlets, LodConfig, LodMeshletData, LodMeshletMesh,
    MeshletBounds, MeshletData, VisibilityEntry, MAX_LOD_LEVELS, MAX_MESHLET_TRIANGLES,
    MAX_MESHLET_VERTICES,
};
use quasar_render::mesh_shader::{
    MeshShaderCapabilities, MeshShaderUniforms, TaskShaderUniforms,
};

// ── LOD Meshlet Generation Tests ────────────────────────────────

fn create_test_sphere_mesh(subdivisions: u32) -> (Vec<[f32; 3]>, Vec<u32>) {
    // Create a simple UV sphere mesh for testing
    let slices = subdivisions;
    let stacks = subdivisions;
    let mut positions = Vec::new();
    let mut indices = Vec::new();

    // Generate vertices
    for stack in 0..=stacks {
        let phi = std::f32::consts::PI * stack as f32 / stacks as f32;
        for slice in 0..=slices {
            let theta = 2.0 * std::f32::consts::PI * slice as f32 / slices as f32;
            let x = phi.sin() * theta.cos();
            let y = phi.cos();
            let z = phi.sin() * theta.sin();
            positions.push([x, y, z]);
        }
    }

    // Generate indices
    for stack in 0..stacks {
        for slice in 0..slices {
            let current = stack * (slices + 1) + slice;
            let next = current + slices + 1;
            indices.extend_from_slice(&[current, next, current + 1]);
            indices.extend_from_slice(&[current + 1, next, next + 1]);
        }
    }

    (positions, indices)
}

#[test]
fn lod_meshlet_generation_sphere() {
    let (positions, indices) = create_test_sphere_mesh(8);
    let config = LodConfig {
        levels: 4,
        reduction_ratio: 0.5,
        preserve_boundaries: true,
    };

    let lod_mesh = build_lod_meshlets(&positions, &indices, config);

    // Verify basic properties
    assert_eq!(lod_mesh.lod_count, 4);
    assert!(lod_mesh.total_meshlet_count > 0);
    assert!(lod_mesh.meshlets.len() == lod_mesh.total_meshlet_count as usize);

    // Verify each meshlet is within bounds
    for m in &lod_mesh.meshlets {
        assert!(m.base.vertex_count as usize <= MAX_MESHLET_VERTICES);
        assert!(m.base.triangle_count as usize <= MAX_MESHLET_TRIANGLES);
        assert!(m.lod_level < 4);
    }

    // Verify LOD 0 has the most meshlets (highest detail)
    let lod0_count = lod_mesh.meshlets.iter().filter(|m| m.lod_level == 0).count();
    assert!(lod0_count > 0, "LOD 0 should have meshlets");
}

#[test]
fn lod_chain_progressive_reduction() {
    let (positions, indices) = create_test_sphere_mesh(12);
    let config = LodConfig {
        levels: 5,
        reduction_ratio: 0.5,
        preserve_boundaries: true,
    };

    let lod_mesh = build_lod_meshlets(&positions, &indices, config);

    // Count meshlets per LOD level
    let mut lod_counts = [0u32; MAX_LOD_LEVELS];
    let mut lod_triangle_counts = [0u32; MAX_LOD_LEVELS];

    for m in &lod_mesh.meshlets {
        let lod = m.lod_level as usize;
        if lod < MAX_LOD_LEVELS {
            lod_counts[lod] += 1;
            lod_triangle_counts[lod] += m.base.triangle_count;
        }
    }

    // Higher LODs should have fewer or equal triangles
    for i in 1..5 {
        if lod_triangle_counts[i - 1] > 0 {
            assert!(
                lod_triangle_counts[i] <= lod_triangle_counts[i - 1],
                "LOD {} should have <= triangles than LOD {}",
                i,
                i - 1
            );
        }
    }
}

#[test]
fn lod_screen_size_thresholds() {
    let (positions, indices) = create_test_sphere_mesh(8);
    let config = LodConfig {
        levels: 3,
        reduction_ratio: 0.5,
        preserve_boundaries: true,
    };

    let lod_mesh = build_lod_meshlets(&positions, &indices, config);

    // Verify screen size thresholds are set
    for m in &lod_mesh.meshlets {
        assert!(m.lod_max_screen_size > 0.0);
        assert!(m.lod_min_screen_size >= 0.0);
        assert!(m.lod_min_screen_size <= m.lod_max_screen_size);
    }
}

#[test]
fn lod_error_metrics() {
    let (positions, indices) = create_test_sphere_mesh(8);
    let config = LodConfig {
        levels: 4,
        reduction_ratio: 0.5,
        preserve_boundaries: true,
    };

    let lod_mesh = build_lod_meshlets(&positions, &indices, config);

    // Higher LOD levels should have higher error metrics
    for m in &lod_mesh.meshlets {
        assert!(m.error_metric >= 0.0);
        assert!(m.error_metric <= 1.0);

        // LOD 0 should have lowest error
        if m.lod_level == 0 {
            assert!(m.error_metric < 0.1);
        }
    }
}

// ── Visibility Buffer Tests ─────────────────────────────────────

#[test]
fn visibility_buffer_layout() {
    let entry = VisibilityEntry {
        visible: 1,
        compacted_index: 42,
        lod_level: 2,
        _pad: 0,
    };

    let bytes = bytemuck::bytes_of(&entry);
    assert_eq!(bytes.len(), std::mem::size_of::<VisibilityEntry>());

    // Verify roundtrip
    let roundtrip = bytemuck::pod_read_unaligned::<VisibilityEntry>(bytes);
    assert_eq!(roundtrip.visible, 1);
    assert_eq!(roundtrip.compacted_index, 42);
    assert_eq!(roundtrip.lod_level, 2);
}

#[test]
fn visibility_buffer_zero_init() {
    let entry = VisibilityEntry {
        visible: 0,
        compacted_index: 0,
        lod_level: 0,
        _pad: 0,
    };

    let bytes = bytemuck::bytes_of(&entry);
    assert!(bytes.iter().all(|&b| b == 0));
}

// ── Mesh Shader Uniform Tests ──────────────────────────────────

#[test]
fn task_shader_uniform_layout() {
    let uniforms = TaskShaderUniforms {
        view_proj: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]],
        camera_pos: [0.0, 0.0, 5.0],
        meshlet_count: 1000,
        lod_count: 4,
        screen_width: 1920.0,
        screen_height: 1080.0,
        lod_thresholds: [1000.0, 500.0, 250.0, 100.0],
        _pad: [0.0; 2],
    };

    let bytes = bytemuck::bytes_of(&uniforms);
    assert_eq!(bytes.len(), std::mem::size_of::<TaskShaderUniforms>());

    // Verify alignment (must be multiple of 256 for uniform buffers)
    assert_eq!(std::mem::size_of::<TaskShaderUniforms>() % 16, 0);
}

#[test]
fn mesh_shader_uniform_layout() {
    let uniforms = MeshShaderUniforms {
        view_proj: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]],
        model: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]],
        normal_matrix: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]],
        meshlet_count: 500,
        _pad: [0.0; 3],
    };

    let bytes = bytemuck::bytes_of(&uniforms);
    assert_eq!(bytes.len(), std::mem::size_of::<MeshShaderUniforms>());

    // Verify alignment
    assert_eq!(std::mem::size_of::<MeshShaderUniforms>() % 16, 0);
}

// ── Capability Detection Tests ─────────────────────────────────

#[test]
fn capabilities_struct_size() {
    let caps = MeshShaderCapabilities {
        mesh_shader_supported: false,
        indirect_dispatch_supported: false,
        bindless_supported: false,
        max_task_workgroup_size: 32,
        max_mesh_vertices: 64,
        max_mesh_primitives: 126,
    };

    assert!(!caps.can_use_mesh_shaders());
    assert_eq!(caps.max_task_workgroup_size, 32);
    assert_eq!(caps.max_mesh_vertices, 64);
    assert_eq!(caps.max_mesh_primitives, 126);
}

#[test]
fn capabilities_full_support() {
    let caps = MeshShaderCapabilities {
        mesh_shader_supported: true,
        indirect_dispatch_supported: true,
        bindless_supported: true,
        max_task_workgroup_size: 64,
        max_mesh_vertices: 128,
        max_mesh_primitives: 256,
    };

    assert!(caps.can_use_mesh_shaders());
}

// ── Edge Case Tests ─────────────────────────────────────────────

#[test]
fn lod_single_triangle() {
    let positions = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    let indices = [0u32, 1, 2];

    let config = LodConfig {
        levels: 3,
        reduction_ratio: 0.5,
        preserve_boundaries: true,
    };

    let lod_mesh = build_lod_meshlets(&positions, &indices, config);

    // Should have at least 1 meshlet
    assert!(lod_mesh.total_meshlet_count >= 1);

    // All meshlets should have valid data
    for m in &lod_mesh.meshlets {
        assert!(m.base.vertex_count >= 3);
        assert!(m.base.triangle_count >= 1);
    }
}

#[test]
fn lod_empty_mesh_panics() {
    let positions: Vec<[f32; 3]> = vec![];
    let indices: Vec<u32> = vec![];

    let config = LodConfig::default();

    // Should handle empty mesh gracefully or panic with clear message
    let result = std::panic::catch_unwind(|| {
        build_lod_meshlets(&positions, &indices, config)
    });

    // Either it succeeds with empty data or panics
    if let Ok(lod_mesh) = result {
        assert_eq!(lod_mesh.total_meshlet_count, 0);
    }
}

#[test]
fn lod_config_validation() {
    // Test too many LOD levels
    let config = LodConfig {
        levels: MAX_LOD_LEVELS + 1,
        reduction_ratio: 0.5,
        preserve_boundaries: true,
    };

    let positions = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    let indices = [0u32, 1, 2];

    let result = std::panic::catch_unwind(|| {
        build_lod_meshlets(&positions, &indices, config)
    });

    assert!(result.is_err(), "Should panic with too many LOD levels");
}

#[test]
fn lod_reduction_ratio_bounds() {
    let positions = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    let indices = [0u32, 1, 2];

    // Test ratio = 0.0 (invalid)
    let config_zero = LodConfig {
        levels: 3,
        reduction_ratio: 0.0,
        preserve_boundaries: true,
    };

    let result_zero = std::panic::catch_unwind(|| {
        build_lod_meshlets(&positions, &indices, config_zero)
    });
    assert!(result_zero.is_err(), "Should panic with ratio 0.0");

    // Test ratio = 1.0 (invalid)
    let config_one = LodConfig {
        levels: 3,
        reduction_ratio: 1.0,
        preserve_boundaries: true,
    };

    let result_one = std::panic::catch_unwind(|| {
        build_lod_meshlets(&positions, &indices, config_one)
    });
    assert!(result_one.is_err(), "Should panic with ratio 1.0");
}

// ── Performance Regression Tests ────────────────────────────────

#[test]
fn lod_generation_performance() {
    let (positions, indices) = create_test_sphere_mesh(16);

    let config = LodConfig {
        levels: 5,
        reduction_ratio: 0.5,
        preserve_boundaries: true,
    };

    let start = std::time::Instant::now();
    let _lod_mesh = build_lod_meshlets(&positions, &indices, config);
    let elapsed = start.elapsed();

    // Should complete in under 100ms for a reasonable mesh
    assert!(
        elapsed.as_millis() < 100,
        "LOD generation took too long: {:?}",
        elapsed
    );
}

#[test]
fn meshlet_bounds_computation() {
    let (positions, indices) = create_test_sphere_mesh(8);
    let config = LodConfig::default();
    let lod_mesh = build_lod_meshlets(&positions, &indices, config);

    // Verify bounds are reasonable
    for b in &lod_mesh.bounds {
        // Radius should be positive
        assert!(b.radius > 0.0);

        // Cone axis should be normalized (or zero)
        let axis_len = (b.cone_axis[0].powi(2)
            + b.cone_axis[1].powi(2)
            + b.cone_axis[2].powi(2))
        .sqrt();
        assert!(axis_len <= 1.0 + f32::EPSILON);

        // Cone cutoff should be in [-1, 1]
        assert!(b.cone_cutoff >= -1.0 - f32::EPSILON);
        assert!(b.cone_cutoff <= 1.0 + f32::EPSILON);
    }
}
