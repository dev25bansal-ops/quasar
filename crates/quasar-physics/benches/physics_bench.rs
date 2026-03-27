use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_rigid_body_creation(c: &mut Criterion) {
    c.bench_function("rigid_body_creation_1000", |b| {
        b.iter(|| {
            let mut bodies = rapier3d::prelude::RigidBodySet::new();
            for i in 0..1000 {
                let body = rapier3d::prelude::RigidBodyBuilder::dynamic()
                    .translation(rapier3d::prelude::vector![i as f32 * 0.1, 0.0, 0.0])
                    .build();
                bodies.insert(body);
            }
            black_box(bodies);
        });
    });
}

fn bench_collider_creation(c: &mut Criterion) {
    c.bench_function("collider_creation_1000", |b| {
        b.iter(|| {
            let mut colliders = rapier3d::prelude::ColliderSet::new();
            for _ in 0..1000 {
                let collider = rapier3d::prelude::ColliderBuilder::cuboid(0.5, 0.5, 0.5).build();
                let mut bodies = rapier3d::prelude::RigidBodySet::new();
                let body = rapier3d::prelude::RigidBodyBuilder::dynamic().build();
                let handle = bodies.insert(body);
                colliders.insert_with_parent(collider, handle, &mut bodies);
            }
            black_box(colliders);
        });
    });
}

fn bench_collider_shape_creation(c: &mut Criterion) {
    c.bench_function("collider_shape_1000", |b| {
        b.iter(|| {
            let mut shapes: Vec<rapier3d::prelude::SharedShape> = Vec::new();
            for _ in 0..1000 {
                shapes.push(rapier3d::prelude::SharedShape::cuboid(0.5, 0.5, 0.5));
            }
            black_box(shapes);
        });
    });
}

criterion_group!(
    benches,
    bench_rigid_body_creation,
    bench_collider_creation,
    bench_collider_shape_creation,
);
criterion_main!(benches);
