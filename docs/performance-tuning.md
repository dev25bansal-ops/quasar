# Performance Tuning Guide

This guide covers techniques for optimizing Quasar Engine performance across all subsystems.

## Table of Contents

1. [ECS Performance](#ecs-performance)
2. [Rendering Performance](#rendering-performance)
3. [Physics Performance](#physics-performance)
4. [Audio Performance](#audio-performance)
5. [Networking Performance](#networking-performance)
6. [Memory Management](#memory-management)
7. [Profiling Tools](#profiling-tools)

---

## ECS Performance

### Query Optimization

**Use streaming iterators instead of allocating queries:**

```rust,ignore
// Bad - allocates Vec
let results = world.query::<Position>();

// Good - zero allocation
for (entity, pos) in world.query_iter::<Position>() {
    // Process
}
```

**Cache query state for repeated queries:**

```rust,ignore
struct MovementSystem {
    query_state: CachedQueryState2<Position, Velocity>,
}

impl System for MovementSystem {
    fn run(&mut self, world: &mut World) {
        for (pos, vel) in self.query_state.iter(world) {
            pos.x += vel.dx;
        }
    }
}
```

**Filter early at query level:**

```rust,ignore
// Bad - filters in Rust code
for (e, pos) in world.query_iter::<Position>() {
    if world.get::<Player>(e).is_some() { }
}

// Good - filter at query level
for (e, pos) in world.query_filtered_iter::<Position, With<Player>>() { }
```

### Archetype Considerations

**Group frequently queried components together:**

```rust,ignore
// Good - Position and Velocity often queried together
#[derive(Component)]
struct Transform { position: Vec3, rotation: Quat }

#[derive(Component)]
struct Velocity { linear: Vec3, angular: Vec3 }
```

**Avoid archetype fragmentation:**

```rust,ignore
// Bad - creates many archetypes
// Entity has A, Entity has B, Entity has A+B, Entity has A+B+C...
// This creates 2^n archetypes for n optional components

// Good - use marker components sparingly
#[derive(Component)] struct Player;
#[derive(Component)] struct Enemy;
// Instead of Optional<Player>, Optional<Enemy>...
```

### Entity Operations

**Batch structural changes:**

```rust,ignore
// Bad - triggers archetype migration per operation
for i in 0..1000 {
    let e = world.spawn();
    world.insert(e, Position::default());
    world.insert(e, Velocity::default());
}

// Good - batch spawn first
let entities: Vec<_> = (0..1000).map(|_| world.spawn()).collect();
for e in &entities {
    world.insert(*e, Position::default());
    world.insert(*e, Velocity::default());
}
```

---

## Rendering Performance

### Draw Call Batching

**Use instanced rendering:**

```rust,ignore
// Bad - separate draw per object
for mesh in &meshes {
    pass.draw_mesh(mesh);
}

// Good - instanced draw
pass.draw_instanced(&mesh, instances.len());
```

**Batch similar materials:**

```rust,ignore
// Sort by material to minimize state changes
renderables.sort_by_key(|r| r.material_id);
```

### GPU Culling

**Enable GPU-driven culling:**

```rust,ignore
let config = RenderConfig {
    gpu_culling: true,
    occlusion_culling: true,
    lod_enabled: true,
};
```

### Texture Management

**Use texture atlases for sprites:**

```rust,ignore
// Bad - separate texture per sprite
for sprite in &sprites {
    bind_texture(sprite.texture);
    draw(sprite);
}

// Good - atlas with UV offsets
bind_texture(atlas);
for sprite in &sprites {
    draw_with_uv(sprite.uv_rect);
}
```

**Configure virtual textures for large scenes:**

```rust,ignore
let svt_config = SvtConfig {
    tile_size: 128,
    page_table_size: 256,
    max_physical_tiles: 4096,
};
```

### Shader Optimization

**Minimize texture samples:**

```wgsl
// Bad - multiple samples
let a = textureSample(t, s, uv);
let b = textureSample(t, s, uv + offset);

// Good - reuse sample
let base = textureSample(t, s, uv);
let a = base;
let b = textureSample(t, s, uv + offset);
```

**Use compute for parallel operations:**

```rust,ignore
// Compute-based particle update
let compute_pass = encoder.begin_compute_pass(&ComputePassDescriptor::default());
compute_pass.set_pipeline(&particle_update_pipeline);
compute_pass.dispatch_workgroups(particle_count / 256, 1, 1);
```

---

## Physics Performance

### Fixed Timestep

**Use deterministic fixed timestep:**

```rust,ignore
let accumulator = FixedUpdateAccumulator::new(60.0); // 60 Hz

fn physics_system(world: &mut World) {
    while accumulator.tick() {
        step_physics(world, 1.0 / 60.0);
    }
}
```

### Collision Layers

**Use collision layers to reduce checks:**

```rust,ignore
let collision_matrix = CollisionMatrix::new()
    .with_interaction(GameLayer::Player, GameLayer::Enemy, true)
    .with_interaction(GameLayer::Player, GameLayer::Environment, true)
    .with_interaction(GameLayer::Enemy, GameLayer::Enemy, false);
```

### Sleep States

**Configure sleep thresholds:**

```rust,ignore
let config = PhysicsConfig {
    sleep_threshold: 0.1,
    sleep_time: 1.0,
    max_depenetration_iterations: 10,
};
```

---

## Audio Performance

### Audio Bus Limits

**Limit concurrent sounds:**

```rust,ignore
let config = AudioConfig {
    max_concurrent_sounds: 32,
    virtual_voice_count: 64,
    real_voice_count: 32,
};
```

### Streaming Audio

**Use streaming for long audio:**

```rust,ignore
// Music - stream from disk
let music = audio.load_streaming("music/track01.ogg")?;

// SFX - load into memory
let sfx = audio.load_static("sfx/jump.wav")?;
```

### 3D Audio Limits

**Limit spatial audio sources:**

```rust,ignore
// Only closest N sources play at full volume
let config = SpatialAudioConfig {
    max_sources: 16,
    attenuation_model: AttenuationModel::InverseDistance,
    listener_update_rate: 60.0,
};
```

---

## Networking Performance

### Delta Compression

**Enable state delta compression:**

```rust,ignore
let config = NetworkConfig {
    delta_compression: true,
    snapshot_interval: 3, // Every 3rd frame
    max_entity_updates_per_frame: 100,
};
```

### Interest Management

**Use spatial interest management:**

```rust,ignore
let interest = InterestManager::new()
    .with_grid(CellSize::new(100.0, 100.0))
    .with_view_distance(500.0);
```

### Message Batching

**Batch network messages:**

```rust,ignore
// Bad - many small packets
for entity in entities {
    send(NetworkPayload::EntityUpdate { entity, ... });
}

// Good - batch in one message
send(NetworkPayload::StateSnapshot { entities, ... });
```

---

## Memory Management

### Object Pools

**Use object pools for frequent allocations:**

```rust,ignore
let pool = MessagePool::new(1024, 4096); // 1024 buffers of 4KB

let mut buffer = pool.acquire();
buffer.extend_from_slice(&data);
// Buffer returned to pool on drop
```

### Arena Allocation

**Use arenas for frame-temporary data:**

```rust,ignore
let arena = bumpalo::Bump::new();

for _ in 0..1000 {
    let temp: &mut Vec<u8> = arena.alloc(vec![0; 1024]);
    // No individual deallocation needed
}
// Entire arena freed at once
```

### Texture Memory

**Monitor GPU memory:**

```rust,ignore
let tracker = GpuMemoryTracker::new(device);

fn frame() {
    let stats = tracker.stats();
    if stats.texture_bytes > 512 * 1024 * 1024 {
        log::warn!("High texture memory usage: {} MB", stats.texture_bytes / 1024 / 1024);
    }
}
```

---

## Profiling Tools

### Built-in Profiler

**Enable frame profiling:**

```rust,ignore
let profiler = Profiler::new();

fn frame() {
    profiler.begin_frame();

    profiler.begin_scope("update");
    update_systems();
    profiler.end_scope("update");

    profiler.begin_scope("render");
    render();
    profiler.end_scope("render");

    let stats = profiler.end_frame();
    println!("Frame time: {:.2}ms", stats.frame_time_ms);
}
```

### Frame Budget

**Monitor frame budget:**

```rust,ignore
let budget = FrameBudget::new(16.67); // 60 FPS target

fn frame() {
    if budget.remaining_ms() < 5.0 {
        log::warn!("Frame budget exceeded!");
    }
}
```

### GPU Profiler

**Profile GPU commands:**

```rust,ignore
let gpu_profiler = GpuProfiler::new(device);

gpu_profiler.begin_pass(encoder, "shadow_pass");
render_shadows(encoder);
gpu_profiler.end_pass(encoder);

gpu_profiler.begin_pass(encoder, "opaque_pass");
render_opaque(encoder);
gpu_profiler.end_pass(encoder);

let timings = gpu_profiler.resolve();
println!("Shadow: {:.2}ms, Opaque: {:.2}ms",
    timings["shadow_pass"], timings["opaque_pass"]);
```

### Tracy Integration

**Connect to Tracy profiler:**

```rust,ignore
// Enable tracy feature in Cargo.toml
// tracy-client = "0.17"

tracy_client::frame_mark();

{
    let _span = tracy_client::span!("Update Systems");
    update();
}
```

---

## Performance Checklist

### Startup Performance

- [ ] Use incremental compilation
- [ ] Lazy-load assets
- [ ] Profile startup sequence

### Runtime Performance

- [ ] Target 60 FPS (16.67ms budget)
- [ ] Profile before optimizing
- [ ] Monitor memory usage
- [ ] Use object pools for frequent allocations

### Rendering

- [ ] Batch draw calls
- [ ] Use instanced rendering
- [ ] Enable GPU culling
- [ ] Optimize shader complexity

### Physics

- [ ] Use fixed timestep
- [ ] Configure collision layers
- [ ] Enable sleep states

### Networking

- [ ] Enable delta compression
- [ ] Use interest management
- [ ] Batch messages

---

## Common Bottlenecks

| Bottleneck              | Symptoms                | Solution                      |
| ----------------------- | ----------------------- | ----------------------------- |
| Too many draw calls     | Low FPS, GPU waiting    | Batch by material, instancing |
| Archetype fragmentation | Slow queries            | Reduce component combinations |
| Excessive allocations   | Memory churn, GC pauses | Use pools, arenas             |
| Physics step too long   | Frame spikes            | Increase sleep threshold      |
| Network bandwidth       | Lag, packet loss        | Delta compression, interest   |

---

## Recommended Tools

- **Tracy Profiler** - Real-time frame profiling
- **RenderDoc** - GPU frame capture
- **PIX for Windows** - DirectX debugging
- **Instruments** (macOS) - System profiling
- **perf** (Linux) - CPU profiling
- **heaptrack** - Memory profiling
