//! Criterion benchmarks for quasar-render CPU-side work.
//!
//! These measure the CPU cost of shader-graph compilation, radiance-cache
//! injection/sampling, and other hot paths that don't require a live GPU.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use quasar_render::{RadianceCache, RadianceCacheSettings};

// ── Radiance Cache benchmarks ────────────────────────────────────

fn bench_radiance_inject(c: &mut Criterion) {
    let settings = RadianceCacheSettings::default();
    let mut cache = RadianceCache::new(settings);

    c.bench_function("radiance_inject_1k", |b| {
        b.iter(|| {
            for i in 0..1_000u32 {
                let pos = [
                    (i % 64) as f32 - 32.0,
                    ((i / 64) % 16) as f32 - 8.0,
                    ((i / 1024) % 64) as f32 - 32.0,
                ];
                let dir = [0.0_f32, 1.0, 0.0];
                let radiance = [1.0_f32, 0.8, 0.6];
                cache.inject(pos, dir, radiance);
            }
            black_box(&cache);
        });
    });
}

fn bench_radiance_sample(c: &mut Criterion) {
    let settings = RadianceCacheSettings::default();
    let mut cache = RadianceCache::new(settings);

    // Seed some data.
    for i in 0..5_000u32 {
        let pos = [
            (i % 64) as f32 - 32.0,
            ((i / 64) % 16) as f32 - 8.0,
            ((i / 1024) % 64) as f32 - 32.0,
        ];
        cache.inject(pos, [0.0, 1.0, 0.0], [1.0, 1.0, 1.0]);
    }

    c.bench_function("radiance_sample_1k", |b| {
        b.iter(|| {
            for i in 0..1_000u32 {
                let pos = [
                    (i % 50) as f32 - 25.0,
                    ((i / 50) % 12) as f32 - 6.0,
                    ((i / 600) % 50) as f32 - 25.0,
                ];
                let dir = [0.577_f32, 0.577, 0.577];
                black_box(cache.sample(pos, dir));
            }
        });
    });
}

// ── Shader graph compilation benchmark ───────────────────────────

fn bench_shader_compile(c: &mut Criterion) {
    use quasar_render::{MaterialDomain, MaterialGraph};

    let mut group = c.benchmark_group("material_compile");
    for node_count in [4, 16, 64] {
        group.bench_with_input(
            BenchmarkId::from_parameter(node_count),
            &node_count,
            |b, &n| {
                b.iter(|| {
                    let mg = MaterialGraph::new(format!("bench_{n}"), MaterialDomain::Surface);
                    // The graph starts minimal; we just benchmark the compile path.
                    let code = mg.compile().unwrap();
                    black_box(code);
                });
            },
        );
    }
    group.finish();
}

// ── Clustered Lighting benchmarks ────────────────────────────────

#[cfg(feature = "clustered-lighting")]
fn bench_cluster_cpu_assignment(c: &mut Criterion) {
    use quasar_render::{LightClusterGrid, PointLight, CLUSTER_X, CLUSTER_Y, CLUSTER_Z, TOTAL_CLUSTERS, MAX_LIGHTS_PER_CLUSTER};
    use quasar_math::Vec3;

    let mut group = c.benchmark_group("cluster_cpu_assignment");
    
    // Test with different light counts
    for &num_lights in &[50, 100, 250, 500] {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_lights),
            &num_lights,
            |b, &n| {
                let mut grid = LightClusterGrid::new(0.1, 100.0, 1920, 1080);
                
                // Create test lights distributed across the view frustum
                let lights: Vec<(PointLight, [f32; 3])> = (0..n)
                    .map(|i| {
                        let x = (i as f32 % 20.0 - 10.0) * 2.0;
                        let y = (i as f32 / 20.0 % 10.0 - 5.0) * 1.5;
                        let z = 5.0 + (i as f32 / 200.0) * 50.0;
                        let light = PointLight {
                            position: Vec3::new(x, y, z),
                            color: Vec3::new(1.0, 1.0, 1.0),
                            intensity: 1.0,
                            range: 8.0 + (i as f32 % 5.0) * 2.0,
                            falloff: 1.0,
                        };
                        // View position approximation
                        let view_pos = [x, y, z];
                        (light, view_pos)
                    })
                    .collect();

                b.iter(|| {
                    #[allow(deprecated)]
                    grid.assign_lights(&lights);
                    black_box(&grid.clusters);
                });
            },
        );
    }
    group.finish();
}

#[cfg(feature = "clustered-lighting")]
fn bench_cluster_grid_operations(c: &mut Criterion) {
    use quasar_render::{LightClusterGrid, CLUSTER_X, CLUSTER_Y, CLUSTER_Z, TOTAL_CLUSTERS};

    let mut group = c.benchmark_group("cluster_grid_operations");
    
    // Benchmark AABB rebuild
    group.bench_function("rebuild_aabbs_1080p", |b| {
        let mut grid = LightClusterGrid::new(0.1, 100.0, 1920, 1080);
        b.iter(|| {
            grid.rebuild_aabbs();
            black_box(&grid.aabbs);
        });
    });

    group.bench_function("rebuild_aabbs_4k", |b| {
        let mut grid = LightClusterGrid::new(0.1, 100.0, 3840, 2160);
        b.iter(|| {
            grid.rebuild_aabbs();
            black_box(&grid.aabbs);
        });
    });

    // Benchmark cluster grid creation
    group.bench_function("create_grid_1080p", |b| {
        b.iter(|| {
            let grid = LightClusterGrid::new(0.1, 100.0, 1920, 1080);
            black_box(grid);
        });
    });

    group.finish();
}

#[cfg(feature = "clustered-lighting")]
fn bench_cluster_sphere_aabb(c: &mut Criterion) {
    use quasar_render::ClusterAabb;

    // Benchmark the sphere-AABB intersection test (core building block)
    let mut group = c.benchmark_group("cluster_sphere_aabb");
    
    let aabb = ClusterAabb {
        min: [-1.0, -1.0, -1.0],
        _pad0: 0.0,
        max: [1.0, 1.0, 1.0],
        _pad1: 0.0,
    };

    group.bench_function("intersect_inside", |b| {
        b.iter(|| {
            let center = [0.0_f32, 0.0, 0.0];
            let radius = 0.5_f32;
            let mut dist_sq = 0.0_f32;
            for i in 0..3 {
                let v = center[i];
                let clamped = v.clamp(aabb.min[i], aabb.max[i]);
                dist_sq += (v - clamped) * (v - clamped);
            }
            black_box(dist_sq <= radius * radius);
        });
    });

    group.bench_function("intersect_outside", |b| {
        b.iter(|| {
            let center = [5.0_f32, 5.0, 5.0];
            let radius = 1.0_f32;
            let mut dist_sq = 0.0_f32;
            for i in 0..3 {
                let v = center[i];
                let clamped = v.clamp(aabb.min[i], aabb.max[i]);
                dist_sq += (v - clamped) * (v - clamped);
            }
            black_box(dist_sq <= radius * radius);
        });
    });

    group.finish();
}

#[cfg(feature = "clustered-lighting")]
fn bench_cluster_z_range(c: &mut Criterion) {
    use quasar_render::{LightClusterGrid, CLUSTER_Z};

    let mut group = c.benchmark_group("cluster_z_range");
    let grid = LightClusterGrid::new(0.1, 100.0, 1920, 1080);

    for &depth in &[1.0, 5.0, 10.0, 25.0, 50.0] {
        group.bench_with_input(
            BenchmarkId::from_parameter(depth),
            &depth,
            |b, &d| {
                b.iter(|| {
                    let pos = [0.0_f32, 0.0, d];
                    let radius = 5.0_f32;
                    black_box(grid.z_range_for_sphere(&pos, radius));
                });
            },
        );
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_radiance_inject,
    bench_radiance_sample,
    bench_shader_compile,
    #[cfg(feature = "clustered-lighting")]
    bench_cluster_cpu_assignment,
    #[cfg(feature = "clustered-lighting")]
    bench_cluster_grid_operations,
    #[cfg(feature = "clustered-lighting")]
    bench_cluster_sphere_aabb,
    #[cfg(feature = "clustered-lighting")]
    bench_cluster_z_range,
);
criterion_main!(benches);
