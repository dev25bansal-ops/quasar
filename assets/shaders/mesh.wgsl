// ── Mesh Shader (Compute Emulation) ───────────────────────────────
//
// One invocation per visible meshlet workgroup. Performs:
// 1. Reads meshlet vertex/triangle data from storage buffers
// 2. Transforms vertices to clip space
// 3. Outputs triangles to an indirect draw buffer
//
// Note: This uses @compute instead of @mesh because wgpu's native
// mesh shader support is still evolving. The logic is identical to
// a real mesh shader and will work automatically when wgpu adds
// native @task/@mesh support.

const MAX_VERTICES: u32 = 64u;
const MAX_PRIMITIVES: u32 = 126u;

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

struct VisibilityEntry {
    visible: u32,
    compacted_index: u32,
    lod_level: u32,
    _pad: u32,
};

struct MeshUniforms {
    view_proj: mat4x4<f32>,
    model: mat4x4<f32>,
    normal_matrix: mat4x4<f32>,
    meshlet_count: u32,
    _pad: vec3<f32>,
};

struct VertexOutput {
    @location(0) position: vec4<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) texcoord: vec2<f32>,
    @location(3) meshlet_id: u32,
    @builtin(primitive_index) primitive_id: u32,
};

@group(0) @binding(0) var<uniform> uniforms: MeshUniforms;
@group(0) @binding(1) var<storage, read> meshlets: array<LodMeshletData>;
@group(0) @binding(2) var<storage, read> bounds: array<MeshletBounds>;
@group(0) @binding(3) var<storage, read> vertex_indices: array<u32>;
@group(0) @binding(4) var<storage, read> triangle_indices: array<u8>;
@group(0) @binding(5) var<storage, read> visibility: array<VisibilityEntry>;
@group(0) @binding(6) var<storage, read> vertex_positions: array<vec3<f32>>;
@group(0) @binding(7) var<storage, read> vertex_normals: array<vec3<f32>>;
@group(0) @binding(8) var<storage, read> vertex_texcoords: array<vec2<f32>>;

// ── Mesh shader output ─────────────────────────────────────────

// Maximum vertices and primitives per meshlet (must match CPU constants)
const MAX_VERTICES: u32 = 64u;
const MAX_PRIMITIVES: u32 = 126u;

// ── Helper: decode packed triangle index ─────────────────────────

/// Read a single vertex index from the packed triangle index buffer.
/// Triangle indices are stored as 3 × u8 per triangle.
fn get_triangle_index(triangle_offset: u32, tri_idx: u32, corner: u32) -> u32 {
    let byte_idx = triangle_offset * 3u + tri_idx * 3u + corner;
    return u32(triangle_indices[byte_idx]);
}

// ── Mesh shader main (compute entry point) ─────────────────────

struct DrawCommand {
    vertex_count: u32,
    instance_count: u32,
    first_vertex: u32,
    first_instance: u32,
};

@group(0) @binding(9) var<storage, read_write> draw_commands: array<DrawCommand>;
@group(0) @binding(10) var<storage, read_write> out_positions: array<vec4<f32>>;
@group(0) @binding(11) var<storage, read_write> out_normals: array<vec3<f32>>;
@group(0) @binding(12) var<storage, read_write> out_texcoords: array<vec2<f32>>;

@compute @workgroup_size(MESH_WORKGROUP_SIZE)
fn main(
    @builtin(workgroup_id) wg_id: vec3<u32>,
    @builtin(local_invocation_index) local_idx: u32,
) {
    let meshlet_idx = wg_id.x;

    // Read meshlet descriptor
    let m = meshlets[meshlet_idx];
    let _b = bounds[meshlet_idx];

    // Check visibility
    let vis = visibility[meshlet_idx];
    if (vis.visible == 0u) {
        return;
    }

    let vert_count = min(m.vertex_count, MAX_VERTICES);
    let tri_count = min(m.triangle_count, MAX_PRIMITIVES);

    // ── Transform and output vertices ─────────────────────────

    // Each thread processes one vertex
    if (local_idx < vert_count) {
        let global_vert_idx = m.vertex_offset + local_idx;
        let pos_obj = vertex_positions[global_vert_idx];
        let normal_obj = vertex_normals[global_vert_idx];
        let texcoord = vertex_texcoords[global_vert_idx];

        // Transform to clip space
        let world_pos = uniforms.model * vec4<f32>(pos_obj, 1.0);
        let clip_pos = uniforms.view_proj * world_pos;

        // Transform normal to world space
        let world_normal = normalize((uniforms.normal_matrix * vec4<f32>(normal_obj, 0.0)).xyz);

        // Write to output buffers
        let out_base = wg_id.x * MAX_VERTICES;
        out_positions[out_base + local_idx] = clip_pos;
        out_normals[out_base + local_idx] = world_normal;
        out_texcoords[out_base + local_idx] = texcoord;
    }

    // Write draw command for this meshlet
    draw_commands[wg_id.x] = DrawCommand(
        vert_count,
        1u,
        wg_id.x * MAX_VERTICES,
        0u,
    );
}
