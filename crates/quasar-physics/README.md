# quasar-physics

Physics simulation with rigid bodies and colliders for the Quasar Engine.

## Features

- **Rigid Bodies**: Dynamic, kinematic, and static bodies
- **Colliders**: Box, sphere, capsule, mesh, heightfield
- **Joints**: Fixed, prismatic, revolute, spherical
- **Character Controller**: Auto-step, slope limits
- **Sensors**: Trigger volumes
- **Raycasting**: Ray and shape casting
- **Rollback**: Deterministic snapshots for netcode

## Usage

```rust
use quasar_physics::{PhysicsPlugin, RigidBody, Collider};

app.add_plugin(PhysicsPlugin);

let entity = world.spawn();
world.insert(entity, RigidBody::dynamic());
world.insert(entity, Collider::cuboid(1.0, 1.0, 1.0));
```

## Integration

Built on `rapier3d` for robust physics simulation.
