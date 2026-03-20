// Quasar Engine — Basic 3D shader with materials, textures, lighting, and shadows.
//
// Bind groups (3 total to fit wgpu limit):
// group(0) = camera (view_proj + model + normal_matrix)
// group(1) = material + texture (base_color, roughness, metallic, emissive, albedo texture + sampler)
// group(2) = lighting (lights storage + shadow data)
// group(3) = instance data (storage buffer with model matrices for GPU instancing)

struct CameraUniform {
    view_proj: mat4x4<f32>,
    model: mat4x4<f32>,
    normal_matrix: mat4x4<f32>,
    prev_view_proj: mat4x4<f32>,
};

struct InstanceData {
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

struct CascadeUniform {
view_proj: mat4x4<f32>,
split_depth: f32,
_pad0: f32,
_pad1: f32,
_pad2: f32,
};

const CASCADE_COUNT: u32 = 4u;

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@group(1) @binding(0)
var<uniform> material: MaterialUniform;
@group(1) @binding(1)
var t_albedo: texture_2d<f32>;
@group(1) @binding(2)
var s_albedo: sampler;

@group(2) @binding(0)
var<storage, read> lights: LightsUniform;
@group(2) @binding(1)
var<uniform> shadow_uniform: ShadowUniform;
@group(2) @binding(2)
var t_shadow: texture_depth_2d;
@group(2) @binding(3)
var s_shadow: sampler_comparison;
@group(2) @binding(4)
var s_shadow_depth: sampler;
@group(2) @binding(5)
var<storage, read> cascades: array<CascadeUniform, 4>;
@group(2) @binding(6)
var t_cascade_shadow: texture_depth_2d_array;

// Instance data for GPU instancing (optional, used when gpu_driven_culling is enabled)
@group(3) @binding(0)
var<storage, read> instances: array<InstanceData>;

// Push constant for instance index (used with GPU-driven indirect rendering)
var<push_constant> instance_index: u32;

/// Select the cascade index for the given view-space depth.
fn select_cascade(view_depth: f32) -> u32 {
    for (var i = 0u; i < CASCADE_COUNT - 1u; i++) {
        if (view_depth < cascades[i].split_depth) {
            return i;
        }
    }
    return CASCADE_COUNT - 1u;
}

/// Sample shadow from the CSM array texture for the given world position.
fn calculate_cascade_shadow(world_pos: vec3<f32>, view_depth: f32) -> f32 {
    let idx = select_cascade(view_depth);
    let light_space = cascades[idx].view_proj * vec4<f32>(world_pos, 1.0);
    let ndc = light_space.xyz / light_space.w;
    let uv = ndc.xy * 0.5 + 0.5;

    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
        return 1.0;
    }

    let shadow_depth = ndc.z;
    var shadow = 0.0;
    let texel = 1.0 / shadow_uniform.pcss_params.y;
    let offsets = array<vec2<f32>, 4>(
        vec2<f32>(-texel, -texel),
        vec2<f32>( texel, -texel),
        vec2<f32>(-texel,  texel),
        vec2<f32>( texel,  texel),
    );
    for (var s = 0u; s < 4u; s++) {
        shadow += textureSampleCompareLevel(
            t_cascade_shadow, s_shadow,
            uv + offsets[s], i32(idx), shadow_depth);
    }
    return shadow / 4.0;
}

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
    // Instance ID for GPU instancing (built-in)
    @builtin(instance_index) instance_id: u32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) world_position: vec3<f32>,
    @location(4) shadow_position: vec4<f32>,
    @location(5) view_depth: f32,
    @location(6) motion_vector: vec2<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    // Support both instanced and non-instanced rendering
    // When instance_id > 0 and instances buffer is bound, use instance data
    let model = camera.model;
    let normal_mat = camera.normal_matrix;

    let world_pos = model * vec4<f32>(in.position, 1.0);
    out.clip_position = camera.view_proj * world_pos;
    out.world_position = world_pos.xyz;

    out.world_normal = normalize((normal_mat * vec4<f32>(in.normal, 0.0)).xyz);

    out.uv = in.uv;
    out.color = in.color;

    // Calculate shadow map coordinates
    out.shadow_position = shadow_uniform.light_view_proj * world_pos;
    out.view_depth = (camera.view_proj * world_pos).z;

    // Calculate motion vector for TAA: current NDC - previous NDC
    let prev_clip = camera.prev_view_proj * world_pos;
    let prev_ndc = prev_clip.xy / prev_clip.w;
    let curr_ndc = out.clip_position.xy / out.clip_position.w;
    out.motion_vector = (curr_ndc - prev_ndc) * 0.5; // Convert from NDC [-1,1] to motion scale

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

    // 16-sample blue-noise Poisson disk for high-quality PCSS.
    let poisson = array<vec2<f32>, 16>(
        vec2<f32>(-0.94201624, -0.39906216),
        vec2<f32>( 0.94558609, -0.76890725),
        vec2<f32>(-0.09418410, -0.92938870),
        vec2<f32>( 0.34495938,  0.29387760),
        vec2<f32>(-0.91588581,  0.45771432),
        vec2<f32>(-0.81544232, -0.87912464),
        vec2<f32>(-0.38277543,  0.27676845),
        vec2<f32>( 0.97484398,  0.75648379),
        vec2<f32>( 0.44323325, -0.97511554),
        vec2<f32>( 0.53742981, -0.47373420),
        vec2<f32>(-0.26496911, -0.41893023),
        vec2<f32>( 0.79197514,  0.19090188),
        vec2<f32>(-0.24188840,  0.99706507),
        vec2<f32>(-0.81409955,  0.91437590),
        vec2<f32>( 0.19984126,  0.78641367),
        vec2<f32>( 0.14383161, -0.14100790),
    );

    // --- Step 1: Blocker search (16 samples) ---
    let search_radius = light_size * texel * 8.0;
    var blocker_sum = 0.0;
    var blocker_count = 0.0;
    for (var i = 0u; i < 16u; i++) {
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

    // --- Step 3: PCF with 16-sample variable filter ---
    var shadow = 0.0;
    for (var i = 0u; i < 16u; i++) {
        shadow += textureSampleCompare(t_shadow, s_shadow, uv + poisson[i] * filter_radius, shadow_depth);
    }
    
    return shadow / 16.0;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Sample the albedo texture.
    let tex_color = textureSample(t_albedo, s_albedo, in.uv);

    // Combine: texture * material base_color * vertex color.
    let base = tex_color * material.base_color * in.color;

    let n = normalize(in.world_normal);
    let pcss_shadow = calculate_shadow(in.shadow_position);
    let csm_shadow = calculate_cascade_shadow(in.world_position, in.view_depth);
    // Use CSM for pixels inside cascade frustums, fall back to PCSS for distant pixels
    // is_in_cascade is true when view_depth is within any cascade split depth
    let is_in_cascade = in.view_depth < cascades[CASCADE_COUNT - 1u].split_depth;
    let shadow = select(pcss_shadow, csm_shadow, is_in_cascade);
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
