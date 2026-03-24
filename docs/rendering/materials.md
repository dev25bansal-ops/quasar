# Materials

Materials define how surfaces are rendered in Quasar. They combine shaders, textures, and parameters to create visual effects.

## Overview

```
Material
├── Shader (WGSL)
├── Parameters
│   ├── Base Color
│   ├── Metallic
│   ├── Roughness
│   └── Emissive
└── Textures
    ├── Albedo Map
    ├── Normal Map
    ├── Metallic/Roughness Map
    └── Emissive Map
```

## PBR Material

Quasar uses Physically Based Rendering (PBR) for realistic materials:

```rust,ignore
pub struct PbrMaterial {
    pub base_color: Vec4,
    pub base_color_texture: Option<AssetHandle<Texture>>,
    pub metallic: f32,
    pub roughness: f32,
    pub metallic_roughness_texture: Option<AssetHandle<Texture>>,
    pub normal_texture: Option<AssetHandle<Texture>>,
    pub occlusion_texture: Option<AssetHandle<Texture>>,
    pub emissive: Vec3,
    pub emissive_texture: Option<AssetHandle<Texture>>,
    pub alpha_mode: AlphaMode,
    pub double_sided: bool,
}

pub enum AlphaMode {
    Opaque,
    Mask(f32),
    Blend,
}
```

## Creating Materials

### Programmatically

```rust,ignore
let material = PbrMaterial {
    base_color: Vec4::new(1.0, 0.0, 0.0, 1.0),  // Red
    metallic: 0.8,
    roughness: 0.2,
    ..Default::default()
};

let handle = asset_server.add_material(material);
```

### From Assets

```rust,ignore
// Load material from file
let material: PbrMaterial = asset_server.load("materials/metal.mat")?;

// Assign to mesh
world.insert(entity, MeshRenderer {
    mesh: mesh_handle,
    material: handle,
});
```

## Textures

### Texture Types

| Type               | Format   | Usage             |
| ------------------ | -------- | ----------------- |
| Albedo             | RGBA8    | Base color        |
| Normal             | RG/RGBA8 | Surface detail    |
| Metallic/Roughness | RG8      | PBR parameters    |
| Occlusion          | R8       | Ambient occlusion |
| Emissive           | RGBA8    | Self-illumination |
| Height             | R8       | Parallax mapping  |

### Loading Textures

```rust,ignore
// Load from file
let albedo = asset_server.load::<Texture>("textures/brick_albedo.png")?;
let normal = asset_server.load::<Texture>("textures/brick_normal.png")?;

let material = PbrMaterial {
    base_color_texture: Some(albedo),
    normal_texture: Some(normal),
    ..Default::default()
};
```

## Material Parameters

### Base Color

```rust,ignore
// Solid color
material.base_color = Vec4::new(1.0, 0.5, 0.0, 1.0);  // Orange

// With texture
material.base_color_texture = Some(texture_handle);
material.base_color = Vec4::new(1.0, 1.0, 1.0, 1.0);  // White (texture colors)
```

### Metallic / Roughness

```rust,ignore
// Metal: high metallic, low roughness = shiny reflections
material.metallic = 1.0;
material.roughness = 0.1;

// Plastic: low metallic, high roughness = matte
material.metallic = 0.0;
material.roughness = 0.8;
```

### Emissive

```rust,ignore
// Self-illuminating surface
material.emissive = Vec3::new(1.0, 0.5, 0.0);  // Orange glow
material.emissive_texture = Some(emissive_map);
```

### Alpha Modes

```rust,ignore
// Opaque (default)
material.alpha_mode = AlphaMode::Opaque;

// Alpha masking (cutout)
material.alpha_mode = AlphaMode::Mask(0.5);  // Cutoff value

// Alpha blending (transparency)
material.alpha_mode = AlphaMode::Blend;
```

## Custom Materials

### Shader Structure

```wgsl
// custom.wgsl

@group(0) @binding(0) var<uniform> view: ViewUniform;
@group(1) @binding(0) var<uniform> material: MaterialUniform;
@group(1) @binding(1) var base_color: texture_2d<f32>;
@group(1) @binding(2) var base_color_sampler: sampler;

struct MaterialUniform {
    base_color: vec4<f32>,
    custom_param: f32,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) normal: vec3<f32>,
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    // Vertex transformation
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // Custom shading
    let color = textureSample(base_color, base_color_sampler, input.uv);
    return color * material.base_color;
}
```

### Custom Material Type

```rust,ignore
pub struct CustomMaterial {
    pub shader: ShaderHandle,
    pub base_color: Vec4,
    pub base_color_texture: Option<AssetHandle<Texture>>,
    pub custom_param: f32,
}

impl Material for CustomMaterial {
    fn shader(&self) -> &ShaderHandle {
        &self.shader
    }

    fn bind_group(&self, device: &Device) -> wgpu::BindGroup {
        // Create bind group with material parameters
    }
}
```

## Material Sorting

For optimal rendering, materials are sorted:

1. **Opaque materials** (front to back) - Reduces overdraw
2. **Alpha masked materials** (front to back)
3. **Transparent materials** (back to front) - Correct blending

```rust,ignore
pub fn sort_materials(renderables: &mut [Renderable]) {
    renderables.sort_by(|a, b| {
        match (a.material.alpha_mode(), b.material.alpha_mode()) {
            (AlphaMode::Blend, AlphaMode::Blend) => {
                // Back to front for transparency
                b.distance.cmp(&a.distance)
            }
            _ => {
                // Front to back for opaque
                a.distance.cmp(&b.distance)
            }
        }
    });
}
```

## Material Instances

Share materials with different parameters:

```rust,ignore
// Base material
let base_material = asset_server.load::<PbrMaterial>("materials/car.mat")?;

// Create instances with different colors
let red_car = MaterialInstance::new(base_material.clone())
    .with("base_color", Vec4::new(1.0, 0.0, 0.0, 1.0));

let blue_car = MaterialInstance::new(base_material.clone())
    .with("base_color", Vec4::new(0.0, 0.0, 1.0, 1.0));
```

## Material Hot-Reload

Materials can be reloaded at runtime:

```rust,ignore
// Watch for changes
let mut watcher = MaterialWatcher::new();

// In update loop
for event in watcher.poll_events() {
    match event {
        MaterialEvent::Modified(handle) => {
            log::info!("Material {:?} modified, reloading", handle);
            asset_server.reload(handle);
        }
    }
}
```

## Performance Tips

### 1. Batch Similar Materials

```rust,ignore
// Bad: Different material per object
for object in objects {
    object.material = unique_material;
}

// Better: Share materials
let shared_material = create_material();
for object in objects {
    object.material = shared_material.clone();
}
```

### 2. Use Texture Atlases

```rust,ignore
// Combine multiple textures into one
let atlas = TextureAtlas::new(4096, 4096);
atlas.add("grass.png", 0, 0);
atlas.add("dirt.png", 512, 0);
atlas.add("stone.png", 1024, 0);
```

### 3. LOD Materials

```rust,ignore
// Simpler materials for distant objects
if distance > 100.0 {
    material.normal_texture = None;
    material.occlusion_texture = None;
}
```

## Common Materials

### Metal

```rust,ignore
PbrMaterial {
    base_color: Vec4::new(0.95, 0.95, 0.95, 1.0),
    metallic: 1.0,
    roughness: 0.3,
}
```

### Plastic

```rust,ignore
PbrMaterial {
    base_color: Vec4::new(0.8, 0.2, 0.2, 1.0),
    metallic: 0.0,
    roughness: 0.5,
}
```

### Glass

```rust,ignore
PbrMaterial {
    base_color: Vec4::new(1.0, 1.0, 1.0, 0.3),
    metallic: 0.0,
    roughness: 0.0,
    alpha_mode: AlphaMode::Blend,
}
```

### Emissive (Light Source)

```rust,ignore
PbrMaterial {
    emissive: Vec3::new(1.0, 0.9, 0.8) * 10.0,
    base_color: Vec4::new(1.0, 1.0, 1.0, 1.0),
}
```

## Next Steps

- [Render Graph](render-graph.md)
- [Shaders](shaders.md)
