// Meshlet occlusion culling compute shader.
//
// Performs frustum + Hi-Z occlusion culling for each meshlet,
// writing visible meshlets to indirect draw buffer.

struct MeshletBounds {
    center: vec3<f32>,
    radius: f32,
    normal: vec3<f32>,
    cone_cutoff: f32,
    lod_bounds: vec2<f32>,  // min/max screen size for LOD
    _pad: vec2<f32>,
};

struct CameraData {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    view_position: vec3<f32>,
    near_far: vec2<f32>,
    jitter: vec2<f32>,
};

struct DrawCommand {
    index_count: u32,
    instance_count: u32,
    first_index: u32,
    base_vertex: u32,
    first_instance: u32,
};

@group(0) @binding(0) var<storage, read> meshlets: array<MeshletBounds>;
@group(0) @binding(1) var t_hiz: texture_2d<f32>;
@group(0) @binding(2) var s_hiz: sampler;
@group(0) @binding(3) var<storage, read_write> draw_commands: array<DrawCommand>;
@group(0) @binding(4) var<storage, read_write> draw_counter: atomic<u32>;
@group(0) @binding(5) var<uniform> camera: CameraData;

fn frustum_cull(bounds: MeshletBounds) -> bool {
    let clip = camera.view_proj * vec4<f32>(bounds.center, 1.0);
    let ndc = clip.xyz / clip.w;
    
    // Extended bounds accounting for radius
    let extent = bounds.radius / abs(clip.w);
    
    return ndc.x > -1.0 - extent && ndc.x < 1.0 + extent &&
           ndc.y > -1.0 - extent && ndc.y < 1.0 + extent &&
           ndc.z > 0.0 && ndc.z < 1.0;
}

fn hiz_occlusion_cull(bounds: MeshletBounds) -> bool {
    // Project bounds corners to screen space
    let clip = camera.view_proj * vec4<f32>(bounds.center, 1.0);
    let ndc = clip.xyz / clip.w;
    let uv = ndc.xy * 0.5 + 0.5;
    
    // Sample Hi-Z at appropriate mip level
    let radius_pixels = bounds.radius / clip.w * 512.0;  // Approximate
    let mip_level = max(0.0, floor(log2(radius_pixels)));
    
    let depth_sample = textureSampleLevel(t_hiz, s_hiz, uv, mip_level).r;
    let meshlet_depth = ndc.z;
    
    // Conservative: cull if meshlet is definitely behind Hi-Z
    return meshlet_depth < depth_sample - 0.001;
}

fn cone_cull(bounds: MeshletBounds, view_dir: vec3<f32>) -> bool {
    // Backface culling using normal cone
    let cos_angle = dot(bounds.normal, view_dir);
    return cos_angle > bounds.cone_cutoff;
}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x;
    let total_meshlets = arrayLength(&meshlets);
    
    if (idx >= total_meshlets) {
        return;
    }
    
    let bounds = meshlets[idx];
    
    // Frustum culling
    if (!frustum_cull(bounds)) {
        return;
    }
    
    // Hi-Z occlusion culling
    if (hiz_occlusion_cull(bounds)) {
        return;
    }
    
    // Backface cone culling
    let view_dir = normalize(camera.view_position - bounds.center);
    if (!cone_cull(bounds, view_dir)) {
        return;
    }
    
    // Meshlet is visible - write to draw buffer
    let slot = atomicAdd(&draw_counter, 1u);
    
    if (slot < arrayLength(&draw_commands)) {
        // Write draw command (will be filled with actual meshlet data)
        draw_commands[slot] = DrawCommand(
            64u,    // index_count (meshlet-specific)
            1u,     // instance_count
            idx * 64u,  // first_index
            0u,     // base_vertex
            slot,   // first_instance
        );
    }
}
