// Quasar Engine — PBR shader with Cook-Torrance BRDF and IBL.
//
// Implements:
// - Cook-Torrance specular BRDF (GGX distribution, Smith geometry, Schlick Fresnel)
// - Image-based lighting (IBL) from cubemap environment
// - Normal mapping
// - Multi-light support (directional, point, spot)

const PI: f32 = 3.14159265359;

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
    // Clearcoat layer (car paint, carbon fiber)
    clearcoat_weight: f32,      // 0.0 = none, 1.0 = full clearcoat
    clearcoat_roughness: f32,   // Roughness of clearcoat layer
    // Anisotropy (brushed metal, hair)
    anisotropy_strength: f32,   // 0.0 = isotropic, 1.0 = full anisotropy
    anisotropy_angle: f32,      // Rotation angle in radians
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

struct CascadeUniform {
    view_proj: mat4x4<f32>,
    split_depth: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};

const CASCADE_COUNT: u32 = 4u;

struct IblUniform {
    mip_count: f32,
    _pad: vec3<f32>,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;

@group(1) @binding(0) var<uniform> material: MaterialUniform;

@group(2) @binding(0) var<storage, read> lights: LightsUniform;

@group(3) @binding(0) var t_albedo: texture_2d<f32>;
@group(3) @binding(1) var s_albedo: sampler;
@group(3) @binding(2) var t_normal: texture_2d<f32>;
@group(3) @binding(3) var s_normal: sampler;
@group(3) @binding(4) var t_metallic_roughness: texture_2d<f32>;
@group(3) @binding(5) var s_metallic_roughness: sampler;
@group(3) @binding(6) var t_clearcoat: texture_2d<f32>;
@group(3) @binding(7) var s_clearcoat: sampler;
@group(3) @binding(8) var t_anisotropy: texture_2d<f32>;
@group(3) @binding(9) var s_anisotropy: sampler;

@group(4) @binding(0) var<uniform> shadow_uniform: ShadowUniform;
@group(4) @binding(1) var t_shadow: texture_depth_2d;
@group(4) @binding(2) var s_shadow: sampler_comparison;
@group(4) @binding(3) var s_shadow_depth: sampler;

// CSM cascade bindings
@group(4) @binding(4) var<storage, read> cascades: array<CascadeUniform, 4>;
@group(4) @binding(5) var t_cascade_shadow: texture_depth_2d_array;

fn select_cascade(view_depth: f32) -> u32 {
    for (var i = 0u; i < CASCADE_COUNT - 1u; i++) {
        if (view_depth < cascades[i].split_depth) {
            return i;
        }
    }
    return CASCADE_COUNT - 1u;
}

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

@group(5) @binding(0) var<uniform> ibl: IblUniform;
@group(5) @binding(1) var t_irradiance: texture_cube<f32>;
@group(5) @binding(2) var s_irradiance: sampler;
@group(5) @binding(3) var t_prefilter: texture_cube<f32>;
@group(5) @binding(4) var s_prefilter: sampler;
@group(5) @binding(5) var t_brdf_lut: texture_2d<f32>;
@group(5) @binding(6) var s_brdf_lut: sampler;

// LOD cross-fade dithering (blend = 0 → fully visible, blend = 1 → fully discarded).
@group(6) @binding(0) var<uniform> lod_crossfade_blend: f32;

fn bayer4x4(coord: vec2<u32>) -> f32 {
    let m = array<f32, 16>(
         0.0/16.0,  8.0/16.0,  2.0/16.0, 10.0/16.0,
        12.0/16.0,  4.0/16.0, 14.0/16.0,  6.0/16.0,
         3.0/16.0, 11.0/16.0,  1.0/16.0,  9.0/16.0,
        15.0/16.0,  7.0/16.0, 13.0/16.0,  5.0/16.0,
    );
    return m[(coord.y % 4u) * 4u + (coord.x % 4u)];
}

fn discard_crossfade(frag_coord: vec2<f32>, blend: f32) {
    let threshold = bayer4x4(vec2<u32>(u32(frag_coord.x), u32(frag_coord.y)));
    if threshold >= blend { discard; }
}

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
    @location(4) tangent: vec3<f32>,
    @location(5) bitangent: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) world_position: vec3<f32>,
    @location(4) shadow_position: vec4<f32>,
    @location(5) world_tangent: vec3<f32>,
    @location(6) world_bitangent: vec3<f32>,
    @location(7) view_depth: f32,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    let world_pos = camera.model * vec4<f32>(in.position, 1.0);
    out.clip_position = camera.view_proj * world_pos;
    out.world_position = world_pos.xyz;

    out.world_normal = normalize((camera.normal_matrix * vec4<f32>(in.normal, 0.0)).xyz);
    out.world_tangent = normalize((camera.normal_matrix * vec4<f32>(in.tangent, 0.0)).xyz);
    out.world_bitangent = normalize((camera.normal_matrix * vec4<f32>(in.bitangent, 0.0)).xyz);

    out.uv = in.uv;
    out.color = in.color;

    out.shadow_position = shadow_uniform.light_view_proj * world_pos;
    out.view_depth = (camera.view_proj * world_pos).z;

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

    // 16-sample Poisson disk shared by blocker search and PCF.
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

    // --- Step 1: Blocker search (read raw depth with non-comparison sampler) ---
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

    // --- Step 3: PCF with variable-size filter ---
    var shadow = 0.0;
    for (var i = 0u; i < 16u; i++) {
        shadow += textureSampleCompare(
            t_shadow, s_shadow,
            uv + poisson[i] * filter_radius,
            shadow_depth,
        );
    }

    return shadow / 16.0;
}

fn get_normal_from_map(uv: vec2<f32>, N: vec3<f32>, T: vec3<f32>, B: vec3<f32>) -> vec3<f32> {
    let tangent_normal = textureSample(t_normal, s_normal, uv).xyz * 2.0 - 1.0;
    let TBN = mat3x3<f32>(T, B, N);
    return normalize(TBN * tangent_normal);
}

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



// Clearcoat BRDF (simplified GGX on top layer)
fn distribution_ggx_clearcoat(N: vec3<f32>, H: vec3<f32>, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let NdotH = max(dot(N, H), 0.0);
    let NdotH2 = NdotH * NdotH;
    let num = a2;
    var denom = (NdotH2 * (a2 - 1.0) + 1.0);
    denom = PI * denom * denom;
    return num / denom;
}

// Anisotropic GGX distribution
fn distribution_ggx_aniso(
    N: vec3<f32>, H: vec3<f32>, 
    T: vec3<f32>, B: vec3<f32>,
    ax: f32, ay: f32
) -> f32 {
    let NdotH = max(dot(N, H), 0.0);
    let TdotH = dot(T, H);
    let BdotH = dot(B, H);
    
    let a2 = ax * ay;
    let NdotH2 = NdotH * NdotH;
    
    let term1 = TdotH * TdotH / (ax * ax);
    let term2 = BdotH * BdotH / (ay * ay);
    let term3 = NdotH2;
    
    let denom = PI * a2 * (term1 + term2 + term3) * (term1 + term2 + term3);
    return 1.0 / denom;
}

// Get anisotropic tangent frame rotated by anisotropy_angle
fn get_aniso_frame(T: vec3<f32>, B: vec3<f32>, angle: f32) -> (vec3<f32>, vec3<f32>) {
    let c = cos(angle);
    let s = sin(angle);
    let Tp = c * T + s * B;
    let Bp = -s * T + c * B;
    return (Tp, Bp);
}

fn fresnel_schlick_roughness(cosTheta: f32, F0: vec3<f32>, roughness: f32) -> vec3<f32> {
    return F0 + (max(vec3<f32>(1.0 - roughness), F0) - F0) * pow(clamp(1.0 - cosTheta, 0.0, 1.0), 5.0);
}

fn calculate_light(
    light: LightData,
    N: vec3<f32>,
    V: vec3<f32>,
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
        L = normalize(light.position.xyz - V);
        let distance = length(light.position.xyz - V);
        let range = light.params.y;
        attenuation = saturate(1.0 - (distance / range));
        attenuation = attenuation * attenuation;
    } else {
        let light_to_frag = light.position.xyz - V;
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
    // LOD cross-fade dithering: discard fragments based on Bayer pattern.
    if lod_crossfade_blend > 0.0 {
        discard_crossfade(in.clip_position.xy, lod_crossfade_blend);
    }

    let tex_color = textureSample(t_albedo, s_albedo, in.uv);
    let albedo = pow(tex_color.rgb * material.base_color.rgb * in.color.rgb, vec3<f32>(2.2));

    let mr_sample = textureSample(t_metallic_roughness, s_metallic_roughness, in.uv);
    let roughness = mr_sample.g * material.roughness_metallic[0];
    let metallic = mr_sample.b * material.roughness_metallic[1];

    var N = normalize(in.world_normal);
    var T = normalize(in.world_tangent);
    var B = normalize(in.world_bitangent);

    // Orthogonalize tangent frame against normal to handle degenerate cases
    // This ensures T and B are perpendicular to N
    let NdotT = dot(N, T);
    T = normalize(T - NdotT * N); // Gram-Schmidt orthogonalization
    B = normalize(cross(N, T));   // Reconstruct bitangent to ensure right-handed frame
    
    N = get_normal_from_map(in.uv, N, T, B);

    let V = normalize(camera.view_position - in.world_position);

    let F0 = mix(vec3<f32>(0.04), albedo, metallic);

    let pcss_shadow = calculate_shadow(in.shadow_position);
    let csm_shadow = calculate_cascade_shadow(in.world_position, in.view_depth);
    // Use CSM for pixels inside cascade frustums, fall back to PCSS for distant pixels
    // is_in_cascade is true when view_depth is within any cascade split depth
    let is_in_cascade = in.view_depth < cascades[CASCADE_COUNT - 1u].split_depth;
    let shadow = select(pcss_shadow, csm_shadow, is_in_cascade);

    var Lo = vec3<f32>(0.0);

    for (var i = 0u; i < lights.count; i++) {
        Lo += calculate_light(lights.lights[i], N, V, in.world_position, albedo, metallic, roughness, F0, shadow);
    }

    var ambient = calculate_ibl(N, V, albedo, metallic, roughness, F0);

    var color = ambient + Lo;

    color = color + albedo * material.emissive;

    return vec4<f32>(color, tex_color.a * material.base_color.a * in.color.a);
}
