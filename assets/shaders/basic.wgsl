// Quasar Engine — Basic 3D shader with materials, textures, and lighting.
//
// Bind groups:
//   group(0) = camera  (view_proj + model)
//   group(1) = material (base_color, roughness, metallic, emissive)
//   group(2) = texture  (albedo texture + sampler)

struct CameraUniform {
    view_proj: mat4x4<f32>,
    model: mat4x4<f32>,
};

struct MaterialUniform {
    base_color: vec4<f32>,
    // x = roughness, y = metallic
    roughness_metallic: vec2<f32>,
    emissive: f32,
    _pad: f32,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@group(1) @binding(0)
var<uniform> material: MaterialUniform;

@group(2) @binding(0)
var t_albedo: texture_2d<f32>;
@group(2) @binding(1)
var s_albedo: sampler;

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
    // Sample the albedo texture.
    let tex_color = textureSample(t_albedo, s_albedo, in.uv);

    // Combine: texture * material base_color * vertex color.
    let base = tex_color * material.base_color * in.color;

    // Simple directional light from upper-right-front.
    let light_dir = normalize(vec3<f32>(0.5, 1.0, 0.7));
    let light_color = vec3<f32>(1.0, 0.98, 0.92);
    let ambient = vec3<f32>(0.15, 0.15, 0.2);

    let n = normalize(in.world_normal);
    let ndotl = max(dot(n, light_dir), 0.0);

    let diffuse = light_color * ndotl;
    let lighting = ambient + diffuse;

    // Apply lighting to the base color.
    var final_color = base.rgb * lighting;

    // Add emissive contribution.
    final_color = final_color + base.rgb * material.emissive;

    return vec4<f32>(final_color, base.a);
}
