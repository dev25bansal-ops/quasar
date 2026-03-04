// Quasar Engine — Basic 3D shader with materials, textures, lighting, and shadows.
//
// Bind groups:
// group(0) = camera (view_proj + model + normal_matrix)
// group(1) = material (base_color, roughness, metallic, emissive)
// group(2) = lights (directional + ambient)
// group(3) = texture (albedo texture + sampler)
// group(4) = shadow (shadow map + comparison sampler)

struct CameraUniform {
    view_proj: mat4x4<f32>,
    model: mat4x4<f32>,
    normal_matrix: mat4x4<f32>,
};

struct LightUniform {
    direction: vec4<f32>,
    color: vec4<f32>,
    ambient: vec4<f32>,
};

struct MaterialUniform {
    base_color: vec4<f32>,
    roughness_metallic: vec2<f32>,
    emissive: f32,
    _pad: f32,
};

struct ShadowUniform {
    light_view_proj: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@group(1) @binding(0)
var<uniform> material: MaterialUniform;

@group(2) @binding(0)
var<uniform> lights: LightUniform;

@group(3) @binding(0)
var t_albedo: texture_2d<f32>;
@group(3) @binding(1)
var s_albedo: sampler;

@group(4) @binding(0)
var<uniform> shadow_uniform: ShadowUniform;
@group(4) @binding(1)
var t_shadow: texture_depth_2d;
@group(4) @binding(2)
var s_shadow: sampler_comparison;

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
    @location(4) shadow_position: vec4<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    let world_pos = camera.model * vec4<f32>(in.position, 1.0);
    out.clip_position = camera.view_proj * world_pos;
    out.world_position = world_pos.xyz;

    out.world_normal = normalize((camera.normal_matrix * vec4<f32>(in.normal, 0.0)).xyz);

    out.uv = in.uv;
    out.color = in.color;

    // Calculate shadow map coordinates
    out.shadow_position = shadow_uniform.light_view_proj * world_pos;

    return out;
}

fn calculate_shadow(shadow_pos: vec4<f32>) -> f32 {
    // Convert to NDC and then to texture coordinates
    let ndc = shadow_pos.xyz / shadow_pos.w;
    let uv = ndc.xy * 0.5 + 0.5;
    
    // Check if outside shadow map bounds
    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
        return 1.0;
    }
    
    // PCF (Percentage Closer Filtering) with 2x2 samples
    let shadow_depth = ndc.z;
    var shadow = 0.0;
    let offset = 1.0 / 1024.0; // Assumes 1024x1024 shadow map
    
    shadow += textureSampleCompare(t_shadow, s_shadow, uv + vec2<f32>(-offset, -offset), shadow_depth);
    shadow += textureSampleCompare(t_shadow, s_shadow, uv + vec2<f32>( offset, -offset), shadow_depth);
    shadow += textureSampleCompare(t_shadow, s_shadow, uv + vec2<f32>(-offset,  offset), shadow_depth);
    shadow += textureSampleCompare(t_shadow, s_shadow, uv + vec2<f32>( offset,  offset), shadow_depth);
    
    return shadow / 4.0;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Sample the albedo texture.
    let tex_color = textureSample(t_albedo, s_albedo, in.uv);

    // Combine: texture * material base_color * vertex color.
    let base = tex_color * material.base_color * in.color;

    // Directional light from uniform.
    let light_dir = normalize(lights.direction.xyz);
    let light_color = lights.color.rgb;
    let ambient = lights.ambient.rgb;

    let n = normalize(in.world_normal);
    let ndotl = max(dot(n, light_dir), 0.0);

    // Calculate shadow
    let shadow = calculate_shadow(in.shadow_position);

    // Apply shadow to diffuse
    let diffuse = light_color * ndotl * shadow;
    let lighting = ambient + diffuse;

    // Apply lighting to the base color.
    var final_color = base.rgb * lighting;

    // Add emissive contribution.
    final_color = final_color + base.rgb * material.emissive;

    return vec4<f32>(final_color, base.a);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Sample the albedo texture.
    let tex_color = textureSample(t_albedo, s_albedo, in.uv);

    // Combine: texture * material base_color * vertex color.
    let base = tex_color * material.base_color * in.color;

    // Directional light from uniform.
    let light_dir = normalize(lights.direction.xyz);
    let light_color = lights.color.rgb;
    let ambient = lights.ambient.rgb;

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
