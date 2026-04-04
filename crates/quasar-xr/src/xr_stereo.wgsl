// XR Stereo Rendering Shader

struct ViewUniform {
    view: mat4x4<f32>,
    projection: mat4x4<f32>,
    view_position: vec4<f32>,
    fov: vec4<f32>,
    _pad: vec4<f32>,
}

@group(0) @binding(0)
var<uniform> view: ViewUniform;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    // Full-screen triangle
    let uv = vec2<f32>(
        f32((vertex_index << 1u) & 2u),
        f32(vertex_index & 2u)
    );
    
    var output: VertexOutput;
    output.position = vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
    output.uv = uv;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // Simple gradient for testing
    let color = vec3<f32>(input.uv, 0.5);
    return vec4<f32>(color, 1.0);
}
