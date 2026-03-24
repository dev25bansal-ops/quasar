# Queries

Queries are the primary way to access and iterate over components in Quasar's ECS. They provide efficient, cache-friendly access to entities with specific component combinations.

## Basic Queries

### Single Component

```rust,ignore
for (entity, position) in world.query::<Position>() {
    println!("Entity {} at ({}, {}, {})", entity, position.x, position.y, position.z);
}
```

### Multiple Components

```rust,ignore
for (entity, (pos, vel)) in world.query_2::<Position, Velocity>() {
    println!("Entity {} moving at ({}, {}, {})", entity, vel.dx, vel.dy, vel.dz);
}
```

### Streaming Iterator

For zero-allocation iteration:

```rust,ignore
for (entity, pos) in world.query_iter::<Position>() {
    // No allocation - directly iterates archetypes
}
```

## Query Types

### Immutable Queries

Read-only access:

```rust,ignore
for (entity, pos) in world.query::<Position>() {
    // Can only read pos
}
```

### Mutable Queries

Write access:

```rust,ignore
for (entity, pos) in world.query_mut::<Position>() {
    pos.x += 1.0;  // Can modify
}
```

### Mixed Mutability

```rust,ignore
for (entity, (pos, vel)) in world.query_mut_2::<Position, Velocity>() {
    pos.x += vel.dx;  // Modify position
    vel.dx *= 0.99;   // Modify velocity
}
```

## Query Filters

### With Filter

Include entities that have a component (without accessing it):

```rust,ignore
// All entities with Position AND Player tag
for (entity, pos) in world.query_filtered::<Position, With<Player>>() {
    // Only players
}
```

### Without Filter

Exclude entities that have a component:

```rust,ignore
// All entities with Position but NOT Static
for (entity, pos) in world.query_filtered::<Position, Without<Static>>() {
    // Non-static entities only
}
```

### Changed Filter

Only entities whose component changed:

```rust,ignore
for (entity, pos) in world.query_filtered::<Position, Changed<Position>>() {
    // Position was modified this frame
}
```

### Or Filter

```rust,ignore
for (entity, pos) in world.query_filtered::<Position, Or<With<Player>, With<Enemy>>>() {
    // Players or enemies
}
```

## Query Performance

### Zero-Allocation Iterators

Use streaming iterators for best performance:

```rust,ignore
// Allocates Vec
let results = world.query::<Position>();

// Zero allocation
for (entity, pos) in world.query_iter::<Position>() {
    // Process
}
```

### Cached Query State

For repeated queries, cache the state:

```rust,ignore
struct MovementSystem {
    query_state: CachedQueryState2<Position, Velocity>,
}

impl MovementSystem {
    fn update(&mut self, world: &World) {
        for (pos, vel) in self.query_state.iter(world) {
            pos.x += vel.dx;
            pos.y += vel.dy;
            pos.z += vel.dz;
        }
    }
}
```

### Archetype Caching

The query caches which archetypes match:

```rust,ignore
pub struct CachedQueryState<T: Component> {
    matching_archetypes: Vec<ArchetypeId>,
    // ...
}
```

## Query Patterns

### Update Pattern

```rust,ignore
fn update_positions(world: &mut World, dt: f32) {
    for (entity, (pos, vel)) in world.query_mut_2::<Position, Velocity>() {
        pos.x += vel.dx * dt;
        pos.y += vel.dy * dt;
        pos.z += vel.dz * dt;
    }
}
```

### Collision Detection

```rust,ignore
fn check_collisions(world: &World) -> Vec<(Entity, Entity)> {
    let positions: Vec<_> = world.query::<(Entity, Position, Collider)>().collect();
    let mut collisions = Vec::new();

    for i in 0..positions.len() {
        for j in (i + 1)..positions.len() {
            let (e1, p1, _) = positions[i];
            let (e2, p2, _) = positions[j];

            if p1.distance(p2) < COLLISION_THRESHOLD {
                collisions.push((e1, e2));
            }
        }
    }

    collisions
}
```

### Component Transfer

```rust,ignore
fn transfer_component<T: Component + Clone>(world: &mut World, from: Entity, to: Entity) {
    if let Some(component) = world.get::<T>(from).cloned() {
        world.remove::<T>(from);
        world.insert(to, component);
    }
}
```

### Entity Spawning from Query

```rust,ignore
fn spawn_particles(world: &mut World) {
    for (entity, pos) in world.query_filtered::<Position, With<Emitter>>() {
        // Spawn particle at emitter position
        let particle = world.spawn();
        world.insert(particle, pos.clone());
        world.insert(particle, Particle { lifetime: 1.0 });
    }
}
```

## Advanced Queries

### Query with Entity Commands

```rust,ignore
fn destroy_marked_entities(world: &mut World) {
    let to_destroy: Vec<Entity> = world.query_filtered::<Entity, With<Destroy>>()
        .map(|(e, _)| e)
        .collect();

    for entity in to_destroy {
        world.despawn(entity);
    }
}
```

### Nested Queries

```rust,ignore
fn find_nearby_allies(world: &World, entity: Entity, range: f32) -> Vec<Entity> {
    let my_pos = world.get::<Position>(entity)?;
    let my_faction = world.get::<Faction>(entity)?;

    world.query_filtered_2::<Entity, Position, With<Ally>>()
        .filter(|(e, pos)| {
            e != entity && pos.distance(my_pos) < range
        })
        .map(|(e, _)| e)
        .collect()
}
```

### Parallel Query Processing

```rust,ignore
use rayon::prelude::*;

fn process_entities_parallel(world: &World) {
    let entities: Vec<_> = world.query::<(Entity, Position, Velocity)>().collect();

    entities.par_iter().for_each(|(entity, pos, vel)| {
        // Parallel processing
    });
}
```

## Query Limitations

### Aliasing

Cannot have multiple mutable references to same component type:

```rust,ignore
// ERROR: Cannot have two mutable queries to same component
for (e, pos1) in world.query_mut::<Position>() { }
for (e, pos2) in world.query_mut::<Position>() { }  // Compile error
```

### Structural Mutation

Cannot modify archetype while iterating:

```rust,ignore
// ERROR: Cannot spawn/despawn during iteration
for (entity, pos) in world.query_iter::<Position>() {
    world.despawn(entity);  // Panic!
}

// Correct: collect first
let to_remove: Vec<Entity> = world.query_filtered::<Entity, With<Dead>>()
    .map(|(e, _)| e)
    .collect();

for entity in to_remove {
    world.despawn(entity);
}
```

## Query API Reference

### World Methods

| Method                     | Returns                 | Description              |
| -------------------------- | ----------------------- | ------------------------ |
| `query::<T>()`             | `Vec<(Entity, &T)>`     | Allocating query         |
| `query_mut::<T>()`         | `Vec<(Entity, &mut T)>` | Mutable allocating query |
| `query_iter::<T>()`        | `impl Iterator`         | Zero-allocation iterator |
| `query_iter_mut::<T>()`    | `impl Iterator`         | Mutable zero-allocation  |
| `query_filtered::<T, F>()` | `Vec<(Entity, &T)>`     | Filtered query           |
| `get::<T>(entity)`         | `Option<&T>`            | Single entity access     |
| `get_mut::<T>(entity)`     | `Option<&mut T>`        | Mutable single access    |

### Filter Types

| Filter       | Description             |
| ------------ | ----------------------- |
| `With<T>`    | Include entities with T |
| `Without<T>` | Exclude entities with T |
| `Changed<T>` | Only changed components |
| `Or<A, B>`   | Either filter matches   |
| `And<A, B>`  | Both filters match      |

## Best Practices

### 1. Use Streaming Iterators

```rust,ignore
// Bad - allocates
let results = world.query::<Position>();

// Good - zero allocation
for (e, pos) in world.query_iter::<Position>() { }
```

### 2. Batch Structural Changes

```rust,ignore
// Bad - modify archetype during iteration
for (e, pos) in world.query_iter::<Position>() {
    world.remove::<Position>(e);  // Panic!
}

// Good - defer changes
let to_process: Vec<_> = world.query::<Entity>().collect();
for e in to_process {
    world.remove::<Position>(e);
}
```

### 3. Filter Early

```rust,ignore
// Bad - filters in Rust code
for (e, pos) in world.query_iter::<Position>() {
    if world.get::<Player>(e).is_some() {
        // process
    }
}

// Good - filter at query level
for (e, pos) in world.query_filtered_iter::<Position, With<Player>>() {
    // process
}
```

## Next Steps

- [Entities](entities.md)
- [Components](components.md)
- [Systems](systems.md)
