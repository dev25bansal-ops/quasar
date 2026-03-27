use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

fn bench_audio_mixing(c: &mut Criterion) {
    let mut group = c.benchmark_group("audio_mixing");

    fn mix_buffers(dst: &mut [f32], src: &[f32], volume: f32) {
        for (d, s) in dst.iter_mut().zip(src.iter()) {
            *d += *s * volume;
        }
    }

    let buffer_size = 1024;
    let mut dst = vec![0.0f32; buffer_size];
    let src: Vec<f32> = (0..buffer_size)
        .map(|i| (i as f32 / buffer_size as f32).sin())
        .collect();

    group.bench_function("mix_1024_samples", |b| {
        b.iter(|| {
            dst.fill(0.0);
            mix_buffers(black_box(&mut dst), black_box(&src), 0.5);
        });
    });

    let src2: Vec<f32> = (0..buffer_size)
        .map(|i| ((i * 2) as f32 / buffer_size as f32).sin())
        .collect();
    let src3: Vec<f32> = (0..buffer_size)
        .map(|i| ((i * 3) as f32 / buffer_size as f32).sin())
        .collect();

    group.bench_function("mix_3_buffers_1024", |b| {
        b.iter(|| {
            dst.fill(0.0);
            mix_buffers(black_box(&mut dst), black_box(&src), 0.33);
            mix_buffers(black_box(&mut dst), black_box(&src2), 0.33);
            mix_buffers(black_box(&mut dst), black_box(&src3), 0.33);
        });
    });

    group.finish();
}

fn bench_volume_ramp(c: &mut Criterion) {
    let mut group = c.benchmark_group("volume_ramp");

    fn apply_ramp(buffer: &mut [f32], start_volume: f32, end_volume: f32) {
        let len = buffer.len();
        for (i, s) in buffer.iter_mut().enumerate() {
            let t = i as f32 / len as f32;
            let volume = start_volume + (end_volume - start_volume) * t;
            *s *= volume;
        }
    }

    let buffer_size = 4096;
    let mut buffer: Vec<f32> = (0..buffer_size).map(|i| (i as f32 / 100.0).sin()).collect();

    group.bench_function("apply_ramp_4096", |b| {
        b.iter(|| {
            apply_ramp(black_box(&mut buffer), 1.0, 0.0);
        });
    });

    group.finish();
}

fn bench_resampling(c: &mut Criterion) {
    let mut group = c.benchmark_group("resampling");

    fn nearest_neighbor_resample(input: &[f32], output: &mut [f32]) {
        let ratio = input.len() as f32 / output.len() as f32;
        for (i, out) in output.iter_mut().enumerate() {
            let src_idx = (i as f32 * ratio) as usize;
            *out = input[src_idx.min(input.len() - 1)];
        }
    }

    let input_size = 44100;
    let input: Vec<f32> = (0..input_size).map(|i| (i as f32 / 100.0).sin()).collect();

    for output_size in [22050, 44100, 88200].iter() {
        let mut output = vec![0.0f32; *output_size];
        group.bench_with_input(
            BenchmarkId::new("resample", output_size),
            output_size,
            |b, _| {
                b.iter(|| {
                    nearest_neighbor_resample(black_box(&input), black_box(&mut output));
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_audio_mixing,
    bench_volume_ramp,
    bench_resampling,
);
criterion_main!(benches);
