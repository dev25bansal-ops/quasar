//! Benchmarks for mesh shader pipeline CPU-side work.
//!
//! These measure the CPU cost of:
//! - LOD meshlet generation
//! - Meshlet clustering
//! - GPU buffer preparation
//! - Visibility buffer compaction (CPU simulation)

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use quasar_render::meshlet::{
    build_lod_meshlets, build_meshlets, LodConfig, LodMeshletMesh, MAX_MESHLET_TRIANGLES,
    MAX_MESHLET_VERTICES,
};

// ── Helper: generate test meshes ────────────────────────────────

fn create_plane_mesh(grid_size: u32) -> (Vec<[f32; 3]>, Vec<u32>) {
    let mut positions = Vec::new();
    let mut indices = Vec::new();

    for y in 0..=grid_size {
        for x in 0..=grid_size {
            positions.push([x as f32, 0.0, y as f32]);
        }
    }

    for y in 0..grid_size {
        for x in 0..grid_size {
            let tl = (y * (grid_size + 1) + x) as u32;
            let tr = tl + 1;
            let bl = tl + grid_size + 1;
            let br = bl + 1;
            indices.extend_from_slice(&[tl, bl, tr]);
            indices.extend_from_slice(&[tr, bl, br]);
        }
    }

    (positions, indices)
}

fn create_sphere_mesh(subdivisions: u32) -> (Vec<[f32; 3]>, Vec<u32>) {
    let slices = subdivisions;
    let stacks = subdivisions;
    let mut positions = Vec::new();
    let mut indices = Vec::new();

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

// ── Meshlet Clustering Benchmarks ───────────────────────────────

fn bench_meshlet_clustering(c: &mut Criterion) {
    let mut group = c.benchmark_group("meshlet_clustering");

    // Test different mesh sizes
    for &grid_size in &[16, 32, 64, 128] {
        let (positions, indices) = create_plane_mesh(grid_size);
        let tri_count = indices.len() / 3;

        group.bench_with_input(
            BenchmarkId::new("triangles", tri_count),
            &(&positions, &indices),
            |b, (positions, indices)| {
                b.iter(|| build_meshlets(black_box(positions), black_box(indices)))
            },
        );
    }

    group.finish();
}

// ── LOD Chain Generation Benchmarks ─────────────────────────────

fn bench_lod_generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("lod_generation");

    let config = LodConfig {
        levels: 5,
        reduction_ratio: 0.5,
        preserve_boundaries: true,
    };

    // Test different mesh complexities
    for &subdivisions in &[8, 12, 16, 24] {
        let (positions, indices) = create_sphere_mesh(subdivisions);
        let tri_count = indices.len() / 3;

        group.bench_with_input(
            BenchmarkId::new("sphere_triangles", tri_count),
            &(&positions, &indices),
            |b, (positions, indices)| {
                b.iter(|| {
                    build_lod_meshlets(black_box(positions), black_box(indices), config)
                })
            },
        );
    }

    group.finish();
}

// ── LOD Configuration Benchmarks ────────────────────────────────

fn bench_lod_configurations(c: &mut Criterion) {
    let mut group = c.benchmark_group("lod_configurations");

    let (positions, indices) = create_sphere_mesh(16);

    // Test different LOD configurations
    for &levels in &[2, 4, 6, 8] {
        for &ratio in &[0.3, 0.5, 0.7] {
            let config = LodConfig {
                levels,
                reduction_ratio: ratio,
                preserve_boundaries: true,
            };

            group.bench_with_input(
                BenchmarkId::from_parameter(format!("L{}_{:.1}", levels, ratio)),
                &(&positions, &indices),
                |b, (positions, indices)| {
                    b.iter(|| {
                        build_lod_meshlets(black_box(positions), black_box(indices), config)
                    })
                },
            );
        }
    }

    group.finish();
}

// ── Meshlet Validation Benchmarks ───────────────────────────────

fn bench_meshlet_validation(c: &mut Criterion) {
    let (positions, indices) = create_plane_mesh(64);
    let lod_mesh = build_lod_meshlets(
        &positions,
        &indices,
        LodConfig {
            levels: 4,
            reduction_ratio: 0.5,
            preserve_boundaries: true,
        },
    );

    c.bench_function("validate_meshlet_bounds", |b| {
        b.iter(|| {
            for m in &lod_mesh.bounds {
                black_box(m.radius > 0.0);
                black_box(m.cone_cutoff <= 1.0);
            }
        })
    });

    c.bench_function("validate_lod_assignments", |b| {
        b.iter(|| {
            for m in &lod_mesh.meshlets {
                black_box(m.lod_level < 4);
                black_box(m.base.vertex_count as usize <= MAX_MESHLET_VERTICES);
                black_box(m.base.triangle_count as usize <= MAX_MESHLET_TRIANGLES);
            }
        })
    });
}

// ── Large Mesh Benchmarks (Nanite-style) ────────────────────────

fn bench_large_mesh_nannite_style(c: &mut Criterion) {
    let mut group = c.benchmark_group("nanite_style_meshlets");

    // Test with meshes that would be typical for Nanite-style rendering
    // (10K - 100K triangles)
    for &size in &[50, 100, 150, 200] {
        let (positions, indices) = create_plane_mesh(size);
        let tri_count = indices.len() / 3;
        let vert_count = positions.len();

        group.bench_with_input(
            BenchmarkId::new("mesh_size", format!("{}K tris", tri_count / 1000)),
            &(&positions, &indices),
            |b, (positions, indices)| {
                b.iter(|| {
                    let config = LodConfig {
                        levels: 6,
                        reduction_ratio: 0.5,
                        preserve_boundaries: true,
                    };
                    build_lod_meshlets(black_box(positions), black_box(indices), config)
                })
            },
        );

        println!(
            "Mesh: {} vertices, {} triangles",
            vert_count, tri_count
        );
    }

    group.finish();
}

// ── Benchmark Group Registration ────────────────────────────────

criterion_group!(
    mesh_shader_benches,
    bench_meshlet_clustering,
    bench_lod_generation,
    bench_lod_configurations,
    bench_meshlet_validation,
    bench_large_mesh_nannite_style,
);

criterion_main!(mesh_shader_benches);
