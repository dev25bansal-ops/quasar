// Quasar Engine — Basic 3D shader with materials, textures, lighting, and shadows.
//
// Bind groups:
// group(0) = camera (view_proj + model + normal_matrix)
// group(1) = material (base_color, roughness, metallic, emissive)
// group(2) = lights (storage buffer, multiple lights + ambient)
// group(3) = texture (albedo texture + sampler)
// group(4) = shadow (shadow map + comparison sampler)

struct CameraUniform {
    view_proj: mat4x4<f32>,
    model: mat4x4<f32>,
    normal_matrix: mat4x4<f32>,
};

struct LightData {
    position: vec4<f32>,
    color: vec4<f32>,
    direction: vec4<f32>,
    params: vec4<f32>,
};

struct LightsUniform {
    lights: array<LightData, 256>,
    count: u32,
    _pad: vec3<u32>,
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
    // x = light_size (world-space), y = shadow_map_size (texels), z/w = unused
    pcss_params: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@group(1) @binding(0)
var<uniform> material: MaterialUniform;

@group(2) @binding(0)
var<storage, read> lights: LightsUniform;

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
@group(4) @binding(3)
var s_shadow_depth: sampler;

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
    
    let shadow_depth = ndc.z;
    let light_size = shadow_uniform.pcss_params.x;
    let map_size = shadow_uniform.pcss_params.y;
    let texel = 1.0 / map_size;

    // 4-sample Poisson disk for blocker search and PCF.
    let poisson = array<vec2<f32>, 4>(
        vec2<f32>(-0.94201624, -0.39906216),
        vec2<f32>( 0.94558609, -0.76890725),
        vec2<f32>(-0.09418410, -0.92938870),
        vec2<f32>( 0.34495938,  0.29387760),
    );

    // --- Step 1: Blocker search ---
    let search_radius = light_size * texel * 8.0;
    var blocker_sum = 0.0;
    var blocker_count = 0.0;
    for (var i = 0u; i < 4u; i++) {
        let sample_uv = uv + poisson[i] * search_radius;
        let d = textureSampleLevel(t_shadow, s_shadow_depth, sample_uv, 0.0);
        if (d < shadow_depth) {
            blocker_sum += d;
            blocker_count += 1.0;
        }
    }

    // No blockers → fully lit.
    if (blocker_count < 0.5) {
        return 1.0;
    }

    // --- Step 2: Penumbra estimation ---
    let avg_blocker = blocker_sum / blocker_count;
    let penumbra = light_size * (shadow_depth - avg_blocker) / avg_blocker;
    let filter_radius = max(penumbra * texel * 4.0, texel);

    // --- Step 3: PCF with variable filter ---
    var shadow = 0.0;
    shadow += textureSampleCompare(t_shadow, s_shadow, uv + poisson[0] * filter_radius, shadow_depth);
    shadow += textureSampleCompare(t_shadow, s_shadow, uv + poisson[1] * filter_radius, shadow_depth);
    shadow += textureSampleCompare(t_shadow, s_shadow, uv + poisson[2] * filter_radius, shadow_depth);
    shadow += textureSampleCompare(t_shadow, s_shadow, uv + poisson[3] * filter_radius, shadow_depth);
    
    return shadow / 4.0;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Sample the albedo texture.
    let tex_color = textureSample(t_albedo, s_albedo, in.uv);

    // Combine: texture * material base_color * vertex color.
    let base = tex_color * material.base_color * in.color;

    let n = normalize(in.world_normal);
    let shadow = calculate_shadow(in.shadow_position);
    let ambient = lights.ambient.rgb * lights.ambient.a;

    var diffuse_total = vec3<f32>(0.0);
    for (var i = 0u; i < lights.count; i++) {
        let light = lights.lights[i];
        let light_type = u32(light.params.x);
        var light_dir: vec3<f32>;
        var attenuation = 1.0;

        if (light_type == 0u) {
            // Directional
            light_dir = normalize(-light.direction.xyz);
        } else {
            // Point / Spot
            let to_light = light.position.xyz - in.world_position;
            let dist = length(to_light);
            light_dir = to_light / dist;
            let range = light.params.y;
            if (range > 0.0) {
                attenuation = saturate(1.0 - dist / range);
                attenuation = attenuation * attenuation;
            }
        }

        let ndotl = max(dot(n, light_dir), 0.0);
        diffuse_total += light.color.rgb * light.color.a * ndotl * attenuation * shadow;
    }

    let lighting = ambient + diffuse_total;

    // Apply lighting to the base color.
    var final_color = base.rgb * lighting;

    // Add emissive contribution.
    final_color = final_color + base.rgb * material.emissive;

    return vec4<f32>(final_color, base.a);
}
