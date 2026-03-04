// Quasar Engine — SSAO (Screen-Space Ambient Occlusion) shader.
//
// Implements SSAO using a hemisphere sampling kernel.

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

struct SsaoParams {
    radius: f32,
    bias: f32,
    kernel_size: u32,
    _pad: u32,
};

@group(0) @binding(0) var t_position: texture_2d<f32>;
@group(0) @binding(1) var t_normal: texture_2d<f32>;
@group(0) @binding(2) var t_depth: texture_2d<f32>;
@group(0) @binding(3) var<uniform> params: SsaoParams;
@group(0) @binding(4) var s_source: sampler;

@group(1) @binding(0) var<storage, read> kernel: array<vec4<f32>>;
@group(1) @binding(1) var t_noise: texture_2d<f32>;
@group(1) @binding(2) var s_noise: sampler;

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

@fragment
fn fs_ssao(in: VertexOutput) -> @location(0) vec4<f32> {
    let position = textureSample(t_position, s_source, in.uv).xyz;
    let normal = textureSample(t_normal, s_source, in.uv).xyz;

    let noise_scale = vec2<f32>(textureDimensions(t_position)) / vec2<f32>(textureDimensions(t_noise));
    let random_vec = textureSample(t_noise, s_noise, in.uv * noise_scale).xyz;

    let tangent = normalize(random_vec - normal * dot(random_vec, normal));
    let bitangent = cross(normal, tangent);
    let tbn = mat3x3<f32>(tangent, bitangent, normal);

    var occlusion = 0.0;
    let kernel_size = int(params.kernel_size);

    for (var i = 0; i < 64; i++) {
        if (i >= kernel_size) {
            break;
        }

        let sample_vec = tbn * kernel[i].xyz;
        let sample_pos = position + sample_vec * params.radius;

        let sample_uv = sample_pos.xy;
        let sample_depth = textureSample(t_depth, s_source, sample_uv).r;

        let range_check = smoothstep(0.0, 1.0, params.radius / abs(position.z - sample_depth));
        occlusion += (sample_depth >= sample_pos.z + params.bias ? 1.0 : 0.0) * range_check;
    }

    occlusion = 1.0 - (occlusion / f32(kernel_size));

    return vec4<f32>(occlusion, 0.0, 0.0, 1.0);
}

@fragment
fn fs_ssao_blur(in: VertexOutput) -> @location(0) vec4<f32> {
    let texel_size = vec2<f32>(1.0) / vec2<f32>(textureDimensions(t_depth));

    var result = 0.0;
    let blur_radius = 2;

    for (var x = -2; x <= 2; x++) {
        for (var y = -2; y <= 2; y++) {
            let offset = vec2<f32>(f32(x), f32(y)) * texel_size;
            result += textureSample(t_depth, s_source, in.uv + offset).r;
        }
    }

    result /= 25.0;

    return vec4<f32>(result, 0.0, 0.0, 1.0);
}
