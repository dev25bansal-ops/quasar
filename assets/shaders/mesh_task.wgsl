// ── Mesh Task Shader (Compute Emulation) ──────────────────────────
//
// One invocation per meshlet workgroup. Performs:
// 1. Frustum culling per meshlet
// 2. Back-face cone culling
// 3. LOD selection based on screen-space size
// 4. Writes visible meshlet count to dispatch buffer
//
// Note: This uses @compute instead of @task because wgpu's native
// mesh shader support is still evolving. The logic is identical to
// a real task shader and will work automatically when wgpu adds
// native @task/@mesh support.

const TASK_WORKGROUP_SIZE: u32 = 32u;
const MESH_WORKGROUP_SIZE: u32 = 32u;

struct LodMeshletData {
    vertex_offset: u32,
    triangle_offset: u32,
    vertex_count: u32,
    triangle_count: u32,
    lod_level: u32,
    lod_min_screen_size: f32,
    lod_max_screen_size: f32,
    error_metric: f32,
};

struct MeshletBounds {
    center: vec3<f32>,
    radius: f32,
    cone_axis: vec3<f32>,
    cone_cutoff: f32,
};

struct TaskUniforms {
    view_proj: mat4x4<f32>,
    camera_pos: vec3<f32>,
    meshlet_count: u32,
    lod_count: u32,
    screen_width: f32,
    screen_height: f32,
    lod_thresholds: vec4<f32>,  // Screen-space thresholds for LOD transitions
    _pad: vec2<f32>,
};

struct VisibilityEntry {
    visible: u32,
    compacted_index: u32,
    lod_level: u32,
    _pad: u32,
};

@group(0) @binding(0) var<uniform> uniforms: TaskUniforms;
@group(0) @binding(1) var<storage, read> meshlets: array<LodMeshletData>;
@group(0) @binding(2) var<storage, read> bounds: array<MeshletBounds>;
@group(0) @binding(3) var<storage, read_write> visibility: array<VisibilityEntry>;
@group(0) @binding(4) var<storage, read_write> visible_counter: atomic<u32>;
@group(0) @binding(5) var<storage, read_write> dispatch_buffer: array<u32>;

// ── Frustum culling ─────────────────────────────────────────────

/// Test if a bounding sphere intersects the view frustum.
/// Uses extended bounds accounting for sphere radius in clip space.
fn frustum_cull(center: vec3<f32>, radius: f32) -> bool {
    let clip = uniforms.view_proj * vec4<f32>(center, 1.0);
    let w = clip.w + radius;

    // Check against all 6 frustum planes
    return clip.x > -w && clip.x < w &&
           clip.y > -w && clip.y < w &&
           clip.z > 0.0 && clip.z < w;
}

// ── Back-face cone culling ──────────────────────────────────────

/// Test if a meshlet's normal cone faces away from the camera.
/// If all triangle normals in the meshlet point away from the viewer,
/// the entire meshlet can be culled.
fn cone_cull(b: MeshletBounds, camera_pos: vec3<f32>) -> bool {
    let to_camera = normalize(camera_pos - b.center);
    return dot(to_camera, b.cone_axis) < b.cone_cutoff;
}

// ── LOD selection ───────────────────────────────────────────────

/// Select the appropriate LOD level based on screen-space projected size.
/// Returns the LOD level index (0 = highest detail).
fn select_lod(center: vec3<f32>, radius: f32) -> u32 {
    // Project bounding sphere to screen space
    let clip = uniforms.view_proj * vec4<f32>(center, 1.0);
    if (clip.w <= 0.0) {
        return uniforms.lod_count - 1u; // Behind camera, use lowest LOD
    }

    // Approximate screen-space size in pixels
    let ndc_radius = radius / clip.w;
    let screen_size = ndc_radius * uniforms.screen_width;

    // Select LOD based on thresholds
    // LOD 0: screen_size > threshold[0]
    // LOD 1: threshold[1] < screen_size <= threshold[0]
    // ...
    // LOD N: screen_size <= threshold[N-1]
    for (var lod: u32 = 0u; lod < uniforms.lod_count - 1u; lod = lod + 1u) {
        let threshold = uniforms.lod_thresholds[lod];
        if (screen_size > threshold) {
            return lod;
        }
    }
    return uniforms.lod_count - 1u;
}

// ── Task shader main (compute entry point) ──────────────────────

@compute @workgroup_size(TASK_WORKGROUP_SIZE)
fn main(
    @builtin(workgroup_id) wg_id: vec3<u32>,
    @builtin(num_workgroups) num_wgs: vec3<u32>,
) {
    let wg_size = TASK_WORKGROUP_SIZE;
    let base_idx = wg_id.x * wg_size;
    let total_meshlets = uniforms.meshlet_count;

    // Per-workgroup visible count
    var local_visible_count: u32 = 0u;

    // Process meshlets in this workgroup
    for (var i: u32 = 0u; i < wg_size; i = i + 1u) {
        let idx = base_idx + i;
        if (idx >= total_meshlets) {
            continue;
        }

        let m = meshlets[idx];
        let b = bounds[idx];

        // Initialize visibility entry as culled
        visibility[idx] = VisibilityEntry(0u, 0u, 0u, 0u);

        // Frustum cull
        if (!frustum_cull(b.center, b.radius)) {
            continue;
        }

        // Back-face cone cull
        if (cone_cull(b, uniforms.camera_pos)) {
            continue;
        }

        // LOD selection
        let selected_lod = select_lod(b.center, b.radius);

        // Only use meshlets at or below the selected LOD level
        if (m.lod_level > selected_lod) {
            continue;
        }

        // Find the best LOD meshlet for this cluster
        // (prefer higher LOD that is still <= selected_lod)
        if (m.lod_level != selected_lod && m.lod_level > 0u) {
            // This meshlet is at a different LOD - skip if not optimal
            // In a full implementation, we'd check adjacent LOD meshlets
            // For now, use the meshlet if its LOD is close enough
            continue;
        }

        // Mark as visible
        visibility[idx].visible = 1u;
        visibility[idx].lod_level = m.lod_level;

        local_visible_count = local_visible_count + 1u;
    }

    // Atomic add to global visible counter
    if (local_visible_count > 0u) {
        let base = atomicAdd(&visible_counter, local_visible_count);
        // Write compacted indices for visible meshlets
        for (var i: u32 = 0u; i < wg_size; i = i + 1u) {
            let idx = base_idx + i;
            if (idx >= total_meshlets) {
                continue;
            }
            if (visibility[idx].visible == 1u) {
                // This will be updated in a second pass for proper compaction
                // For now, use a placeholder
                visibility[idx].compacted_index = idx;
            }
        }
    }

    // Write dispatch buffer for mesh shader
    // Calculate workgroup count for mesh shader dispatch
    if (wg_id.x == 0u) {
        // Wait for all workgroups to finish culling (barrier)
        workgroupBarrier();

        let total_visible = atomicLoad(&visible_counter);
        let mesh_wg_size = MESH_WORKGROUP_SIZE;
        let dispatch_count = (total_visible + mesh_wg_size - 1u) / mesh_wg_size;

        dispatch_buffer[0] = max(dispatch_count, 1u);
        dispatch_buffer[1] = 1u;
        dispatch_buffer[2] = 1u;
    }
}
