# quasar-render

High-performance rendering pipeline for the Quasar Engine.

## Features

- **PBR Rendering**: Cook-Torrance BRDF
- **Clustered Lighting**: Efficient many-light rendering
- **Shadow Mapping**: CSM and VSM support
- **Post-Processing**: SSAO, SSR, TAA, Bloom, FXAA
- **GPU Culling**: Hi-Z occlusion culling
- **Meshlets**: GPU-driven meshlet rendering
- **Terrain**: Heightmap-based terrain
- **Particles**: GPU particle systems
- **Volumetric Fog**: Ray-marched volumetrics
- **Debug Wireframe**: Physics debug visualization

## Feature Flags

- `deferred` - Deferred rendering pipeline
- `clustered-lighting` - Clustered shading
- `gpu-culling` - GPU-driven culling
- `meshlet` - Meshlet pipeline
- `ssr` - Screen-space reflections
- `terrain` - Terrain system
- `particles` - Particle system
- `volumetric` - Volumetric fog
- `lightmap` - Baked lightmaps
- `reflection-probes` - Reflection probes
- `decals` - Decal system

## Usage

```rust
use quasar_render::{Renderer, Camera, MeshShape};

let renderer = Renderer::new(&device, &surface);
let camera = Camera::new(1920, 1080);
```
