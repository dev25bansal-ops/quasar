use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_delta_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("delta_encode");

    let data_small: Vec<u8> = (0..64).map(|i| i as u8).collect();
    let data_medium: Vec<u8> = (0..255).cycle().take(1024).collect();
    let data_large: Vec<u8> = (0..255).cycle().take(65536).collect();

    group.bench_function("delta_encode_64", |b| {
        b.iter(|| {
            let mut prev = data_small.clone();
            for (i, v) in prev.iter_mut().enumerate() {
                *v = black_box(*v ^ (i as u8));
            }
        });
    });

    group.bench_function("delta_encode_1024", |b| {
        b.iter(|| {
            let mut prev = data_medium.clone();
            for (i, v) in prev.iter_mut().enumerate() {
                *v = black_box(*v ^ (i as u8));
            }
        });
    });

    group.bench_function("delta_encode_65536", |b| {
        b.iter(|| {
            let mut prev = data_large.clone();
            for (i, v) in prev.iter_mut().enumerate() {
                *v = black_box(*v ^ (i as u8));
            }
        });
    });

    group.finish();
}

fn bench_message_serialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_serialize");

    #[derive(serde::Serialize, serde::Deserialize, Clone)]
    struct NetworkMessage {
        sequence: u32,
        timestamp: f64,
        entities: Vec<EntityUpdate>,
    }

    #[derive(serde::Serialize, serde::Deserialize, Clone)]
    struct EntityUpdate {
        id: u64,
        position: [f32; 3],
        rotation: [f32; 4],
    }

    let message = NetworkMessage {
        sequence: 1,
        timestamp: 0.0,
        entities: (0..50)
            .map(|i| EntityUpdate {
                id: i,
                position: [i as f32, 0.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
            })
            .collect(),
    };

    group.bench_function("bincode_serialize", |b| {
        b.iter(|| {
            let _ = bincode::serde::encode_to_vec(black_box(&message), bincode::config::standard());
        });
    });

    group.bench_function("json_serialize", |b| {
        b.iter(|| {
            let _ = serde_json::to_string(black_box(&message));
        });
    });

    group.finish();
}

fn bench_message_deserialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_deserialize");

    #[derive(serde::Serialize, serde::Deserialize, Clone)]
    struct NetworkMessage {
        sequence: u32,
        timestamp: f64,
        entities: Vec<EntityUpdate>,
    }

    #[derive(serde::Serialize, serde::Deserialize, Clone)]
    struct EntityUpdate {
        id: u64,
        position: [f32; 3],
        rotation: [f32; 4],
    }

    let message = NetworkMessage {
        sequence: 1,
        timestamp: 0.0,
        entities: (0..50)
            .map(|i| EntityUpdate {
                id: i,
                position: [i as f32, 0.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
            })
            .collect(),
    };

    let bincode_data =
        bincode::serde::encode_to_vec(&message, bincode::config::standard()).unwrap();
    let json_data = serde_json::to_string(&message).unwrap();

    group.bench_function("bincode_deserialize", |b| {
        b.iter(|| {
            let _: (NetworkMessage, _) = bincode::serde::decode_from_slice(
                black_box(&bincode_data),
                bincode::config::standard(),
            )
            .unwrap();
        });
    });

    group.bench_function("json_deserialize", |b| {
        b.iter(|| {
            let _: NetworkMessage = serde_json::from_str(black_box(&json_data)).unwrap();
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_delta_encode,
    bench_message_serialize,
    bench_message_deserialize,
);
criterion_main!(benches);
