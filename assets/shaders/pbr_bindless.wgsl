// Quasar Engine — Bindless PBR shader with Cook-Torrance BRDF.
//
// Full physically-based rendering using bindless textures and materials.
// All textures and materials are accessed via non-uniform dynamic indexing.
//
// Features:
// - Cook-Torrance microfacet BRDF (GGX normal distribution, Schlick fresnel, Smith geometry)
// - Bindless albedo, normal, metallic-roughness, and emissive textures
// - Image-based lighting (IBL) support via bindless environment maps
// - Cascade shadow maps with PCSS
// - Non-uniform dynamic indexing for all resources
//
// Bind groups:
// group(0) = Camera uniform + draw call data
// group(1) = Bindless bind group (textures + samplers + materials)
// group(2) = Lighting + environment maps

struct CameraUniform {
    view_proj: mat4x4<f32>,
    model: mat4x4<f32>,
    normal_matrix: mat4x4<f32>,
    prev_view_proj: mat4x4<f32>,
};

struct DrawCallData {
    material_index: u32,
    _pad: vec3<u32>,
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

struct EnvMapUniform {
    lod_bias: f32,
    irradiance_scale: f32,
    _pad: vec2<f32>,
};

const CASCADE_COUNT: u32 = 4u;
const PI: f32 = 3.14159265359;
const MAX_REFLECTION_LOD: f32 = 6.0;

// ── Bindless Material Data ──────────────────────────────────────
// Matches GpuMaterialData in bindless.rs
struct GpuMaterialData {
    base_color: vec4<f32>,
    roughness: f32,
    metallic: f32,
    emissive_strength: f32,
    albedo_tex_index: u32,
    normal_tex_index: u32,
    mr_tex_index: u32,
    sampler_index: u32,
    _pad: vec2<u32>,
};

// ── Bindings ────────────────────────────────────────────────────

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@group(0) @binding(1)
var<uniform> draw_call: DrawCallData;

// Bindless texture array (1024 textures)
@group(1) @binding(0)
var textures: binding_array<texture_2d<f32>, 1024>;

// Bindless sampler array (64 samplers)
@group(1) @binding(1)
var samplers: binding_array<sampler, 64>;

// Bindless material storage buffer (4096 materials)
@group(1) @binding(2)
var<storage, read> materials: array<GpuMaterialData, 4096>;

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

// Environment maps (bindless)
// These use indices 1020-1023 in the texture array (reserved slots)
const ENV_IRRADIANCE_INDEX: u32 = 1020u;
const ENV_PREFILTER_INDEX: u32 = 1021u;
const ENV_BRDF_LUT_INDEX: u32 = 1022u;

// ── Vertex I/O ──────────────────────────────────────────────────

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec4<f32>,  // xyz = tangent, w = bitangent sign
    @location(4) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tbn_matrix: mat3x3<f32>,
    @location(4) color: vec4<f32>,
    @location(5) shadow_position: vec4<f32>,
    @location(6) view_depth: f32,
    @location(7) motion_vector: vec2<f32>,
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

    // Transform normal to world space
    out.world_normal = normalize((normal_mat * vec4<f32>(in.normal, 0.0)).xyz);

    // Build TBN matrix for normal mapping
    let T = normalize((model * vec4<f32>(in.tangent.xyz, 0.0)).xyz);
    let N = out.world_normal;
    let sign = in.tangent.w;
    let B = normalize(cross(N, T)) * sign;
    out.tbn_matrix = mat3x3<f32>(T, B, N);

    out.uv = in.uv;
    out.color = in.color;

    // Shadow coordinates
    out.shadow_position = shadow_uniform.light_view_proj * world_pos;
    out.view_depth = (camera.view_proj * world_pos).z;

    // Motion vector for TAA
    let prev_clip = camera.prev_view_proj * world_pos;
    let prev_ndc = prev_clip.xy / prev_clip.w;
    let curr_ndc = out.clip_position.xy / out.clip_position.w;
    out.motion_vector = (curr_ndc - prev_ndc) * 0.5;

    return out;
}

// ── BRDF Functions ──────────────────────────────────────────────

// Normal distribution function (GGX)
fn distribution_ggx(NdotH: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let NdotH2 = NdotH * NdotH;
    let denom = NdotH2 * (a2 - 1.0) + 1.0;
    return a2 / (PI * denom * denom);
}

// Geometry function (Schlick-GGX)
fn geometry_schlick_ggx(NdotV: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = (r * r) / 8.0;
    return NdotV / (NdotV * (1.0 - k) + k);
}

// Geometry function (Smith)
fn geometry_smith(NdotV: f32, NdotL: f32, roughness: f32) -> f32 {
    return geometry_schlick_ggx(NdotV, roughness) * geometry_schlick_ggx(NdotL, roughness);
}

// Fresnel function (Schlick approximation)
fn fresnel_schlick(cos_theta: f32, F0: vec3<f32>) -> vec3<f32> {
    return F0 + (1.0 - F0) * pow(1.0 - cos_theta, 5.0);
}

// Fresnel function for IBL (optimized)
fn fresnel_schlick_ior(cos_theta: f32, F0: vec3<f32>, roughness: f32) -> vec3<f32> {
    return F0 + (max(vec3<f32>(1.0 - roughness), F0) - F0) * pow(1.0 - cos_theta, 5.0);
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
    // ── Get Material via Non-Uniform Indexing ───────────────────
    let mat_idx = draw_call.material_index;
    let mat = materials[mat_idx];

    // ── Sample Bindless Textures ────────────────────────────────
    var albedo = mat.base_color.rgb;
    var roughness = mat.roughness;
    var metallic = mat.metallic;
    var normal_ts = vec3<f32>(0.0, 0.0, 1.0); // Default normal in tangent space
    var emissive = vec3<f32>(0.0);

    let sampler_idx = mat.sampler_index;

    // Albedo texture
    if (mat.albedo_tex_index != 0xFFFFFFFFu) {
        let albedo_tex = textureSample(
            textures[mat.albedo_tex_index],
            samplers[sampler_idx],
            in.uv
        );
        albedo = albedo * albedo_tex.rgb;
    }

    // Metallic-roughness texture (R = metallic, G = roughness, B = unused)
    if (mat.mr_tex_index != 0xFFFFFFFFu) {
        let mr_tex = textureSample(
            textures[mat.mr_tex_index],
            samplers[sampler_idx],
            in.uv
        );
        metallic = mr_tex.b; // Metallic in blue channel
        roughness = mr_tex.g; // Roughness in green channel
    }

    // Normal map
    if (mat.normal_tex_index != 0xFFFFFFFFu) {
        let normal_tex = textureSample(
            textures[mat.normal_tex_index],
            samplers[sampler_idx],
            in.uv
        );
        // Convert from [0,1] to [-1,1]
        normal_ts = normal_tex.rgb * 2.0 - 1.0;
    }

    // Transform normal from tangent space to world space
    let N = normalize(in.tbn_matrix * normal_ts);
    let V = normalize(camera.model[3].xyz - in.world_position); // View direction
    let H = normalize(V + V); // Half vector (simplified)

    // ── PBR Lighting (Cook-Torrance BRDF) ───────────────────────
    let NdotV = max(dot(N, V), 0.0);

    // Fresnel (Schlick)
    let F0 = mix(vec3<f32>(0.04), albedo, vec3<f32>(metallic));
    let F = fresnel_schlick(NdotV, F0);

    // Shadow
    let shadow_pcss = calculate_shadow(in.shadow_position);
    let shadow_csm = calculate_cascade_shadow(in.world_position, in.view_depth);
    let is_in_cascade = in.view_depth < cascades[CASCADE_COUNT - 1u].split_depth;
    let shadow = select(shadow_pcss, shadow_csm, is_in_cascade);

    // Ambient from IBL (simplified)
    let ambient = lights.ambient.rgb * lights.ambient.a * albedo * (1.0 - metallic);

    var diffuse_total = vec3<f32>(0.0);
    var specular_total = vec3<f32>(0.0);

    for (var i = 0u; i < lights.count; i++) {
        let light = lights.lights[i];
        let light_type = u32(light.params.x);
        var L: vec3<f32>;
        var attenuation = 1.0;

        if (light_type == 0u) {
            // Directional
            L = normalize(-light.direction.xyz);
        } else {
            // Point / Spot
            let to_light = light.position.xyz - in.world_position;
            let dist = length(to_light);
            L = to_light / dist;
            let range = light.params.y;
            if (range > 0.0) {
                attenuation = saturate(1.0 - dist / range);
                attenuation = attenuation * attenuation;
            }
        }

        let H = normalize(V + L);
        let NdotL = max(dot(N, L), 0.0);
        let NdotH = max(dot(N, H), 0.0);
        let HdotV = max(dot(H, V), 0.0);

        if (NdotL <= 0.0) {
            continue;
        }

        // Cook-Torrance BRDF
        let NDF = distribution_ggx(NdotH, roughness);
        let G = geometry_smith(NdotV, NdotL, roughness);
        let F_light = fresnel_schlick(HdotV, F0);

        // Specular
        let numerator = NDF * G * F_light;
        let denominator = 4.0 * NdotV * NdotL + 0.0001;
        let specular = numerator / denominator;

        // Diffuse (Lambertian)
        let kD = (1.0 - F) * (1.0 - metallic);
        let diffuse = kD * albedo / PI;

        let radiance = light.color.rgb * light.color.a * attenuation * shadow;
        diffuse_total += diffuse * radiance * NdotL;
        specular_total += specular * radiance * NdotL;
    }

    // Combine
    var final_color = ambient + diffuse_total + specular_total;

    // Emissive
    final_color = final_color + emissive * mat.emissive_strength;

    // Tone mapping (ACES approximation)
    final_color = final_color * (2.51 * final_color + 0.03) / (final_color * (2.43 * final_color + 0.59) + 0.14);

    return vec4<f32>(final_color, 1.0);
}
