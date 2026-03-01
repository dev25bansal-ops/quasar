// Quasar Engine — Basic 3D shader with per-vertex color and simple lighting.
//
// Uniforms:
//   - view_proj: combined view × projection matrix
//   - model: model (world) matrix

struct CameraUniform {
    view_proj: mat4x4<f32>,
    model: mat4x4<f32>,
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
    @location(0) world_normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) world_position: vec3<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    let world_pos = camera.model * vec4<f32>(in.position, 1.0);
    out.clip_position = camera.view_proj * world_pos;
    out.world_position = world_pos.xyz;

    // Transform normal to world space (using the model matrix).
    // For correct normal transformation with non-uniform scale, we'd use
    // the inverse-transpose — but for now this works for uniform scale.
    out.world_normal = normalize((camera.model * vec4<f32>(in.normal, 0.0)).xyz);

    out.uv = in.uv;
    out.color = in.color;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Simple directional light from upper-right-front.
    let light_dir = normalize(vec3<f32>(0.5, 1.0, 0.7));
    let light_color = vec3<f32>(1.0, 0.98, 0.92);
    let ambient = vec3<f32>(0.15, 0.15, 0.2);

    let n = normalize(in.world_normal);
    let ndotl = max(dot(n, light_dir), 0.0);

    let diffuse = light_color * ndotl;
    let lighting = ambient + diffuse;

    let final_color = in.color.rgb * lighting;

    return vec4<f32>(final_color, in.color.a);
}
