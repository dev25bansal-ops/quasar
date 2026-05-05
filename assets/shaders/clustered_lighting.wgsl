// Clustered Light Assignment Compute Shader
//
// Each thread handles one light, computing its bounding sphere in view space
// and atomically appending the light index to all overlapping clusters.
//
// Workgroup size: 64 threads (covers up to 64 lights per dispatch group).
// For N lights, dispatch ceil(N / 64) workgroups.

// ── Cluster Grid Constants ──────────────────────────────────────────

const CLUSTER_X: u32 = 16u;
const CLUSTER_Y: u32 = 9u;
const CLUSTER_Z: u32 = 24u;
const TOTAL_CLUSTERS: u32 = CLUSTER_X * CLUSTER_Y * CLUSTER_Z; // 3456
const MAX_LIGHTS_PER_CLUSTER: u32 = 128u;
const MAX_LIGHTS: u32 = 256u;

// ── Input Light Structure (matches LightData in Rust) ───────────────

struct LightInput {
    position: vec4<f32>,   // xyz = position, w = 1.0 for point/spot, 0.0 for directional
    color: vec4<f32>,      // rgb = color * intensity, a = intensity
    direction: vec4<f32>,  // xyz = direction (for dir/spot), unused for point
    params: vec4<f32>,     // x = type (0=dir, 1=point, 2=spot), y = range, z/w = spot params
}

// ── Cluster AABB Structure ──────────────────────────────────────────

struct ClusterAabb {
    min: vec3<f32>,
    _pad0: f32,
    max: vec3<f32>,
    _pad1: f32,
}

// ── Cluster Parameters (uniform) ────────────────────────────────────

struct ClusterParams {
    num_lights: u32,
    num_clusters_x: u32,
    num_clusters_y: u32,
    num_clusters_z: u32,
    near: f32,
    far: f32,
    screen_width: f32,
    screen_height: f32,
}

// ── Bindings ────────────────────────────────────────────────────────

@group(0) @binding(0) var<uniform> params: ClusterParams;
@group(0) @binding(1) var<storage, read> lights: array<LightInput>;
@group(0) @binding(2) var<storage, read> cluster_aabbs: array<ClusterAabb>;

@group(0) @binding(3) var<storage, read_write> cluster_counts: array<atomic<u32>>;
@group(0) @binding(4) var<storage, read_write> cluster_lights: array<u32>;

// ── Helper: Sphere vs AABB Intersection ─────────────────────────────

fn sphere_aabb_intersect(center: vec3<f32>, radius: f32, aabb: ClusterAabb) -> bool {
    var dist_sq: f32 = 0.0;

    for (var i: u32 = 0u; i < 3u; i = i + 1u) {
        let v = center[i];
        let clamped = clamp(v, aabb.min[i], aabb.max[i]);
        dist_sq = dist_sq + (v - clamped) * (v - clamped);
    }

    return dist_sq <= radius * radius;
}

// ── Helper: Compute Cluster Index from 3D Coordinates ──────────────

fn compute_cluster_index(cx: u32, cy: u32, cz: u32) -> u32 {
    return cz * CLUSTER_X * CLUSTER_Y + cy * CLUSTER_X + cx;
}

// ── Helper: Compute Z-slice for a given depth ──────────────────────

fn compute_z_slice(depth: f32, near: f32, far: f32) -> u32 {
    let log_ratio = log(far / near);
    let clamped_depth = clamp(depth, near, far);
    let t = log(clamped_depth / near) / log_ratio;
    let slice = u32(t * f32(CLUSTER_Z));
    return min(slice, CLUSTER_Z - 1u);
}

// ── Helper: Compute Z-slice range for a sphere ─────────────────────

fn compute_z_range(center_z: f32, radius: f32, near: f32, far: f32) -> vec2<u32> {
    let z_min = center_z - radius;
    let z_max = center_z + radius;

    let min_slice = compute_z_slice(max(z_min, near), near, far);
    let max_slice = compute_z_slice(min(z_max, far), near, far);

    return vec2<u32>(min_slice, max_slice);
}

// ── Main Compute Entry Point ────────────────────────────────────────

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let light_idx = global_id.x;

    // Guard: skip if beyond number of active lights
    if (light_idx >= params.num_lights) {
        return;
    }

    let light = lights[light_idx];

    // Only process point lights (type == 1.0) and spot lights (type == 2.0)
    // Directional lights affect all clusters and are handled separately.
    let light_type = light.params.x;
    if (light_type < 0.5 || light_type > 2.5) {
        return;
    }

    // Extract view-space position and range
    let view_pos = light.position.xyz;
    let radius = light.params.y; // range

    // Early out: if light is completely outside the frustum depth range
    if (view_pos.z + radius < params.near || view_pos.z - radius > params.far) {
        return;
    }

    // Compute Z-slice range this light overlaps
    let z_range = compute_z_range(view_pos.z, radius, params.near, params.far);
    let z_min = z_range.x;
    let z_max = z_range.y;

    // Iterate over all clusters in Z range (full X/Y coverage for simplicity)
    // A more optimized version could compute X/Y ranges from screen projection.
    for (var z: u32 = z_min; z <= z_max; z = z + 1u) {
        for (var y: u32 = 0u; y < CLUSTER_Y; y = y + 1u) {
            for (var x: u32 = 0u; x < CLUSTER_X; x = x + 1u) {
                let cluster_idx = compute_cluster_index(x, y, z);
                let aabb = cluster_aabbs[cluster_idx];

                // Check if the light's bounding sphere intersects this cluster's AABB
                if (sphere_aabb_intersect(view_pos, radius, aabb)) {
                    // Atomically append light index to cluster's light list
                    let count = atomicAdd(&cluster_counts[cluster_idx], 1u);

                    if (count < MAX_LIGHTS_PER_CLUSTER) {
                        let offset = cluster_idx * MAX_LIGHTS_PER_CLUSTER + count;
                        cluster_lights[offset] = light_idx;
                    } else {
                        // Overflow: decrement count so it stays accurate
                        atomicSub(&cluster_counts[cluster_idx], 1u);
                    }
                }
            }
        }
    }
}
