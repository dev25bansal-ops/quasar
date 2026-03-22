// Quasar Engine — FXAA (Fast Approximate Anti-Aliasing) shader.
//
// Implements the FXAA algorithm for edge smoothing.

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
    );

    let uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
    );

    var out: VertexOutput;
    out.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    out.uv = uvs[vertex_index];
    return out;
}

@group(0) @binding(0) var t_source: texture_2d<f32>;
@group(0) @binding(1) var s_source: sampler;

const FXAA_SPAN_MAX: f32 = 8.0;
const FXAA_REDUCE_MIN: f32 = 1.0 / 128.0;
const FXAA_REDUCE_MUL: f32 = 1.0 / 8.0;

@fragment
fn fs_fxaa(in: VertexOutput) -> @location(0) vec4<f32> {
    let texel_size = vec2<f32>(1.0) / vec2<f32>(textureDimensions(t_source));

    let rgb_nw = textureSample(t_source, s_source, in.uv + vec2<f32>(-1.0, -1.0) * texel_size).rgb;
    let rgb_ne = textureSample(t_source, s_source, in.uv + vec2<f32>( 1.0, -1.0) * texel_size).rgb;
    let rgb_sw = textureSample(t_source, s_source, in.uv + vec2<f32>(-1.0,  1.0) * texel_size).rgb;
    let rgb_se = textureSample(t_source, s_source, in.uv + vec2<f32>( 1.0,  1.0) * texel_size).rgb;
    let rgb_m = textureSample(t_source, s_source, in.uv).rgb;

    let luma_nw = dot(rgb_nw, vec3<f32>(0.299, 0.587, 0.114));
    let luma_ne = dot(rgb_ne, vec3<f32>(0.299, 0.587, 0.114));
    let luma_sw = dot(rgb_sw, vec3<f32>(0.299, 0.587, 0.114));
    let luma_se = dot(rgb_se, vec3<f32>(0.299, 0.587, 0.114));
    let luma_m = dot(rgb_m, vec3<f32>(0.299, 0.587, 0.114));

    let luma_min = min(luma_m, min(min(luma_nw, luma_ne), min(luma_sw, luma_se)));
    let luma_max = max(luma_m, max(max(luma_nw, luma_ne), max(luma_sw, luma_se)));
    let luma_range = luma_max - luma_min;

    if (luma_range < max(0.0312, luma_max * 0.125)) {
        return vec4<f32>(rgb_m, 1.0);
    }

    let dir = vec2<f32>(
        -((luma_nw + luma_ne) - (luma_sw + luma_se)),
        ((luma_nw + luma_sw) - (luma_ne + luma_se))
    );

    let dir_reduce = max((luma_nw + luma_ne + luma_sw + luma_se) * 0.25 * FXAA_REDUCE_MUL, FXAA_REDUCE_MIN);
    let rcp_dir_min = 1.0 / (min(abs(dir.x), abs(dir.y)) + dir_reduce);

    let dir_scaled = min(vec2<f32>(FXAA_SPAN_MAX), max(vec2<f32>(-FXAA_SPAN_MAX), dir * rcp_dir_min)) * texel_size;

    let rgb_a = 0.5 * (
        textureSample(t_source, s_source, in.uv + dir_scaled * 1.0 / 3.0 - 4.0).rgb +
        textureSample(t_source, s_source, in.uv + dir_scaled * 2.0 / 3.0 - 4.0).rgb
    );
    let rgb_b = rgb_a * 0.25 + 0.25 * (
        textureSample(t_source, s_source, in.uv + dir_scaled * -0.5).rgb +
        textureSample(t_source, s_source, in.uv + dir_scaled * 0.5).rgb +
        textureSample(t_source, s_source, in.uv + dir_scaled * -1.0).rgb +
        textureSample(t_source, s_source, in.uv + dir_scaled * 1.0).rgb
    );

    let luma_b = dot(rgb_b, vec3<f32>(0.299, 0.587, 0.114));

    if (luma_b < luma_min || luma_b > luma_max) {
        return vec4<f32>(rgb_a, 1.0);
    }

    return vec4<f32>(rgb_b, 1.0);
}
