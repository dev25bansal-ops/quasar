# quasar-derive

Procedural macros for the Quasar Engine.

## Derive Macros

### `#[derive(Inspect)]`

Generates inspector UI for editor:

```rust
#[derive(Inspect)]
struct Player {
    health: f32,
    name: String,
}
```

### `#[derive(Reflect)]`

Runtime type reflection:

```rust
#[derive(Reflect)]
struct Position {
    x: f32,
    y: f32,
    z: f32,
}
```

### `#[derive(Bundle)]`

ECS bundle trait implementation:

```rust
#[derive(Bundle)]
struct PlayerBundle {
    transform: Transform,
    mesh: MeshShape,
    body: RigidBody,
}
```

### `#[derive(Replicate)]`

Network replication:

```rust
#[derive(Replicate)]
struct NetworkedTransform {
    position: Vec3,
    rotation: Quat,
}
```

### `#[derive(SystemParam)]`

System parameter extraction:

```rust
#[derive(SystemParam)]
struct MyParam<'a> {
    query: Query<'a, &'static Transform>,
}
```
