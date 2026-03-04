// Tonemapping shader — converts HDR to LDR.
//
// Binds:
// group(0) binding(0) = HDR texture (rgba16float)
// group(0) binding(1) = sampler

@group(0) @binding(0)
var t_hdr: texture_2D<f32>;

@group(0) @binding(1)
var s_hdr: sampler;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    // Fullscreen triangle
    var out: VertexOutput;
    let uv = vec2<f32>(
        f32((vertex_index << 1u) & 2u),
        f32(vertex_index & 2u),
    );
    out.clip_position = vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
    out.uv = uv;
    return out;
}

// Reinhard tonemapping
fn tonemap_reinhard(color: vec3<f32>) -> vec3<f32> {
    return color / (color + 1.0);
}

// ACES Filmic tonemapping
// Based on: https://knarkowicz.wordpress.com/2016/01/06/aces-filmic-tone-mapping-curve/
fn tonemap_aces_filmic(color: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp(
        (color * (a * color + b)) / (color * (c * color + d) + e),
        vec3<f32>(0.0),
        vec3<f32>(1.0),
    );
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let hdr = textureSample(t_hdr, s_hdr, in.uv).rgb;
    
    // Apply exposure (could be passed as uniform)
    let exposure = 1.0;
    let exposed = hdr * exposure;
    
    // Apply ACES filmic tonemapping
    let ldr = tonemap_aces_filmic(exposed);
    
    // Apply gamma correction (sRGB output)
    let gamma = 1.0 / 2.2;
    let corrected = pow(ldr, vec3<f32>(gamma));
    
    return vec4<f32>(corrected, 1.0);
}
