// Quasar Engine — TAA (Temporal Anti-Aliasing) resolve shader.
//
// Blends the current jittered frame with the reprojected history buffer.
// Uses a neighbourhood clamp to prevent ghosting.

struct TaaUniforms {
    // (texel_size_x, texel_size_y, blend_factor, _pad)
    params: vec4<f32>,
};

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
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(1.0, 0.0),
    );

    var out: VertexOutput;
    out.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    out.uv = uvs[vertex_index];
    return out;
}

@group(0) @binding(0) var t_current: texture_2d<f32>;
@group(0) @binding(1) var t_history: texture_2d<f32>;
@group(0) @binding(2) var t_motion: texture_2d<f32>;
@group(0) @binding(3) var s_linear: sampler;
@group(0) @binding(4) var<uniform> uniforms: TaaUniforms;

// Convert RGB to YCoCg for neighbourhood clamping (more perceptually correct).
fn rgb_to_ycocg(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
         0.25 * c.r + 0.5 * c.g + 0.25 * c.b,
         0.5  * c.r              - 0.5  * c.b,
        -0.25 * c.r + 0.5 * c.g - 0.25 * c.b,
    );
}

fn ycocg_to_rgb(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        c.x + c.y - c.z,
        c.x       + c.z,
        c.x - c.y - c.z,
    );
}

@fragment
fn fs_taa(in: VertexOutput) -> @location(0) vec4<f32> {
    let texel = vec2<f32>(uniforms.params.x, uniforms.params.y);
    let blend = uniforms.params.z;

    // Sample motion vector to reproject into previous frame.
    let motion = textureSample(t_motion, s_linear, in.uv).rg;
    let history_uv = in.uv - motion;

    // Current frame sample.
    let current = textureSample(t_current, s_linear, in.uv).rgb;

    // Neighbourhood min/max in YCoCg for clamping (3×3).
    var colour_min = vec3<f32>(1e10);
    var colour_max = vec3<f32>(-1e10);
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let offset = vec2<f32>(f32(x), f32(y)) * texel;
            let s = textureSample(t_current, s_linear, in.uv + offset).rgb;
            let ycocg = rgb_to_ycocg(s);
            colour_min = min(colour_min, ycocg);
            colour_max = max(colour_max, ycocg);
        }
    }

    // Sample and clamp history.
    var history = textureSample(t_history, s_linear, history_uv).rgb;

    // Reject history if reprojected UV is out of bounds.
    if history_uv.x < 0.0 || history_uv.x > 1.0 || history_uv.y < 0.0 || history_uv.y > 1.0 {
        return vec4<f32>(current, 1.0);
    }

    let history_ycocg = rgb_to_ycocg(history);
    let clamped = clamp(history_ycocg, colour_min, colour_max);
    history = ycocg_to_rgb(clamped);

    // Exponential blend: lerp towards current frame.
    let result = mix(history, current, blend);

    return vec4<f32>(result, 1.0);
}
