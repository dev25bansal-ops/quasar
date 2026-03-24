# Components

Components are the data containers in Quasar's ECS architecture. They store state and are attached to entities.

## Defining Components

### Basic Component

```rust,ignore
use quasar_core::Component;

#[derive(Component, Clone)]
struct Position {
    x: f32,
    y: f32,
    z: f32,
}
```

All components must implement:

- `Component` - Marker trait
- `Clone` - Required for archetype storage

### Complex Components

```rust,ignore
#[derive(Component, Clone)]
struct Health {
    current: f32,
    max: f32,
    regeneration_rate: f32,
}

#[derive(Component, Clone)]
struct Inventory {
    items: Vec<Item>,
    capacity: usize,
    weight: f32,
}

#[derive(Component, Clone)]
struct Name(String);
```

### Tag Components

Zero-sized components for marking entities:

```rust,ignore
#[derive(Component, Clone, Copy)]
struct Player;

#[derive(Component, Clone, Copy)]
struct Enemy;

#[derive(Component, Clone, Copy)]
struct Static;
```

## Adding Components

### Single Component

```rust,ignore
let entity = world.spawn();
world.insert(entity, Position { x: 0.0, y: 0.0, z: 0.0 });
```

### Multiple Components

```rust,ignore
world.insert(entity, Position { x: 0.0, y: 0.0, z: 0.0 });
world.insert(entity, Velocity { dx: 1.0, dy: 0.0, dz: 0.0 });
world.insert(entity, Health { current: 100.0, max: 100.0, regeneration_rate: 0.0 });
```

### Conditional Insertion

```rust,ignore
// Only insert if entity exists
if world.is_alive(entity) {
    world.insert(entity, component);
}
```

## Removing Components

### Single Component

```rust,ignore
world.remove::<Position>(entity);
```

### Multiple Components

```rust,ignore
world.remove::<Position>(entity);
world.remove::<Velocity>(entity);
```

### Remove All Components

```rust,ignore
world.clear_components(entity);
```

## Accessing Components

### Read Access

```rust,ignore
if let Some(pos) = world.get::<Position>(entity) {
    println!("Position: {}, {}, {}", pos.x, pos.y, pos.z);
}
```

### Write Access

```rust,ignore
if let Some(pos) = world.get_mut::<Position>(entity) {
    pos.x += 1.0;
}
```

### Multiple Components

```rust,ignore
if let (Some(pos), Some(vel)) = (
    world.get::<Position>(entity),
    world.get::<Velocity>(entity),
) {
    // Use both
}
```

## Component Relationships

### Parent-Child Links

```rust,ignore
#[derive(Component, Clone)]
struct Parent(Entity);

#[derive(Component, Clone)]
struct Children(Vec<Entity>);
```

### References

```rust,ignore
#[derive(Component, Clone)]
struct Target(Entity);

#[derive(Component, Clone)]
struct Owner(Entity);
```

## Required Components

Some components depend on others:

```rust,ignore
// MeshRenderer requires Transform
impl MeshRenderer {
    pub fn new(path: &str) -> Self {
        Self { mesh: path.into() }
    }
}

// When adding MeshRenderer, ensure Transform exists
fn add_renderer(world: &mut World, entity: Entity, mesh: &str) {
    if world.get::<Transform>(entity).is_none() {
        world.insert(entity, Transform::default());
    }
    world.insert(entity, MeshRenderer::new(mesh));
}
```

## Change Detection

Track when components are modified:

```rust,ignore
#[derive(Component, Clone)]
struct Changed<T> {
    frame: u64,
    _marker: PhantomData<T>,
}

// In your system
fn detect_changes(world: &mut World, frame: u64) {
    for (entity, pos) in world.query_iter_mut::<Position>() {
        if pos.was_modified() {
            world.insert(entity, Changed::<Position> { frame, _marker: PhantomData });
        }
    }
}
```

## Component Storage

### Archetype Layout

Components are stored contiguously within archetypes:

```
Archetype [Position, Velocity]:
┌─────────────────┬─────────────────┐
│ Position (SOA)  │ Velocity (SOA)  │
├─────────────────┼─────────────────┤
│ (0, 0, 0)       │ (1, 0, 0)       │ Entity 0
│ (5, 2, 3)       │ (0, 0.5, 0)     │ Entity 1
│ (10, 0, 0)      │ (0, 0, 0)       │ Entity 2
└─────────────────┴─────────────────┘
```

This enables cache-friendly iteration.

### Memory Layout

Components are stored in Structure-of-Arrays (SOA) format:

```rust,ignore
// In memory:
// [pos.x, pos.x, pos.x, ...]
// [pos.y, pos.y, pos.y, ...]
// [pos.z, pos.z, pos.z, ...]
```

## Serialization

### JSON Serialization

```rust,ignore
#[derive(Component, Clone, Serialize, Deserialize)]
struct Position {
    x: f32,
    y: f32,
    z: f32,
}

let pos = Position { x: 1.0, y: 2.0, z: 3.0 };
let json = serde_json::to_string(&pos)?;
let decoded: Position = serde_json::from_str(&json)?;
```

### Scene Persistence

Components can be saved/loaded with scenes:

```rust,ignore
// Save entity with components
let data = EntityData {
    name: Some("Player".into()),
    transform: Transform::from_position(pos),
    mesh_shape: Some("Cube".into()),
    children: vec![],
};

// Write to scene file
scene.entities.push(data);
scene.save("scenes/level.scn")?;
```

## Performance Tips

### Hot vs Cold Data

Split frequently accessed data:

```rust,ignore
// Hot data - accessed every frame
#[derive(Component, Clone)]
struct Transform {
    position: Vec3,
    rotation: Quat,
    scale: Vec3,
}

// Cold data - rarely accessed
#[derive(Component, Clone)]
struct Metadata {
    name: String,
    description: String,
    tags: Vec<String>,
}
```

### Avoid Large Components

Large components hurt cache:

```rust,ignore
// Bad - 4KB component
#[derive(Component, Clone)]
struct LargeData {
    vertices: Vec<[f32; 3]>,  // Could be thousands of vertices
}

// Better - reference to data
#[derive(Component, Clone)]
struct MeshRef {
    handle: AssetHandle<Mesh>,
}
```

### Component Grouping

Group related data:

```rust,ignore
// Instead of:
#[derive(Component, Clone)] struct Health(f32);
#[derive(Component, Clone)] struct MaxHealth(f32);
#[derive(Component, Clone)] struct Armor(f32);

// Use:
#[derive(Component, Clone)]
struct CombatStats {
    health: f32,
    max_health: f32,
    armor: f32,
}
```

## Common Component Types

### Transform

```rust,ignore
#[derive(Component, Clone, Copy)]
struct Transform {
    position: Vec3,
    rotation: Quat,
    scale: Vec3,
}
```

### Physics

```rust,ignore
#[derive(Component, Clone)]
struct RigidBody {
    body_type: BodyType,
    mass: f32,
    velocity: Vec3,
    angular_velocity: Vec3,
}

#[derive(Component, Clone)]
struct Collider {
    shape: ColliderShape,
    offset: Vec3,
}
```

### Rendering

```rust,ignore
#[derive(Component, Clone)]
struct MeshRenderer {
    mesh: AssetHandle<Mesh>,
    material: AssetHandle<Material>,
    cast_shadows: bool,
}

#[derive(Component, Clone)]
struct Light {
    light_type: LightType,
    color: Vec3,
    intensity: f32,
    range: f32,
}
```

## Next Steps

- [Queries](queries.md) - How to iterate over components
- [Systems](systems.md) - How to process components
