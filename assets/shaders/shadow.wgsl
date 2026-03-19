// Quasar Engine — Shadow mapping shader.
//
// Renders depth from light's point of view for shadow mapping.
// Uses a depth-only pass with no color output.

struct CameraUniform {
    view_proj: mat4x4<f32>,
    model: mat4x4<f32>,
    normal_matrix: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) depth: f32,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world_pos = camera.model * vec4<f32>(in.position, 1.0);
    out.clip_position = camera.view_proj * world_pos;
    out.depth = out.clip_position.z / out.clip_position.w;
    return out;
}

// No fragment shader needed - depth-only pass
// Depth is written by the rasterizer, no color output required
