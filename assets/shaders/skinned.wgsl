// Quasar Engine — Skinned mesh shader with GPU bone skinning.
//
// Performs vertex skinning in the vertex shader using bone matrices
// uploaded as a storage buffer.

struct CameraUniform {
    view_proj: mat4x4<f32>,
    model: mat4x4<f32>,
    normal_matrix: mat4x4<f32>,
    view_position: vec3<f32>,
    _pad1: f32,
};

struct MaterialUniform {
    base_color: vec4<f32>,
    roughness_metallic: vec2<f32>,
    emissive: f32,
    _pad: f32,
};

struct LightUniform {
    direction: vec4<f32>,
    color: vec4<f32>,
    ambient: vec4<f32>,
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

struct ShadowUniform {
    light_view_proj: mat4x4<f32>,
    // x = light_size (world-space), y = shadow_map_size (texels), z/w = unused
    pcss_params: vec4<f32>,
};

struct IblUniform {
    mip_count: f32,
    _pad: vec3<f32>,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<storage, read> bone_matrices: array<mat4x4<f32>>;
// Morph target buffers — deltas packed as [pos.xyz, normal.xyz] per vertex per target.
@group(0) @binding(2) var<storage, read> morph_deltas: array<f32>;
@group(0) @binding(3) var<storage, read> morph_weights: array<f32>;
// Number of morph targets and vertex count (packed in a uniform vec).
@group(0) @binding(4) var<uniform> morph_info: vec4<u32>; // x = target_count, y = vertex_count

@group(1) @binding(0) var<uniform> material: MaterialUniform;

@group(2) @binding(0) var<storage, read> lights: LightsUniform;

@group(3) @binding(0) var t_albedo: texture_2d<f32>;
@group(3) @binding(1) var s_albedo: sampler;
@group(3) @binding(2) var t_normal: texture_2d<f32>;
@group(3) @binding(3) var s_normal: sampler;
@group(3) @binding(4) var t_metallic_roughness: texture_2d<f32>;
@group(3) @binding(5) var s_metallic_roughness: sampler;

@group(4) @binding(0) var<uniform> shadow_uniform: ShadowUniform;
@group(4) @binding(1) var t_shadow: texture_depth_2d;
@group(4) @binding(2) var s_shadow: sampler_comparison;
@group(4) @binding(3) var s_shadow_depth: sampler;

@group(5) @binding(0) var<uniform> ibl: IblUniform;
@group(5) @binding(1) var t_irradiance: texture_cube<f32>;
@group(5) @binding(2) var s_irradiance: sampler;
@group(5) @binding(3) var t_prefilter: texture_cube<f32>;
@group(5) @binding(4) var s_prefilter: sampler;
@group(5) @binding(5) var t_brdf_lut: texture_2d<f32>;
@group(5) @binding(6) var s_brdf_lut: sampler;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
    @location(4) joint_indices: vec4<u32>,
    @location(5) joint_weights: vec4<f32>,
    @location(6) tangent: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) world_position: vec3<f32>,
    @location(4) shadow_position: vec4<f32>,
    @location(5) world_tangent: vec3<f32>,
};

// Apply morph target deltas (blend shapes) to position and normal.
fn apply_morph_targets(vertex_index: u32, base_pos: vec3<f32>, base_normal: vec3<f32>) -> array<vec3<f32>, 2> {
    var pos = base_pos;
    var nrm = base_normal;
    let target_count = morph_info.x;
    let vert_count = morph_info.y;

    for (var t = 0u; t < target_count; t++) {
        let w = morph_weights[t];
        if (abs(w) < 0.0001) {
            continue;
        }
        // 6 floats per vertex per target: pos(3) + normal(3)
        let base_idx = (t * vert_count + vertex_index) * 6u;
        pos += vec3<f32>(morph_deltas[base_idx], morph_deltas[base_idx + 1u], morph_deltas[base_idx + 2u]) * w;
        nrm += vec3<f32>(morph_deltas[base_idx + 3u], morph_deltas[base_idx + 4u], morph_deltas[base_idx + 5u]) * w;
    }

    return array<vec3<f32>, 2>(pos, normalize(nrm));
}

fn skin_position(position: vec3<f32>, joint_indices: vec4<u32>, joint_weights: vec4<f32>) -> vec3<f32> {
    var skinned_pos = vec3<f32>(0.0, 0.0, 0.0);
    
    for (var i = 0u; i < 4u; i++) {
        let joint_index = joint_indices[i];
        let weight = joint_weights[i];
        if (weight > 0.0) {
            let bone_matrix = bone_matrices[joint_index];
            skinned_pos += (bone_matrix * vec4<f32>(position, 1.0)).xyz * weight;
        }
    }
    
    return skinned_pos;
}

fn skin_normal(normal: vec3<f32>, joint_indices: vec4<u32>, joint_weights: vec4<f32>) -> vec3<f32> {
    var skinned_normal = vec3<f32>(0.0, 0.0, 0.0);
    
    for (var i = 0u; i < 4u; i++) {
        let joint_index = joint_indices[i];
        let weight = joint_weights[i];
        if (weight > 0.0) {
            let bone_matrix = bone_matrices[joint_index];
            skinned_normal += (bone_matrix * vec4<f32>(normal, 0.0)).xyz * weight;
        }
    }
    
    return normalize(skinned_normal);
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32, in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    // 1. Apply morph target deltas to base position/normal.
    let morphed = apply_morph_targets(vertex_index, in.position, in.normal);
    let morphed_position = morphed[0];
    let morphed_normal = morphed[1];

    // 2. Skin the morphed base attributes.
    let skinned_position = skin_position(morphed_position, in.joint_indices, in.joint_weights);
    let skinned_normal = skin_normal(morphed_normal, in.joint_indices, in.joint_weights);
    let skinned_tangent = skin_normal(in.tangent, in.joint_indices, in.joint_weights);

    let world_pos = camera.model * vec4<f32>(skinned_position, 1.0);
    out.clip_position = camera.view_proj * world_pos;
    out.world_position = world_pos.xyz;

    out.world_normal = normalize((camera.normal_matrix * vec4<f32>(skinned_normal, 0.0)).xyz);
    out.world_tangent = normalize((camera.normal_matrix * vec4<f32>(skinned_tangent, 0.0)).xyz);

    out.uv = in.uv;
    out.color = in.color;

    out.shadow_position = shadow_uniform.light_view_proj * world_pos;

    return out;
}

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

const PI: f32 = 3.14159265359;

fn distribution_ggx(N: vec3<f32>, H: vec3<f32>, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let NdotH = max(dot(N, H), 0.0);
    let NdotH2 = NdotH * NdotH;

    let num = a2;
    var denom = (NdotH2 * (a2 - 1.0) + 1.0);
    denom = PI * denom * denom;

    return num / denom;
}

fn geometry_schlick_ggx(NdotV: f32, roughness: f32) -> f32 {
    let r = (roughness + 1.0);
    let k = (r * r) / 8.0;

    let num = NdotV;
    let denom = NdotV * (1.0 - k) + k;

    return num / denom;
}

fn geometry_smith(N: vec3<f32>, V: vec3<f32>, L: vec3<f32>, roughness: f32) -> f32 {
    let NdotV = max(dot(N, V), 0.0);
    let NdotL = max(dot(N, L), 0.0);
    let ggx2 = geometry_schlick_ggx(NdotV, roughness);
    let ggx1 = geometry_schlick_ggx(NdotL, roughness);

    return ggx1 * ggx2;
}

fn fresnel_schlick(cosTheta: f32, F0: vec3<f32>) -> vec3<f32> {
    return F0 + (1.0 - F0) * pow(clamp(1.0 - cosTheta, 0.0, 1.0), 5.0);
}

fn fresnel_schlick_roughness(cosTheta: f32, F0: vec3<f32>, roughness: f32) -> vec3<f32> {
    return F0 + (max(vec3<f32>(1.0 - roughness), F0) - F0) * pow(clamp(1.0 - cosTheta, 0.0, 1.0), 5.0);
}

fn calculate_light(
    light: LightData,
    N: vec3<f32>,
    V: vec3<f32>,
    world_pos: vec3<f32>,
    albedo: vec3<f32>,
    metallic: f32,
    roughness: f32,
    F0: vec3<f32>,
    shadow: f32,
) -> vec3<f32> {
    var L: vec3<f32>;
    var attenuation: f32;

    let light_type = u32(light.params.x);

    if (light_type == 0u) {
        L = normalize(-light.direction.xyz);
        attenuation = 1.0;
    } else if (light_type == 1u) {
        L = normalize(light.position.xyz - world_pos);
        let distance = length(light.position.xyz - world_pos);
        let range = light.params.y;
        attenuation = saturate(1.0 - (distance / range));
        attenuation = attenuation * attenuation;
    } else {
        let light_to_frag = light.position.xyz - world_pos;
        let distance = length(light_to_frag);
        L = normalize(light_to_frag);

        let theta = dot(L, normalize(-light.direction.xyz));
        let inner_cutoff = light.params.y;
        let outer_cutoff = light.params.z;
        let epsilon = inner_cutoff - outer_cutoff;
        let spot_attenuation = saturate((theta - outer_cutoff) / epsilon);

        let range_attenuation = saturate(1.0 - (distance / light.params.w));
        attenuation = spot_attenuation * range_attenuation * range_attenuation;
    }

    let H = normalize(V + L);

    let NdotL = max(dot(N, L), 0.0);

    let radiance = light.color.rgb * light.color.a * attenuation * shadow;

    let NDF = distribution_ggx(N, H, roughness);
    let G = geometry_smith(N, V, L, roughness);
    let F = fresnel_schlick(max(dot(H, V), 0.0), F0);

    let numerator = NDF * G * F;
    var denominator = 4.0 * max(dot(N, V), 0.0) * max(dot(N, L), 0.0) + 0.0001;
    let specular = numerator / denominator;

    let kS = F;
    var kD = vec3<f32>(1.0) - kS;
    kD = kD * (1.0 - metallic);

    let diffuse = kD * albedo / PI;

    return (diffuse + specular) * radiance * NdotL;
}

fn calculate_ibl(
    N: vec3<f32>,
    V: vec3<f32>,
    albedo: vec3<f32>,
    metallic: f32,
    roughness: f32,
    F0: vec3<f32>,
) -> vec3<f32> {
    let R = reflect(-V, N);
    let NdotV = max(dot(N, V), 0.0);

    let F = fresnel_schlick_roughness(NdotV, F0, roughness);

    let kS = F;
    var kD = vec3<f32>(1.0) - kS;
    kD = kD * (1.0 - metallic);

    let irradiance = textureSample(t_irradiance, s_irradiance, N).rgb;
    let diffuse = kD * irradiance * albedo;

    let max_mip_level = ibl.mip_count - 1.0;
    let mip_level = roughness * max_mip_level;
    let prefiltered_color = textureSampleLevel(t_prefilter, s_prefilter, R, mip_level).rgb;

    let brdf = textureSample(t_brdf_lut, s_brdf_lut, vec2<f32>(NdotV, roughness)).rg;
    let specular = prefiltered_color * (F * brdf.x + brdf.y);

    return diffuse + specular;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let tex_color = textureSample(t_albedo, s_albedo, in.uv);
    let albedo = pow(tex_color.rgb * material.base_color.rgb * in.color.rgb, vec3<f32>(2.2));

    let mr_sample = textureSample(t_metallic_roughness, s_metallic_roughness, in.uv);
    let roughness = mr_sample.g * material.roughness_metallic[0];
    let metallic = mr_sample.b * material.roughness_metallic[1];

    let N = normalize(in.world_normal);
    let V = normalize(camera.view_position - in.world_position);

    let F0 = mix(vec3<f32>(0.04), albedo, metallic);

    let shadow = calculate_shadow(in.shadow_position);

    var Lo = vec3<f32>(0.0);

    for (var i = 0u; i < lights.count; i++) {
        Lo += calculate_light(lights.lights[i], N, V, in.world_position, albedo, metallic, roughness, F0, shadow);
    }

    var ambient = calculate_ibl(N, V, albedo, metallic, roughness, F0);

    var color = ambient + Lo;

    color = color + albedo * material.emissive;

    color = color / (color + vec3<f32>(1.0));
    color = pow(color, vec3<f32>(1.0 / 2.2));

    return vec4<f32>(color, tex_color.a * material.base_color.a * in.color.a);
}
