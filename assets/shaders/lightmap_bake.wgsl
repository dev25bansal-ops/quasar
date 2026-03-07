// GPU lightmap bake — direct light + shadow ray per texel.
//
// Dispatched with workgroup_size(8, 8, 1).
// Each thread owns one lightmap texel: find the triangle via UV barycentric
// lookup, compute direct light with a shadow ray, write the result.

struct BakeUniforms {
    width: u32,
    height: u32,
    ao_rays: u32,
    triangle_count: u32,
    light_dir_ao_dist: vec4<f32>,
    light_color_ao_str: vec4<f32>,
};

struct Triangle {
    p0: vec4<f32>, p1: vec4<f32>, p2: vec4<f32>,
    n0: vec4<f32>, n1: vec4<f32>, n2: vec4<f32>,
    uv01: vec4<f32>,
    uv2_pad: vec4<f32>,
};

@group(0) @binding(0) var<uniform> cfg: BakeUniforms;
@group(0) @binding(1) var<storage, read> tris: array<Triangle>;
@group(0) @binding(2) var output: texture_storage_2d<rgba16float, write>;

fn barycentric(tri_uv0: vec2<f32>, tri_uv1: vec2<f32>, tri_uv2: vec2<f32>, p: vec2<f32>) -> vec3<f32> {
    let v0 = tri_uv1 - tri_uv0;
    let v1 = tri_uv2 - tri_uv0;
    let v2 = p - tri_uv0;
    let d00 = dot(v0, v0);
    let d01 = dot(v0, v1);
    let d11 = dot(v1, v1);
    let d20 = dot(v2, v0);
    let d21 = dot(v2, v1);
    let denom = d00 * d11 - d01 * d01;
    if abs(denom) < 1e-10 { return vec3(-1.0); }
    let bv = (d11 * d20 - d01 * d21) / denom;
    let bw = (d00 * d21 - d01 * d20) / denom;
    let bu = 1.0 - bv - bw;
    return vec3(bu, bv, bw);
}

fn ray_tri_intersect(ro: vec3<f32>, rd: vec3<f32>, p0: vec3<f32>, p1: vec3<f32>, p2: vec3<f32>) -> f32 {
    let e1 = p1 - p0;
    let e2 = p2 - p0;
    let h = cross(rd, e2);
    let a = dot(e1, h);
    if abs(a) < 1e-8 { return -1.0; }
    let f = 1.0 / a;
    let s = ro - p0;
    let u = f * dot(s, h);
    if u < 0.0 || u > 1.0 { return -1.0; }
    let q = cross(s, e1);
    let v = f * dot(rd, q);
    if v < 0.0 || u + v > 1.0 { return -1.0; }
    let t = f * dot(e2, q);
    if t > 1e-4 { return t; }
    return -1.0;
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if gid.x >= cfg.width || gid.y >= cfg.height { return; }

    let u = (f32(gid.x) + 0.5) / f32(cfg.width);
    let v = (f32(gid.y) + 0.5) / f32(cfg.height);
    let p = vec2(u, v);

    var world_pos = vec3(0.0);
    var world_normal = vec3(0.0, 1.0, 0.0);
    var found = false;

    for (var i = 0u; i < cfg.triangle_count; i = i + 1u) {
        let tri = tris[i];
        let uv0 = tri.uv01.xy;
        let uv1 = tri.uv01.zw;
        let uv2 = tri.uv2_pad.xy;
        let bary = barycentric(uv0, uv1, uv2, p);
        if bary.x >= 0.0 && bary.y >= 0.0 && bary.z >= 0.0 {
            world_pos = tri.p0.xyz * bary.x + tri.p1.xyz * bary.y + tri.p2.xyz * bary.z;
            world_normal = normalize(tri.n0.xyz * bary.x + tri.n1.xyz * bary.y + tri.n2.xyz * bary.z);
            found = true;
            break;
        }
    }

    if !found {
        textureStore(output, vec2<i32>(gid.xy), vec4(0.0, 0.0, 0.0, 1.0));
        return;
    }

    let light_dir = cfg.light_dir_ao_dist.xyz;
    let n_dot_l = max(dot(world_normal, -light_dir), 0.0);

    // Shadow ray
    let shadow_origin = world_pos + world_normal * 0.001;
    var shadowed = false;
    for (var i = 0u; i < cfg.triangle_count; i = i + 1u) {
        let tri = tris[i];
        let t = ray_tri_intersect(shadow_origin, -light_dir, tri.p0.xyz, tri.p1.xyz, tri.p2.xyz);
        if t > 0.0 { shadowed = true; break; }
    }

    var direct = vec3(0.0);
    if !shadowed {
        direct = cfg.light_color_ao_str.xyz * n_dot_l;
    }

    let ao_strength = cfg.light_color_ao_str.w;
    let color = direct * (1.0 - ao_strength * 0.5);

    textureStore(output, vec2<i32>(gid.xy), vec4(color, 1.0));
}
