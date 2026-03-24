# Custom Render Pass Example

This guide demonstrates how to create custom render passes for the Quasar render graph.

## Overview

A render pass is a unit of GPU work that:

1. Reads from input attachments (textures)
2. Performs rendering/compute operations
3. Writes to output attachments

## Creating a Custom Pass

### Basic Structure

```rust,ignore
use quasar_render::render_graph::{RenderPass, RenderContext};
use wgpu::{Device, Queue, CommandEncoder};

pub struct MyCustomPass {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
}

impl MyCustomPass {
    pub fn new(device: &Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("my_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("my_shader.wgsl").into()),
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("my_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba16Float,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self { pipeline, bind_group }
    }
}

impl RenderPass for MyCustomPass {
    fn name(&self) -> &str {
        "my_custom_pass"
    }

    fn execute(
        &self,
        device: &Device,
        queue: &Queue,
        encoder: &mut CommandEncoder,
        context: &RenderContext,
    ) {
        let output = context.hdr_texture.as_ref().unwrap();

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("My Custom Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: output,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.draw(0..3, 0..1);  // Full-screen triangle
    }
}
```

## Adding to Render Graph

### Register the Pass

```rust,ignore
use quasar_render::render_graph::{RenderGraph, PassId, AttachmentId, Attachment};

fn setup_render_graph(device: &Device, queue: &Queue) -> RenderGraph {
    let mut graph = RenderGraph::new();

    // Create attachments
    let hdr = AttachmentId(0);
    graph.add_attachment(hdr, Attachment {
        name: "HDR Color".into(),
        format: wgpu::TextureFormat::Rgba16Float,
        size: (1920, 1080),
        texture: None,
        view: None,
    });

    // Add custom pass
    let custom_pass = PassId(10);
    graph.add_pass(custom_pass, Box::new(MyCustomPass::new(device)));
    graph.add_input(custom_pass, hdr);
    graph.add_output(custom_pass, hdr);

    graph
}
```

## Common Pass Types

### Full-Screen Quad Pass

For post-processing effects:

```rust,ignore
pub struct FullScreenPass {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
}

impl FullScreenPass {
    pub fn new(device: &Device, shader_source: &str) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fullscreen_shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        // ... create pipeline

        Self { pipeline, bind_group }
    }
}

impl RenderPass for FullScreenPass {
    fn name(&self) -> &str { "fullscreen" }

    fn execute(&self, device: &Device, queue: &Queue, encoder: &mut CommandEncoder, ctx: &RenderContext) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Fullscreen Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: ctx.hdr_texture.as_ref().unwrap(),
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.draw(0..3, 0..1);  // Triangle covers screen
    }
}
```

### Compute Pass

For GPU compute:

```rust,ignore
pub struct ComputePass {
    pipeline: wgpu::ComputePipeline,
    bind_group: wgpu::BindGroup,
}

impl ComputePass {
    pub fn new(device: &Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("compute_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("compute.wgsl").into()),
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("compute_pipeline"),
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        Self { pipeline, bind_group }
    }
}

impl RenderPass for ComputePass {
    fn name(&self) -> &str { "compute" }

    fn execute(&self, device: &Device, queue: &Queue, encoder: &mut CommandEncoder, ctx: &RenderContext) {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Compute Pass"),
            timestamp_writes: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups(16, 16, 1);
    }
}
```

### Geometry Pass

For rendering 3D geometry:

```rust,ignore
pub struct GeometryPass {
    pipeline: wgpu::RenderPipeline,
    objects: Vec<RenderObject>,
}

struct RenderObject {
    mesh: Arc<Mesh>,
    transform: Mat4,
    material_bind_group: wgpu::BindGroup,
}

impl RenderPass for GeometryPass {
    fn name(&self) -> &str { "geometry" }

    fn execute(&self, device: &Device, queue: &Queue, encoder: &mut CommandEncoder, ctx: &RenderContext) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Geometry Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: ctx.hdr_texture.as_ref().unwrap(),
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &ctx.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(&self.pipeline);

        for object in &self.objects {
            pass.set_bind_group(0, &object.material_bind_group, &[]);
            pass.set_vertex_buffer(0, object.mesh.vertex_buffer.slice(..));
            pass.set_index_buffer(object.mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..object.mesh.index_count, 0, 0..1);
        }
    }
}
```

## Post-Processing Example

### Bloom Effect

```rust,ignore
pub struct BloomPass {
    bright_pass_pipeline: wgpu::RenderPipeline,
    blur_pipeline: wgpu::RenderPipeline,
    combine_pipeline: wgpu::RenderPipeline,
    bright_texture: wgpu::Texture,
    blur_textures: [wgpu::Texture; 2],
}

impl BloomPass {
    pub fn new(device: &Device, size: (u32, u32)) -> Self {
        // Create bright-pass texture (quarter resolution)
        let bright_texture = create_texture(device, size.0 / 4, size.1 / 4, "Bright");

        // Create blur ping-pong textures
        let blur_textures = [
            create_texture(device, size.0 / 4, size.1 / 4, "Blur0"),
            create_texture(device, size.0 / 4, size.1 / 4, "Blur1"),
        ];

        // ... create pipelines

        Self { bright_pass_pipeline, blur_pipeline, combine_pipeline, bright_texture, blur_textures }
    }
}

impl RenderPass for BloomPass {
    fn name(&self) -> &str { "bloom" }

    fn execute(&self, device: &Device, queue: &Queue, encoder: &mut CommandEncoder, ctx: &RenderContext) {
        // Step 1: Extract bright pixels
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.bright_texture.default_view,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                    resolve_target: None,
                })],
                ..Default::default()
            });
            pass.set_pipeline(&self.bright_pass_pipeline);
            pass.draw(0..3, 0..1);
        }

        // Step 2: Horizontal blur
        // Step 3: Vertical blur
        // Step 4: Combine with original
    }
}
```

## Debugging

### Debug Overlays

```rust,ignore
pub struct DebugRenderPass {
    debug_lines: Vec<(Vec3, Vec3, Color)>,
}

impl DebugRenderPass {
    pub fn draw_line(&mut self, from: Vec3, to: Vec3, color: Color) {
        self.debug_lines.push((from, to, color));
    }

    pub fn draw_sphere(&mut self, center: Vec3, radius: f32, color: Color) {
        // Generate line segments for sphere
    }
}

impl RenderPass for DebugRenderPass {
    fn name(&self) -> &str { "debug" }

    fn execute(&self, device: &Device, queue: &Queue, encoder: &mut CommandEncoder, ctx: &RenderContext) {
        // Render debug geometry
    }
}
```

## Performance Tips

### 1. Batch Draw Calls

```rust,ignore
// Bad - separate draw calls
for object in objects {
    pass.draw(object);
}

// Better - instanced rendering
pass.draw_instanced(0..vertex_count, 0..instance_count);
```

### 2. Minimize Pass Count

```rust,ignore
// Bad - multiple passes
graph.add_pass(PassId(0), Box::new(BlurX));
graph.add_pass(PassId(1), Box::new(BlurY));

// Better - single combined pass
graph.add_pass(PassId(0), Box::new(BlurXY));
```

### 3. Use Compute for Image Processing

```rust,ignore
// Compute is often faster for image processing
pub struct BlurComputePass {
    pipeline: wgpu::ComputePipeline,
}

impl RenderPass for BlurComputePass {
    fn execute(&self, device: &Device, queue: &Queue, encoder: &mut CommandEncoder, ctx: &RenderContext) {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
        pass.set_pipeline(&self.pipeline);
        pass.dispatch_workgroups(width / 8, height / 8, 1);
    }
}
```

## Next Steps

- [Render Graph](../rendering/render-graph.md)
- [Materials](../rendering/materials.md)
