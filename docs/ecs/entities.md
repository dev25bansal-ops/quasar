# Entities

Entities are the fundamental objects in Quasar's ECS architecture. An entity is simply a unique identifier (ID) that can have components attached to it.

## Entity IDs

Entities are represented by `Entity`, which contains:

- **Index**: A 32-bit index into entity storage
- **Generation**: A version counter to detect stale references

```rust,ignore
pub struct Entity {
    index: u32,
    generation: u32,
}
```

The generation counter allows detection of dangling references after an entity is despawned.

## Creating Entities

### Spawn

```rust,ignore
let entity = world.spawn();
```

This creates an empty entity with no components.

### Spawn with Components

```rust,ignore
let entity = world.spawn();
world.insert(entity, Position { x: 0.0, y: 0.0, z: 0.0 });
world.insert(entity, Velocity { dx: 1.0, dy: 0.0, dz: 0.0 });
world.insert(entity, MeshRenderer::new("models/player.glb"));
```

### Entity Builder

For ergonomic entity creation:

```rust,ignore
let entity = EntityBuilder::new()
    .with(Position { x: 0.0, y: 0.0, z: 0.0 })
    .with(Velocity { dx: 1.0, dy: 0.0, dz: 0.0 })
    .with(Name("Player".to_string()))
    .spawn(&mut world);
```

## Destroying Entities

### Despawn

```rust,ignore
world.despawn(entity);
```

This removes the entity and all its components. The entity ID is recycled, with an incremented generation.

### Despawn with Children

When despawning a parent, all children are also despawned:

```rust,ignore
world.despawn_recursive(parent_entity);
```

## Entity Relationships

### Parent-Child Hierarchy

```rust,ignore
let parent = world.spawn();
world.insert(parent, Position::default());

let child = world.spawn();
world.insert(child, Position { x: 1.0, y: 0.0, z: 0.0 });
world.insert(child, Parent(parent));
```

### Querying Hierarchy

```rust,ignore
// Get all children of an entity
for child in world.children(parent) {
    // Process child
}

// Get parent
if let Some(parent) = world.parent(child) {
    // Process parent
}
```

## Entity Commands

For deferred entity operations:

```rust,ignore
// Commands are batched and applied at end of system
let mut cmds = Commands::new();
cmds.spawn()
    .insert(Position::default())
    .insert(Velocity::default());

// Later in the frame
cmds.apply(&mut world);
```

## Entity Reference Safety

### Stale References

```rust,ignore
let e1 = world.spawn();
world.despawn(e1);

let e2 = world.spawn(); // May reuse e1's index

// e1 is now stale - its generation doesn't match
assert!(!world.is_alive(e1));
```

### Safe Access

```rust,ignore
if world.is_alive(entity) {
    if let Some(pos) = world.get::<Position>(entity) {
        // Safe to access
    }
}
```

## Performance Considerations

### Archetype Migration

When components are added/removed, entities migrate between archetypes:

```rust,ignore
// Entity starts in archetype [Position]
let e = world.spawn();
world.insert(e, Position::default());

// Migrates to archetype [Position, Velocity]
world.insert(e, Velocity::default());

// Migrates to archetype [Velocity]
world.remove::<Position>(e);
```

Migration has a small cost. Batch component insertions when possible:

```rust,ignore
// Slower - multiple migrations
world.insert(e, Position::default());
world.insert(e, Velocity::default());
world.insert(e, Health::default());

// Faster - single batch (if API supports)
world.insert_batch(e, (Position::default(), Velocity::default(), Health::default()));
```

### Entity Spawning Batch

For spawning many entities:

```rust,ignore
// Spawn 1000 entities efficiently
let entities: Vec<Entity> = (0..1000)
    .map(|_| world.spawn())
    .collect();
```

## Common Patterns

### Tag Entities

Use empty structs as tags:

```rust,ignore
#[derive(Component)]
struct Player;

#[derive(Component)]
struct Enemy;

#[derive(Component)]
struct Destroyed;

// Query by tag
for (entity, _) in world.query_iter::<Player>() {
    // Process player
}
```

### Entity as Component Reference

```rust,ignore
#[derive(Component)]
struct Target(Option<Entity>);

#[derive(Component)]
struct Weapon { owner: Entity }
```

### Entity Prefabs

```rust,ignore
fn spawn_player(world: &mut World, position: Position) -> Entity {
    let entity = world.spawn();
    world.insert(entity, position);
    world.insert(entity, Velocity::default());
    world.insert(entity, Health { current: 100, max: 100 });
    world.insert(entity, Player);
    world.insert(entity, MeshRenderer::new("models/player.glb"));
    entity
}
```

## Next Steps

- [Components](components.md)
- [Queries](queries.md)
