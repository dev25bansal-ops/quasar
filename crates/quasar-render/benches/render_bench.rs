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

criterion_group!(
    benches,
    bench_radiance_inject,
    bench_radiance_sample,
    bench_shader_compile,
);
criterion_main!(benches);
