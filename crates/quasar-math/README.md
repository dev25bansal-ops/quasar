# quasar-math

Math types and utilities for the Quasar Engine.

## Features

- **Transform**: Position, rotation, scale with matrix conversion
- **Color**: RGBA colors with conversions
- **Ray**: Ray types for collision detection
- **Bounds**: AABB and bounding spheres

## Types

```rust
use quasar_math::{Transform, Vec3, Quat, Color};

let transform = Transform {
    position: Vec3::new(0.0, 1.0, 0.0),
    rotation: Quat::IDENTITY,
    scale: Vec3::ONE,
};

let color = Color::rgb(1.0, 0.5, 0.0);
```

## Integration

Built on `glam` for high-performance SIMD math operations.
