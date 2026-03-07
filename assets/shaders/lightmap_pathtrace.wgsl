// GPU path-traced lightmap bake — multi-bounce GI per texel.
//
// Dispatched with workgroup_size(8, 8, 1).
// Each thread path-traces from a lightmap texel using simple
// hashing-based random numbers.

struct PathTraceUniforms {
    width: u32,
    height: u32,
    sample_index: u32,
    max_bounces: u32,
    light_dir: vec4<f32>,
    light_color_energy: vec4<f32>,
    triangle_count: u32,
    samples_per_dispatch: u32,
    _pad0: u32,
    _pad1: u32,
};

struct Triangle {
    p0: vec4<f32>, p1: vec4<f32>, p2: vec4<f32>,
    n0: vec4<f32>, n1: vec4<f32>, n2: vec4<f32>,
    uv01: vec4<f32>,
    uv2_pad: vec4<f32>,
};

@group(0) @binding(0) var<uniform> cfg: PathTraceUniforms;
@group(0) @binding(1) var<storage, read> tris: array<Triangle>;
@group(0) @binding(2) var output: texture_storage_2d<rgba32float, read_write>;

fn hash(v: u32) -> u32 {
    var x = v;
    x = x ^ (x >> 16u);
    x = x * 0x45d9f3bu;
    x = x ^ (x >> 16u);
    x = x * 0x45d9f3bu;
    x = x ^ (x >> 16u);
    return x;
}

fn rand_f(seed: ptr<function, u32>) -> f32 {
    *seed = hash(*seed);
    return f32(*seed) / 4294967295.0;
}

fn barycentric_pt(uv0: vec2<f32>, uv1: vec2<f32>, uv2: vec2<f32>, p: vec2<f32>) -> vec3<f32> {
    let v0 = uv1 - uv0;
    let v1 = uv2 - uv0;
    let v2 = p - uv0;
    let d00 = dot(v0, v0);
    let d01 = dot(v0, v1);
    let d11 = dot(v1, v1);
    let d20 = dot(v2, v0);
    let d21 = dot(v2, v1);
    let denom = d00 * d11 - d01 * d01;
    if abs(denom) < 1e-10 { return vec3(-1.0); }
    let bv = (d11 * d20 - d01 * d21) / denom;
    let bw = (d00 * d21 - d01 * d20) / denom;
    return vec3(1.0 - bv - bw, bv, bw);
}

fn ray_tri(ro: vec3<f32>, rd: vec3<f32>, p0: vec3<f32>, p1: vec3<f32>, p2: vec3<f32>) -> f32 {
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

fn trace_scene(ro: vec3<f32>, rd: vec3<f32>) -> vec4<f32> {
    var closest = 1e30;
    var hit_normal = vec3(0.0);
    for (var i = 0u; i < cfg.triangle_count; i = i + 1u) {
        let tri = tris[i];
        let t = ray_tri(ro, rd, tri.p0.xyz, tri.p1.xyz, tri.p2.xyz);
        if t > 0.0 && t < closest {
            closest = t;
            let bary_u = 1.0; // simplified — weight from Moller-Trumbore
            hit_normal = normalize(tri.n0.xyz + tri.n1.xyz + tri.n2.xyz);
        }
    }
    if closest < 1e29 {
        return vec4(hit_normal, closest);
    }
    return vec4(0.0, 0.0, 0.0, -1.0);
}

fn cosine_hemisphere(n: vec3<f32>, seed: ptr<function, u32>) -> vec3<f32> {
    let r1 = rand_f(seed);
    let r2 = rand_f(seed);
    let phi = 6.2831853 * r1;
    let cos_theta = sqrt(1.0 - r2);
    let sin_theta = sqrt(r2);

    var t: vec3<f32>;
    if abs(n.x) > 0.9 {
        t = vec3(0.0, 1.0, 0.0);
    } else {
        t = vec3(1.0, 0.0, 0.0);
    }
    let b1 = normalize(cross(n, t));
    let b2 = cross(n, b1);
    return normalize(b1 * cos(phi) * sin_theta + b2 * sin(phi) * sin_theta + n * cos_theta);
}

@compute @workgroup_size(8, 8, 1)
fn pathtrace_main(@builtin(global_invocation_id) gid: vec3<u32>) {
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
        let bary = barycentric_pt(uv0, uv1, uv2, p);
        if bary.x >= 0.0 && bary.y >= 0.0 && bary.z >= 0.0 {
            world_pos = tri.p0.xyz * bary.x + tri.p1.xyz * bary.y + tri.p2.xyz * bary.z;
            world_normal = normalize(tri.n0.xyz * bary.x + tri.n1.xyz * bary.y + tri.n2.xyz * bary.z);
            found = true;
            break;
        }
    }

    if !found {
        return;
    }

    var seed = hash(gid.x + gid.y * cfg.width + cfg.sample_index * cfg.width * cfg.height);
    var accumulated = vec3(0.0);

    for (var s = 0u; s < cfg.samples_per_dispatch; s = s + 1u) {
        var throughput = vec3(1.0);
        var radiance = vec3(0.0);
        var ray_pos = world_pos + world_normal * 0.001;
        var ray_dir = cosine_hemisphere(world_normal, &seed);
        var cur_normal = world_normal;

        for (var bounce = 0u; bounce < cfg.max_bounces; bounce = bounce + 1u) {
            // Direct light evaluation at the surface
            let light_dir = cfg.light_dir.xyz;
            let n_dot_l = max(dot(cur_normal, -light_dir), 0.0);
            let shadow_origin = ray_pos + cur_normal * 0.001;
            var in_shadow = false;
            for (var j = 0u; j < cfg.triangle_count; j = j + 1u) {
                let tri = tris[j];
                let st = ray_tri(shadow_origin, -light_dir, tri.p0.xyz, tri.p1.xyz, tri.p2.xyz);
                if st > 0.0 { in_shadow = true; break; }
            }
            if !in_shadow {
                radiance = radiance + throughput * cfg.light_color_energy.xyz * cfg.light_color_energy.w * n_dot_l;
            }

            // Trace bounce ray
            let hit = trace_scene(ray_pos, ray_dir);
            if hit.w < 0.0 { break; }

            // Lambertian BRDF attenuation
            throughput = throughput * 0.8;

            // Russian roulette after 2 bounces
            if bounce >= 2u {
                let rr = max(throughput.x, max(throughput.y, throughput.z));
                if rand_f(&seed) > rr { break; }
                throughput = throughput / rr;
            }

            cur_normal = hit.xyz;
            ray_pos = ray_pos + ray_dir * hit.w + cur_normal * 0.001;
            ray_dir = cosine_hemisphere(cur_normal, &seed);
        }

        accumulated = accumulated + radiance;
    }

    let avg = accumulated / f32(cfg.samples_per_dispatch);
    let prev = textureLoad(output, vec2<i32>(gid.xy));
    let blend = f32(cfg.sample_index) / f32(cfg.sample_index + 1u);
    let result = prev.xyz * blend + avg * (1.0 - blend);
    textureStore(output, vec2<i32>(gid.xy), vec4(result, 1.0));
}
