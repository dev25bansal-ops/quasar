// Quasar Engine — PCSS (Percentage-Closer Soft Shadows) Library
//
// Implements contact-hardening soft shadows with:
// - Adaptive blocker search
// - Penumbra estimation
// - Variable-size PCF filtering
// - Stratified Poisson sampling

// 16-sample Poisson disk for blocker search and PCF
const POISSON_16: array<vec2<f32>, 16> = array<vec2<f32>, 16>(
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

// 32-sample Poisson disk for higher quality
const POISSON_32: array<vec2<f32>, 32> = array<vec2<f32>, 32>(
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
    vec2<f32>( 0.52160436,  0.16312911),
    vec2<f32>(-0.70583957,  0.54581582),
    vec2<f32>( 0.12200000, -0.60300000),
    vec2<f32>(-0.46700000, -0.22500000),
    vec2<f32>( 0.63700000, -0.26700000),
    vec2<f32>(-0.30600000,  0.74200000),
    vec2<f32>( 0.84400000,  0.45700000),
    vec2<f32>(-0.58800000, -0.62500000),
    vec2<f32>( 0.09400000,  0.33700000),
    vec2<f32>(-0.15900000, -0.95800000),
    vec2<f32>( 0.33400000,  0.61800000),
    vec2<f32>(-0.94200000,  0.13700000),
    vec2<f32>( 0.58200000, -0.85900000),
    vec2<f32>(-0.63100000,  0.47300000),
    vec2<f32>( 0.78900000, -0.29500000),
    vec2<f32>(-0.28500000, -0.54200000),
);

// Find average blocker depth using adaptive search
// Returns (avg_blocker_depth, blocker_count)
fn find_blockers(
    shadow_map: texture_depth_2d,
    depth_sampler: sampler,
    uv: vec2<f32>,
    receiver_depth: f32,
    light_size: f32,
    search_width: f32,
) -> vec2<f32> {
    var blocker_sum = 0.0;
    var blocker_count = 0.0;
    
    // Adaptive sample count based on light size
    let sample_count = select(16u, 32u, light_size > 0.05);
    
    for (var i = 0u; i < sample_count; i++) {
        let offset = select(POISSON_16[i], POISSON_32[i], sample_count == 16u);
        let sample_uv = uv + offset * search_width;
        
        // Sample depth from shadow map
        let shadow_depth = textureSampleLevel(shadow_map, depth_sampler, sample_uv, 0.0);
        
        if (shadow_depth < receiver_depth) {
            blocker_sum += shadow_depth;
            blocker_count += 1.0;
        }
    }
    
    if (blocker_count > 0.0) {
        return vec2<f32>(blocker_sum / blocker_count, blocker_count);
    }
    return vec2<f32>(0.0, 0.0);
}

// Estimate penumbra width based on blocker depth
fn estimate_penumbra(
    receiver_depth: f32,
    avg_blocker_depth: f32,
    light_size: f32,
    texel_size: f32,
) -> f32 {
    // Penumbra estimation: w_penumbra = (d_receiver - d_blocker) * w_light / d_blocker
    let penumbra_ratio = (receiver_depth - avg_blocker_depth) / avg_blocker_depth;
    let penumbra = light_size * penumbra_ratio;
    
    // Clamp to reasonable range and convert to texel space
    return max(penumbra * texel_size * 4.0, texel_size);
}

// PCSS with contact hardening
fn pcss(
    shadow_map: texture_depth_2d,
    shadow_sampler: sampler_comparison,
    depth_sampler: sampler,
    uv: vec2<f32>,
    receiver_depth: f32,
    light_size: f32,
    shadow_map_size: f32,
) -> f32 {
    let texel_size = 1.0 / shadow_map_size;
    
    // Step 1: Blocker search
    let search_width = light_size * texel_size * 8.0;
    let blockers = find_blockers(
        shadow_map, depth_sampler,
        uv, receiver_depth,
        light_size, search_width
    );
    
    // No blockers = fully lit
    if (blockers.y < 0.5) {
        return 1.0;
    }
    
    // Step 2: Penumbra estimation
    let filter_radius = estimate_penumbra(
        receiver_depth,
        blockers.x,
        light_size,
        texel_size
    );
    
    // Step 3: PCF with variable filter size
    var shadow = 0.0;
    let sample_count = select(16u, 32u, filter_radius > texel_size * 4.0);
    
    for (var i = 0u; i < sample_count; i++) {
        let offset = select(POISSON_16[i], POISSON_32[i], sample_count == 16u);
        shadow += textureSampleCompare(
            shadow_map, shadow_sampler,
            uv + offset * filter_radius,
            receiver_depth
        );
    }
    
    return shadow / f32(sample_count);
}

// Contact Hardening Shadows (CHS) - shadows get sharper near contact points
fn chs_pcss(
    shadow_map: texture_depth_2d,
    shadow_sampler: sampler_comparison,
    depth_sampler: sampler,
    uv: vec2<f32>,
    receiver_depth: f32,
    light_size: f32,
    shadow_map_size: f32,
    min_filter_radius: f32,
) -> f32 {
    let texel_size = 1.0 / shadow_map_size;
    
    // Blocker search with larger search radius for CHS
    let search_width = light_size * texel_size * 16.0;
    let blockers = find_blockers(
        shadow_map, depth_sampler,
        uv, receiver_depth,
        light_size, search_width
    );
    
    if (blockers.y < 0.5) {
        return 1.0;
    }
    
    // Contact hardening: filter size decreases as receiver approaches blocker
    let distance_ratio = (receiver_depth - blockers.x) / max(receiver_depth, 0.001);
    let hardness = 1.0 - distance_ratio; // 0 = far, 1 = contact
    
    // Base filter radius from penumbra estimation
    let base_radius = estimate_penumbra(receiver_depth, blockers.x, light_size, texel_size);
    
    // Apply contact hardening: reduce filter size near contact
    let contact_hardened_radius = mix(
        base_radius,
        min_filter_radius * texel_size,
        hardness * hardness // Quadratic falloff for smoother transition
    );
    
    // PCF with contact-hardened filter
    var shadow = 0.0;
    let sample_count = 32u;
    
    for (var i = 0u; i < sample_count; i++) {
        shadow += textureSampleCompare(
            shadow_map, shadow_sampler,
            uv + POISSON_32[i] * contact_hardened_radius,
            receiver_depth
        );
    }
    
    return shadow / f32(sample_count);
}

// PCSS for cascade shadow maps
fn pcss_cascade(
    shadow_map: texture_depth_2d_array,
    shadow_sampler: sampler_comparison,
    depth_sampler: sampler,
    uv: vec2<f32>,
    cascade_index: i32,
    receiver_depth: f32,
    light_size: f32,
    shadow_map_size: f32,
) -> f32 {
    let texel_size = 1.0 / shadow_map_size;
    let search_width = light_size * texel_size * 8.0;
    
    // Blocker search in cascade
    var blocker_sum = 0.0;
    var blocker_count = 0.0;
    
    for (var i = 0u; i < 16u; i++) {
        let sample_uv = uv + POISSON_16[i] * search_width;
        let shadow_depth = textureSampleLevel(
            shadow_map, depth_sampler,
            sample_uv, cascade_index, 0.0
        );
        
        if (shadow_depth < receiver_depth) {
            blocker_sum += shadow_depth;
            blocker_count += 1.0;
        }
    }
    
    if (blocker_count < 0.5) {
        return 1.0;
    }
    
    let avg_blocker = blocker_sum / blocker_count;
    let penumbra = estimate_penumbra(receiver_depth, avg_blocker, light_size, texel_size);
    
    // PCF
    var shadow = 0.0;
    for (var i = 0u; i < 16u; i++) {
        shadow += textureSampleCompare(
            shadow_map, shadow_sampler,
            uv + POISSON_16[i] * penumbra,
            cascade_index,
            receiver_depth
        );
    }
    
    return shadow / 16.0;
}

// Interleave gradient noise for temporal filtering
fn interleaved_gradient_noise(pixel_coord: vec2<f32>) -> f32 {
    let magic = vec3<f32>(0.06711056, 0.00583715, 52.9829189);
    return fract(magic.z * fract(dot(pixel_coord, magic.xy)));
}

// Rotated Poisson disk for temporal variance reduction
fn rotate_poisson(offset: vec2<f32>, angle: f32) -> vec2<f32> {
    let c = cos(angle);
    let s = sin(angle);
    return vec2<f32>(
        offset.x * c - offset.y * s,
        offset.x * s + offset.y * c
    );
}

// Temporal PCSS with rotation for TAA integration
fn pcss_temporal(
    shadow_map: texture_depth_2d,
    shadow_sampler: sampler_comparison,
    depth_sampler: sampler,
    uv: vec2<f32>,
    receiver_depth: f32,
    light_size: f32,
    shadow_map_size: f32,
    pixel_coord: vec2<f32>,
    frame_index: u32,
) -> f32 {
    let texel_size = 1.0 / shadow_map_size;
    let search_width = light_size * texel_size * 8.0;
    
    // Use frame index for rotation
    let rotation_angle = f32(frame_index % 8u) * 0.785398; // PI / 4
    
    // Blocker search
    var blocker_sum = 0.0;
    var blocker_count = 0.0;
    
    for (var i = 0u; i < 16u; i++) {
        let rotated = rotate_poisson(POISSON_16[i], rotation_angle);
        let sample_uv = uv + rotated * search_width;
        let shadow_depth = textureSampleLevel(shadow_map, depth_sampler, sample_uv, 0.0);
        
        if (shadow_depth < receiver_depth) {
            blocker_sum += shadow_depth;
            blocker_count += 1.0;
        }
    }
    
    if (blocker_count < 0.5) {
        return 1.0;
    }
    
    let avg_blocker = blocker_sum / blocker_count;
    let filter_radius = estimate_penumbra(receiver_depth, avg_blocker, light_size, texel_size);
    
    // Add gradient noise offset for temporal variance
    let noise = interleaved_gradient_noise(pixel_coord);
    let noise_offset = vec2<f32>(cos(noise * 6.28318), sin(noise * 6.28318)) * texel_size * 0.5;
    
    // PCF with temporal rotation
    var shadow = 0.0;
    for (var i = 0u; i < 16u; i++) {
        let rotated = rotate_poisson(POISSON_16[i], rotation_angle);
        shadow += textureSampleCompare(
            shadow_map, shadow_sampler,
            uv + rotated * filter_radius + noise_offset,
            receiver_depth
        );
    }
    
    return shadow / 16.0;
}
