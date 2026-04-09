use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use quasar_core::event_bus::EventBus;

#[derive(Debug, Clone)]
struct TestEvent {
    value: u32,
}

#[derive(Debug, Clone)]
struct LargeEvent {
    data: [u8; 256],
}

fn bench_event_send(c: &mut Criterion) {
    let mut group = c.benchmark_group("event_send");
    for count in [100, 1_000, 10_000] {
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &n| {
            let mut bus = EventBus::new();
            b.iter(|| {
                for i in 0..n {
                    bus.send(TestEvent { value: i });
                }
                black_box(&bus);
            });
        });
    }
    group.finish();
}

fn bench_event_send_with_priority(c: &mut Criterion) {
    let mut group = c.benchmark_group("event_send_priority");
    for count in [100, 1_000, 10_000] {
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &n| {
            let mut bus = EventBus::new();
            b.iter(|| {
                for i in 0..n {
                    bus.send_with_priority(TestEvent { value: i }, (i % 10) as u32);
                }
                black_box(&bus);
            });
        });
    }
    group.finish();
}

fn bench_event_send_batch(c: &mut Criterion) {
    let mut group = c.benchmark_group("event_send_batch");
    for count in [100, 1_000, 10_000] {
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &n| {
            let mut bus = EventBus::new();
            b.iter(|| {
                let events: Vec<_> = (0..n).map(|i| TestEvent { value: i }).collect();
                bus.send_batch(events);
                black_box(&bus);
            });
        });
    }
    group.finish();
}

fn bench_event_read_single_reader(c: &mut Criterion) {
    let mut group = c.benchmark_group("event_read_single");
    for count in [100, 1_000, 10_000] {
        let mut bus = EventBus::new();
        for i in 0..count {
            bus.send(TestEvent { value: i });
        }
        let reader_id = bus.register_reader::<TestEvent>();

        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, _| {
            b.iter(|| {
                let events = bus.read::<TestEvent>(reader_id);
                black_box(events.len());
            });
        });
    }
    group.finish();
}

fn bench_event_read_multiple_readers(c: &mut Criterion) {
    let mut group = c.benchmark_group("event_read_multiple");
    for reader_count in [2, 5, 10] {
        let mut bus = EventBus::new();
        for i in 0..1_000 {
            bus.send(TestEvent { value: i });
        }
        let reader_ids: Vec<_> = (0..reader_count)
            .map(|_| bus.register_reader::<TestEvent>())
            .collect();

        group.bench_with_input(
            BenchmarkId::from_parameter(reader_count),
            &reader_count,
            |b, _| {
                b.iter(|| {
                    for &reader_id in &reader_ids {
                        let events = bus.read::<TestEvent>(reader_id);
                        black_box(events.len());
                    }
                });
            },
        );
    }
    group.finish();
}

fn bench_register_reader(c: &mut Criterion) {
    let bus = EventBus::new();
    c.bench_function("register_reader", |b| {
        b.iter(|| {
            let reader_id = bus.register_reader::<TestEvent>();
            black_box(reader_id);
        });
    });
}

fn bench_large_event_send(c: &mut Criterion) {
    let mut group = c.benchmark_group("large_event_send");
    for count in [100, 1_000, 10_000] {
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &n| {
            let mut bus = EventBus::new();
            b.iter(|| {
                for i in 0..n {
                    bus.send(LargeEvent {
                        data: [i as u8; 256],
                    });
                }
                black_box(&bus);
            });
        });
    }
    group.finish();
}

fn bench_large_event_read(c: &mut Criterion) {
    let mut group = c.benchmark_group("large_event_read");
    for count in [100, 1_000, 10_000] {
        let mut bus = EventBus::new();
        for i in 0..count {
            bus.send(LargeEvent {
                data: [i as u8; 256],
            });
        }
        let reader_id = bus.register_reader::<LargeEvent>();

        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, _| {
            b.iter(|| {
                let events = bus.read::<LargeEvent>(reader_id);
                black_box(events.len());
            });
        });
    }
    group.finish();
}

fn bench_clear_events(c: &mut Criterion) {
    let mut group = c.benchmark_group("event_clear");
    for count in [100, 1_000, 10_000] {
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &n| {
            b.iter_batched(
                || {
                    let mut bus = EventBus::new();
                    for i in 0..n {
                        bus.send(TestEvent { value: i });
                    }
                    bus
                },
                |mut bus| {
                    bus.clear::<TestEvent>();
                    black_box(&bus);
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

fn bench_clear_all(c: &mut Criterion) {
    let mut group = c.benchmark_group("event_clear_all");
    for count in [100, 1_000, 10_000] {
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &n| {
            b.iter_batched(
                || {
                    let mut bus = EventBus::new();
                    for i in 0..n {
                        bus.send(TestEvent { value: i });
                        bus.send(LargeEvent {
                            data: [i as u8; 256],
                        });
                    }
                    bus
                },
                |mut bus| {
                    bus.clear_all();
                    black_box(&bus);
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_event_send,
    bench_event_send_with_priority,
    bench_event_send_batch,
    bench_event_read_single_reader,
    bench_event_read_multiple_readers,
    bench_register_reader,
    bench_large_event_send,
    bench_large_event_read,
    bench_clear_events,
    bench_clear_all,
);
criterion_main!(benches);
