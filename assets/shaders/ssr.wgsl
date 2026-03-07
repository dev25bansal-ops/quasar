// Screen-Space Reflections — trace + temporal resolve.
//
// Pipeline:
//   1. fs_ssr_trace   — ray-march reflected direction in screen-space.
//   2. fs_ssr_resolve — temporal blend with previous frame.

struct SsrUniform {
    inv_view_proj: mat4x4<f32>,
    view_proj: mat4x4<f32>,
    camera_pos_steps: vec4<f32>,
    params: vec4<f32>,
    resolution: vec4<f32>,
};

// GBuffer (group 0)
@group(0) @binding(0) var t_albedo: texture_2d<f32>;
@group(0) @binding(1) var t_normal: texture_2d<f32>;
@group(0) @binding(2) var t_rm: texture_2d<f32>;
@group(0) @binding(3) var t_depth: texture_depth_2d;
@group(0) @binding(4) var s_gbuffer: sampler;

// SSR (group 1)
@group(1) @binding(0) var<uniform> ssr: SsrUniform;
@group(1) @binding(1) var t_scene: texture_2d<f32>;
@group(1) @binding(2) var t_scene_depth: texture_depth_2d;
@group(1) @binding(3) var t_history: texture_2d<f32>;
@group(1) @binding(4) var s_ssr: sampler;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_fullscreen(@builtin(vertex_index) idx: u32) -> VsOut {
    var positions = array<vec2<f32>, 3>(
        vec2(-1.0, -1.0),
        vec2( 3.0, -1.0),
        vec2(-1.0,  3.0),
    );
    var out: VsOut;
    out.pos = vec4(positions[idx], 0.0, 1.0);
    out.uv = (positions[idx] + 1.0) * 0.5;
    out.uv.y = 1.0 - out.uv.y;
    return out;
}

fn reconstruct_position(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let ndc = vec4<f32>(uv * 2.0 - 1.0, depth, 1.0);
    let world = ssr.inv_view_proj * ndc;
    return world.xyz / world.w;
}

@fragment
fn fs_ssr_trace(in: VsOut) -> @location(0) vec4<f32> {
    let depth = textureSample(t_depth, s_gbuffer, in.uv);
    if depth >= 1.0 { return vec4(0.0); }

    let roughness = textureSample(t_rm, s_gbuffer, in.uv).r;
    let max_roughness = ssr.params.z;
    if roughness > max_roughness { return vec4(0.0); }

    let world_pos = reconstruct_position(in.uv, depth);
    let normal = normalize(textureSample(t_normal, s_gbuffer, in.uv).xyz * 2.0 - 1.0);
    let camera_pos = ssr.camera_pos_steps.xyz;
    let view_dir = normalize(world_pos - camera_pos);
    let reflect_dir = reflect(view_dir, normal);

    let max_steps = u32(ssr.camera_pos_steps.w);
    let step_sz = ssr.params.x;
    let thickness = ssr.params.y;

    var ray_pos = world_pos + reflect_dir * 0.01;
    var hit_color = vec3<f32>(0.0);
    var confidence = 0.0;

    for (var i: u32 = 0u; i < max_steps; i = i + 1u) {
        ray_pos += reflect_dir * step_sz * (1.0 + f32(i) * 0.05);

        let clip = ssr.view_proj * vec4(ray_pos, 1.0);
        if clip.w <= 0.0 { break; }
        let ray_uv = (clip.xy / clip.w) * 0.5 + 0.5;
        let ray_uv_flipped = vec2(ray_uv.x, 1.0 - ray_uv.y);

        if ray_uv_flipped.x < 0.0 || ray_uv_flipped.x > 1.0 ||
           ray_uv_flipped.y < 0.0 || ray_uv_flipped.y > 1.0 { break; }

        let scene_depth = textureSample(t_scene_depth, s_ssr, ray_uv_flipped);
        let ray_depth = clip.z / clip.w;
        let diff = ray_depth - scene_depth;

        if diff > 0.0 && diff < thickness {
            hit_color = textureSample(t_scene, s_ssr, ray_uv_flipped).rgb;
            // Fade near screen edges
            let edge_fade = smoothstep(0.0, 0.05, min(
                min(ray_uv_flipped.x, 1.0 - ray_uv_flipped.x),
                min(ray_uv_flipped.y, 1.0 - ray_uv_flipped.y)
            ));
            confidence = edge_fade * (1.0 - roughness / max_roughness);
            break;
        }
    }

    return vec4(hit_color, confidence);
}

@fragment
fn fs_ssr_resolve(in: VsOut) -> @location(0) vec4<f32> {
    let current = textureSample(t_scene, s_ssr, in.uv);
    let history = textureSample(t_history, s_ssr, in.uv);
    let temporal_w = ssr.params.w;
    return mix(current, history, temporal_w);
}
