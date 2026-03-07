// Hi-Z depth pyramid build — downsample one mip level per dispatch.
//
// Reads parent mip as texture, writes child mip as storage.
// Each thread processes one output texel from a 2×2 region of the parent.
// The max (farthest) depth is stored — this is the conservative choice for
// standard [0, 1] Z where 0 = near and 1 = far.

struct HizParams {
    // .x = parent mip width, .y = parent mip height,
    // .z = output mip width,  .w = output mip height.
    dims: vec4<u32>,
};

@group(0) @binding(0) var<uniform> params: HizParams;
@group(0) @binding(1) var t_src: texture_2d<f32>;
@group(0) @binding(2) var t_dst: texture_storage_2d<r32float, write>;

@compute @workgroup_size(8, 8)
fn hiz_downsample(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dst_x = gid.x;
    let dst_y = gid.y;
    let out_w = params.dims.z;
    let out_h = params.dims.w;

    if dst_x >= out_w || dst_y >= out_h {
        return;
    }

    let src_w = params.dims.x;
    let src_h = params.dims.y;

    let sx = dst_x * 2u;
    let sy = dst_y * 2u;

    // Gather the 2×2 block from the parent mip (clamped).
    let s00 = textureLoad(t_src, vec2<u32>(sx, sy), 0).r;
    let s10 = textureLoad(t_src, vec2<u32>(min(sx + 1u, src_w - 1u), sy), 0).r;
    let s01 = textureLoad(t_src, vec2<u32>(sx, min(sy + 1u, src_h - 1u)), 0).r;
    let s11 = textureLoad(t_src, vec2<u32>(min(sx + 1u, src_w - 1u), min(sy + 1u, src_h - 1u)), 0).r;

    // Max = farthest depth (conservative for standard Z: 0 near, 1 far).
    let d = max(max(s00, s10), max(s01, s11));

    textureStore(t_dst, vec2<u32>(dst_x, dst_y), vec4<f32>(d, 0.0, 0.0, 1.0));
}
