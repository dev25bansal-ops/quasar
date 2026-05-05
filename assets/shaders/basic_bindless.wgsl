// Quasar Engine — Bindless Basic 3D shader with non-uniform indexing.
//
// This shader uses a single bindless bind group for ALL materials and textures,
// eliminating per-material bind group switches. Textures and materials are
// accessed via dynamic non-uniform indexing.
//
// Bind groups:
// group(0) = Camera uniform (view_proj + model + normal_matrix + prev_view_proj)
// group(1) = Bindless bind group:
//            binding 0: binding_array<texture_2d<f32>> (1024 textures)
//            binding 1: binding_array<sampler> (64 samplers)
//            binding 2: storage<read> material data buffer (4096 materials)
// group(2) = Lighting (lights storage + shadow data)

struct CameraUniform {
    view_proj: mat4x4<f32>,
    model: mat4x4<f32>,
    normal_matrix: mat4x4<f32>,
    prev_view_proj: mat4x4<f32>,
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
    _pad2: vec4<f32>,
};

struct ShadowUniform {
    light_view_proj: mat4x4<f32>,
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

// ── Bindless Material Data ──────────────────────────────────────
// Matches GpuMaterialData in bindless.rs (64 bytes / 4x vec4)
struct GpuMaterialData {
    base_color: vec4<f32>,          // 16 bytes
    roughness: f32,                 // 4 bytes
    metallic: f32,                  // 4 bytes
    emissive_strength: f32,         // 4 bytes
    albedo_tex_index: u32,          // 4 bytes
    normal_tex_index: u32,          // 4 bytes
    mr_tex_index: u32,              // 4 bytes
    sampler_index: u32,             // 4 bytes
    _pad: vec2<u32>,                // 8 bytes padding
};                                  // Total: 64 bytes

// ── Bindings ────────────────────────────────────────────────────

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

// Bindless texture array (1024 textures)
// Uses TEXTURE_BINDING_ARRAY feature
@group(1) @binding(0)
var t_albedo_array: binding_array<texture_2d<f32>, 1024>;

// Bindless sampler array (64 samplers)
// Uses SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING
@group(1) @binding(1)
var s_array: binding_array<sampler, 64>;

// Bindless material storage buffer
// Uses STORAGE_RESOURCE_BINDING_ARRAY + NON_UNIFORM_INDEXING
@group(1) @binding(2)
var<storage, read> materials: array<GpuMaterialData, 4096>;

// Draw call material index (set per-draw via push constant or uniform)
// This tells the shader which material to use for the current draw call
@group(0) @binding(1)
var<uniform> draw_call_material_index: u32;

// Lighting
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

// ── Vertex I/O ──────────────────────────────────────────────────

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
    @location(5) view_depth: f32,
    @location(6) motion_vector: vec2<f32>,
};

// ── Vertex Shader ───────────────────────────────────────────────

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    let model = camera.model;
    let normal_mat = camera.normal_matrix;

    let world_pos = model * vec4<f32>(in.position, 1.0);
    out.clip_position = camera.view_proj * world_pos;
    out.world_position = world_pos.xyz;

    out.world_normal = normalize((normal_mat * vec4<f32>(in.normal, 0.0)).xyz);

    out.uv = in.uv;
    out.color = in.color;

    // Shadow map coordinates
    out.shadow_position = shadow_uniform.light_view_proj * world_pos;
    out.view_depth = (camera.view_proj * world_pos).z;

    // Motion vector for TAA
    let prev_clip = camera.prev_view_proj * world_pos;
    let prev_ndc = prev_clip.xy / prev_clip.w;
    let curr_ndc = out.clip_position.xy / out.clip_position.w;
    out.motion_vector = (curr_ndc - prev_ndc) * 0.5;

    return out;
}

// ── Shadow Sampling ─────────────────────────────────────────────

fn calculate_shadow(shadow_pos: vec4<f32>) -> f32 {
    let ndc = shadow_pos.xyz / shadow_pos.w;
    let uv = ndc.xy * 0.5 + 0.5;

    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
        return 1.0;
    }

    let shadow_depth = ndc.z;
    let light_size = shadow_uniform.pcss_params.x;
    let map_size = shadow_uniform.pcss_params.y;
    let texel = 1.0 / map_size;

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

    // Blocker search
    let search_radius = light_size * texel * 8.0;
    var blocker_sum = 0.0;
    var blocker_count = 0.0;
    let shadow_dims = textureDimensions(t_shadow);
    for (var i = 0u; i < 16u; i++) {
        let sample_uv = uv + poisson[i] * search_radius;
        let texel_coords = vec2<i32>(sample_uv * vec2<f32>(shadow_dims));
        let d = textureLoad(t_shadow, texel_coords, 0);
        if (d < shadow_depth) {
            blocker_sum += d;
            blocker_count += 1.0;
        }
    }

    if (blocker_count < 0.5) {
        return 1.0;
    }

    // Penumbra estimation
    let avg_blocker = blocker_sum / blocker_count;
    let penumbra = light_size * (shadow_depth - avg_blocker) / avg_blocker;
    let filter_radius = max(penumbra * texel * 4.0, texel);

    // PCF
    var shadow = 0.0;
    for (var i = 0u; i < 16u; i++) {
        shadow += textureSampleCompare(t_shadow, s_shadow, uv + poisson[i] * filter_radius, shadow_depth);
    }

    return shadow / 16.0;
}

fn calculate_cascade_shadow(world_pos: vec3<f32>, view_depth: f32) -> f32 {
    var idx: u32 = CASCADE_COUNT - 1u;
    for (var i = 0u; i < CASCADE_COUNT - 1u; i++) {
        if (view_depth < cascades[i].split_depth) {
            idx = i;
            break;
        }
    }

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

// ── Fragment Shader ─────────────────────────────────────────────

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // ── Non-Uniform Material Indexing ───────────────────────────
    // Get material index for this draw call from uniform
    let mat_idx = draw_call_material_index;
    let mat = materials[mat_idx];

    // ── Bindless Texture Sampling with Non-Uniform Indexing ─────
    // The texture index comes from the material data, which varies per-draw-call.
    // This requires SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING.
    //
    // On Vulkan/D3D12, the compiler generates the appropriate non-uniform
    // indexing instructions. On WebGPU, this is supported when the feature
    // is enabled.

    var albedo_color = mat.base_color;
    var tex_alpha = 1.0;

    // Sample albedo texture if index is valid (not u32::MAX)
    if (mat.albedo_tex_index != 0xFFFFFFFFu) {
        // Non-uniform dynamic indexing of texture array
        // The texture index varies per draw call, not per-fragment
        let albedo_tex = textureSample(
            t_albedo_array[mat.albedo_tex_index],
            s_array[mat.sampler_index],
            in.uv
        );
        albedo_color = albedo_color * albedo_tex;
        tex_alpha = albedo_tex.a;
    }

    // Combine with vertex color
    let base = albedo_color * in.color;

    // ── Lighting ────────────────────────────────────────────────
    let n = normalize(in.world_normal);
    let shadow_pcss = calculate_shadow(in.shadow_position);
    let shadow_csm = calculate_cascade_shadow(in.world_position, in.view_depth);
    let is_in_cascade = in.view_depth < cascades[CASCADE_COUNT - 1u].split_depth;
    let shadow = select(shadow_pcss, shadow_csm, is_in_cascade);

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
    var final_color = base.rgb * lighting;

    // Emissive
    final_color = final_color + base.rgb * mat.emissive_strength;

    return vec4<f32>(final_color, base.a * tex_alpha);
}
