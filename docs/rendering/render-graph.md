# Render Graph

The render graph is Quasar's declarative system for managing rendering passes and GPU resources. It automatically handles resource dependencies, lifetime management, and pass execution order.

## Overview

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│ ShadowPass  │────▶│ OpaquePass  │────▶│ PostProcess │
└─────────────┘     └─────────────┘     └─────────────┘
       │                   │                   │
       ▼                   ▼                   ▼
  ShadowAtlas          HDR Color           LDR Color
```

## Core Concepts

### RenderPass

A render pass performs GPU work:

```rust,ignore
pub trait RenderPass: Send + Sync {
    fn name(&self) -> &str;
    fn execute(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        context: &RenderContext,
    );
}
```

### Attachment

An attachment is a GPU texture used as input/output:

```rust,ignore
pub struct Attachment {
    pub name: String,
    pub format: wgpu::TextureFormat,
    pub size: (u32, u32),
    pub texture: Option<wgpu::Texture>,
    pub view: Option<wgpu::TextureView>,
}
```

### PassNode

A node in the graph representing a pass:

```rust,ignore
pub struct PassNode {
    pub pass: Box<dyn RenderPass>,
    pub dependencies: Vec<PassId>,
    pub inputs: Vec<AttachmentId>,
    pub outputs: Vec<AttachmentId>,
}
```

## Creating a Render Graph

### Basic Setup

```rust,ignore
let mut graph = RenderGraph::new();

// Add attachments
let hdr = AttachmentId(0);
graph.add_attachment(hdr, Attachment {
    name: "HDR Color".into(),
    format: wgpu::TextureFormat::Rgba16Float,
    size: (1920, 1080),
    texture: None,
    view: None,
});

let depth = AttachmentId(1);
graph.add_attachment(depth, Attachment {
    name: "Depth".into(),
    format: wgpu::TextureFormat::Depth32Float,
    size: (1920, 1080),
    texture: None,
    view: None,
});
```

### Adding Passes

```rust,ignore
// Add shadow pass
let shadow_pass = PassId(0);
graph.add_pass(shadow_pass, Box::new(ShadowPass::new(2048)));
graph.add_output(shadow_pass, shadow_atlas);

// Add opaque pass
let opaque_pass = PassId(1);
graph.add_pass(opaque_pass, Box::new(OpaquePass::new()));
graph.add_input(opaque_pass, shadow_atlas);
graph.add_output(opaque_pass, hdr);
graph.add_output(opaque_pass, depth);
graph.add_dependency(opaque_pass, shadow_pass);

// Add post-process pass
let post_pass = PassId(2);
graph.add_pass(post_pass, Box::new(PostProcessPass::new()));
graph.add_input(post_pass, hdr);
graph.add_output(post_pass, ldr);
graph.add_dependency(post_pass, opaque_pass);
```

## Executing the Graph

### Compile and Execute

```rust,ignore
// Create GPU resources
graph.create_resources(&device)?;

// Execute
let command_buffer = graph.execute(&device, &queue, &context)?;
queue.submit(Some(command_buffer));
```

### Resource Creation

The graph automatically creates textures with aliasing:

```rust,ignore
// Non-overlapping passes share textures
// Pass A writes to texture X
// Pass B reads from X (after A completes)
// Pass C writes to texture X (reused!)
```

## Standard Passes

### ShadowPass

Renders depth from light views:

```rust,ignore
pub struct ShadowPass {
    pub shadow_atlas_size: u32,
}

impl RenderPass for ShadowPass {
    fn name(&self) -> &str { "ShadowPass" }

    fn execute(&self, device: &wgpu::Device, queue: &wgpu::Queue,
               encoder: &mut wgpu::CommandEncoder, ctx: &RenderContext) {
        // Render shadow cascades
    }
}
```

### GBufferPass

Deferred rendering geometry pass:

```rust,ignore
pub struct GBufferPass;

impl RenderPass for GBufferPass {
    fn name(&self) -> &str { "GBufferPass" }

    fn execute(&self, device: &wgpu::Device, queue: &wgpu::Queue,
               encoder: &mut wgpu::CommandEncoder, ctx: &RenderContext) {
        // Render to G-Buffer (albedo, normal, depth)
    }
}
```

### DeferredLightPass

Lighting calculation:

```rust,ignore
pub struct DeferredLightPass;

impl RenderPass for DeferredLightPass {
    fn name(&self) -> &str { "DeferredLightPass" }

    fn execute(&self, device: &wgpu::Device, queue: &wgpu::Queue,
               encoder: &mut wgpu::CommandEncoder, ctx: &RenderContext) {
        // Accumulate lighting from G-Buffer
    }
}
```

### PostProcessPass

Post-processing effects:

```rust,ignore
pub struct PostProcessPass {
    pub enable_taa: bool,
    pub enable_bloom: bool,
    pub enable_fxaa: bool,
}

impl RenderPass for PostProcessPass {
    fn name(&self) -> &str { "PostProcessPass" }

    fn execute(&self, device: &wgpu::Device, queue: &wgpu::Queue,
               encoder: &mut wgpu::CommandEncoder, ctx: &RenderContext) {
        // TAA -> Bloom -> Tonemap -> FXAA
    }
}
```

## Resource Management

### Automatic Aliasing

The graph reuses textures between non-overlapping passes:

```rust,ignore
// Pass 1: writes to Attachment A (frames 0-1)
// Pass 2: reads from Attachment A (frame 2)
// Pass 3: writes to Attachment A (frame 3+) <- Same texture memory!
```

### Resource States

```rust,ignore
pub enum ResourceState {
    Undefined,
    RenderTarget,
    ShaderRead,
    Storage,
    Present,
}
```

### Barrier Insertion

The graph automatically inserts barriers:

```rust,ignore
// Compute transitions between passes
let transitions = graph.compute_transitions(&order);

for transition in transitions {
    // Insert pipeline barrier
}
```

## Dependency Management

### Explicit Dependencies

```rust,ignore
graph.add_dependency(post_process_pass, opaque_pass);
graph.add_dependency(opaque_pass, shadow_pass);
```

### Automatic Inference

```rust,ignore
// Automatically add dependencies based on read-after-write
graph.infer_dependencies();
```

### Cycle Detection

```rust,ignore
match graph.compile() {
    Ok(order) => {
        // Execute passes
    }
    Err(RenderGraphError::CycleDetected(passes)) => {
        log::error!("Render graph has cycle: {:?}", passes);
    }
}
```

## RenderContext

Context passed to each pass:

```rust,ignore
pub struct RenderContext {
    pub screen_size: (u32, u32),
    pub hdr_texture: Option<wgpu::TextureView>,
    pub depth_view: wgpu::TextureView,
    pub camera_bind_group: wgpu::BindGroup,
    pub light_bind_group: wgpu::BindGroup,
    pub resources: HashMap<String, Arc<dyn Any + Send + Sync>>,
}
```

## Advanced Features

### Async Compute

```rust,ignore
pub enum PassQueue {
    Graphics,
    AsyncCompute,
    Transfer,
}

// Mark pass for async compute
let culling_pass = PassId(10);
graph.set_queue(culling_pass, PassQueue::AsyncCompute);
```

### Cross-Queue Synchronization

```rust,ignore
let barriers = graph.build_barrier_plan(&order, &queue_map);
for barrier in barriers {
    if barrier.cross_queue {
        // Insert cross-queue semaphore
    }
}
```

### Graph Builder

Helper for building standard graphs:

```rust,ignore
let mut builder = GraphBuilder::new(&device, &queue);

let hdr = builder.add_attachment("HDR", wgpu::TextureFormat::Rgba16Float, 1920, 1080);
let depth = builder.add_attachment("Depth", wgpu::TextureFormat::Depth32Float, 1920, 1080);

builder.add_pass(
    Box::new(OpaquePass::new()),
    vec![],           // inputs
    vec![hdr, depth], // outputs
    vec![],           // dependencies
);

let graph = builder.build();
```

## Performance Considerations

### Minimize Pass Count

Each pass has overhead:

```rust,ignore
// Bad - many small passes
for light in lights {
    graph.add_pass(PassId(i), Box::new(LightPass::new(light)));
}

// Better - batch lights
graph.add_pass(PassId(0), Box::new(ClusteredLightPass::new()));
```

### Reuse Attachments

```rust,ignore
// Bad - separate textures
let color1 = graph.add_attachment("Color1", ...);
let color2 = graph.add_attachment("Color2", ...);

// Better - same texture if non-overlapping
// Graph handles this automatically with aliasing
```

### Batch Draw Calls

```rust,ignore
pub struct OpaquePassData {
    pub objects: Vec<OpaqueDraw>,  // Batch all opaque objects
    pub pipeline: Arc<wgpu::RenderPipeline>,
}
```

## Debugging

### Graph Visualization

```rust,ignore
// Print graph structure
for (id, node) in &graph.nodes {
    println!("Pass: {} (deps: {:?})", node.pass.name(), node.dependencies);
}

// Print execution order
let order = graph.compile()?;
println!("Execution order: {:?}", order);
```

### Resource Tracking

```rust,ignore
let lifetimes = graph.compute_attachment_lifetimes(&order);
for (att_id, (first, last)) in &lifetimes {
    println!("Attachment {:?}: used in passes {}-{}", att_id, first, last);
}
```

## Common Issues

### Missing Dependency Error

```
Error: pass "PostProcess" reads attachment "HDR" written by "Opaque" but has no dependency on it
```

Solution: Add explicit dependency:

```rust,ignore
graph.add_dependency(post_process_pass, opaque_pass);
```

### Missing Writer Error

```
Error: pass "Opaque" reads attachment "ShadowAtlas" but no pass writes it
```

Solution: Add a pass that writes the attachment:

```rust,ignore
graph.add_output(shadow_pass, shadow_atlas);
```

## Next Steps

- [Materials](materials.md)
- [Shaders](shaders.md)
- [Architecture](../architecture.md)
