// Quasar Engine — Bloom shader.
//
// Implements bloom effect with luminance threshold and Kawase blur.

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@group(0) @binding(0) var t_source: texture_2d<f32>;
@group(0) @binding(1) var s_source: sampler;

// Bloom parameters (hardcoded for simplicity)
const BLOOM_THRESHOLD: f32 = 0.8;
const BLOOM_INTENSITY: f32 = 0.3;
const BLOOM_BLUR_DIRECTION: vec2<f32> = vec2<f32>(1.0, 0.0);

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

fn luminance(color: vec3<f32>) -> f32 {
    return dot(color, vec3<f32>(0.299, 0.587, 0.114));
}

@fragment
fn fs_bloom_extract(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(t_source, s_source, in.uv).rgb;
    let lum = luminance(color);

    if (lum > BLOOM_THRESHOLD) {
        let brightness = (lum - BLOOM_THRESHOLD) / (1.0 - BLOOM_THRESHOLD);
        return vec4<f32>(color * brightness, 1.0);
    }

    return vec4<f32>(0.0, 0.0, 0.0, 1.0);
}

@fragment
fn fs_bloom_blur(in: VertexOutput) -> @location(0) vec4<f32> {
    let texel_size = vec2<f32>(1.0) / vec2<f32>(textureDimensions(t_source));
    let dir = BLOOM_BLUR_DIRECTION * texel_size;

    var color = vec3<f32>(0.0);

    // Kawase blur - 5-tap
    color += textureSample(t_source, s_source, in.uv + dir * 0.0).rgb;
    color += textureSample(t_source, s_source, in.uv + dir * 1.0).rgb;
    color += textureSample(t_source, s_source, in.uv - dir * 1.0).rgb;
    color += textureSample(t_source, s_source, in.uv + dir * 2.0).rgb;
    color += textureSample(t_source, s_source, in.uv - dir * 2.0).rgb;

    color /= 5.0;

    return vec4<f32>(color, 1.0);
}

@fragment
fn fs_bloom_combine(in: VertexOutput) -> @location(0) vec4<f32> {
    let bloom = textureSample(t_source, s_source, in.uv).rgb;
    return vec4<f32>(bloom * BLOOM_INTENSITY, 1.0);
}
