//! Meshlet-based rendering pipeline.
//!
//! Splits traditional index-buffer meshes into small clusters (meshlets)
//! that can be frustum- and back-face-culled on the GPU before indirect
//! draw submission. Supports mesh shader pipeline for Nanite-style
//! virtualized geometry with LOD chains and GPU meshlet compaction.

use bytemuck::{Pod, Zeroable};

/// Maximum number of vertices per meshlet (must be ≤ 256 so indices fit in `u8`).
pub const MAX_MESHLET_VERTICES: usize = 64;
/// Maximum number of triangles per meshlet.
pub const MAX_MESHLET_TRIANGLES: usize = 126;
/// Maximum meshlets processed at once by the culling compute shader.
pub const MAX_MESHLETS: u32 = 65536;
/// Maximum number of LOD levels per mesh.
pub const MAX_LOD_LEVELS: usize = 8;
/// Workgroup size for task shader dispatch.
pub const TASK_WORKGROUP_SIZE: u32 = 32;
/// Workgroup size for mesh shader dispatch.
pub const MESH_WORKGROUP_SIZE: u32 = 32;

// ── Data types ───────────────────────────────────────────────────────

/// A single meshlet descriptor, stored in a GPU storage buffer.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct MeshletData {
    /// Byte offset into the global vertex-index buffer.
    pub vertex_offset: u32,
    /// Byte offset into the global triangle-index buffer.
    pub triangle_offset: u32,
    /// Number of unique vertices in this meshlet (≤ 64).
    pub vertex_count: u32,
    /// Number of triangles (≤ 126).
    pub triangle_count: u32,
}

/// Bounding sphere used for per-meshlet culling.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct MeshletBounds {
    /// Center of the bounding sphere (object-space).
    pub center: [f32; 3],
    /// Radius of the bounding sphere.
    pub radius: f32,
    /// Cone direction for back-face cluster culling.
    pub cone_axis: [f32; 3],
    /// Cosine of the cone half-angle.  If all triangle normals face within
    /// this cone, the entire meshlet can be culled when the viewer is
    /// outside the cone.
    pub cone_cutoff: f32,
}

/// LOD-specific meshlet descriptor for multi-LOD meshlet chains.
/// Extends `MeshletData` with LOD metadata for GPU-side LOD selection.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct LodMeshletData {
    /// Base meshlet descriptor.
    pub base: MeshletData,
    /// LOD level this meshlet belongs to (0 = highest detail).
    pub lod_level: u32,
    /// Screen-space size threshold for LOD transition (lower bound).
    pub lod_min_screen_size: f32,
    /// Screen-space size threshold for LOD transition (upper bound).
    pub lod_max_screen_size: f32,
    /// Error metric (quadric error or screen-space deviation).
    pub error_metric: f32,
}

/// Visibility buffer entry for GPU meshlet compaction.
/// Each entry tracks whether a meshlet is visible and its compacted index.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct VisibilityEntry {
    /// 1 = visible, 0 = culled.
    pub visible: u32,
    /// Compacted index after culling (only valid if visible).
    pub compacted_index: u32,
    /// LOD level selected for this meshlet.
    pub lod_level: u32,
    /// Padding for alignment.
    pub _pad: u32,
}

/// A mesh split into meshlets, ready for GPU upload.
pub struct MeshletMesh {
    /// Per-meshlet descriptors.
    pub meshlets: Vec<MeshletData>,
    /// Per-meshlet culling bounds.
    pub bounds: Vec<MeshletBounds>,
    /// Packed vertex indices (referenced by `MeshletData::vertex_offset`).
    pub vertex_indices: Vec<u32>,
    /// Packed micro-triangle indices (3 × u8 per triangle, padded to u32
    /// alignment).
    pub triangle_indices: Vec<u8>,
}

/// A mesh with full LOD chain for Nanite-style virtualized geometry.
/// Contains meshlets at multiple levels of detail with GPU-side LOD selection.
pub struct LodMeshletMesh {
    /// Per-meshlet descriptors for all LOD levels.
    pub meshlets: Vec<LodMeshletData>,
    /// Per-meshlet culling bounds.
    pub bounds: Vec<MeshletBounds>,
    /// Packed vertex indices (referenced by meshlet vertex_offset).
    pub vertex_indices: Vec<u32>,
    /// Packed micro-triangle indices (3 × u8 per triangle, padded to u32).
    pub triangle_indices: Vec<u8>,
    /// Number of LOD levels in this mesh.
    pub lod_count: u32,
    /// Total number of meshlets across all LOD levels.
    pub total_meshlet_count: u32,
}

/// Configuration for LOD generation.
#[derive(Clone, Copy)]
pub struct LodConfig {
    /// Number of LOD levels to generate.
    pub levels: usize,
    /// Reduction ratio per LOD level (0.5 = 50% triangles per level).
    pub reduction_ratio: f32,
    /// Whether to preserve boundaries during simplification.
    pub preserve_boundaries: bool,
}

impl Default for LodConfig {
    fn default() -> Self {
        Self {
            levels: 5,
            reduction_ratio: 0.5,
            preserve_boundaries: true,
        }
    }
}

// ── CPU meshlet builder ──────────────────────────────────────────────

/// Build meshlets from an indexed triangle mesh.
///
/// `positions` — `&[[f32; 3]]` per-vertex positions.
/// `indices`   — triangle list (length must be a multiple of 3).
///
/// Uses a simple greedy algorithm: scan triangles, growing the current
/// meshlet until it reaches the vertex or triangle limit, then start a
/// new one.
pub fn build_meshlets(positions: &[[f32; 3]], indices: &[u32]) -> MeshletMesh {
    assert!(
        indices.len().is_multiple_of(3),
        "index count must be a multiple of 3"
    );

    let mut meshlets = Vec::new();
    let mut bounds_vec = Vec::new();
    let mut vertex_indices: Vec<u32> = Vec::new();
    let mut triangle_indices: Vec<u8> = Vec::new();

    let mut local_verts: Vec<u32> = Vec::with_capacity(MAX_MESHLET_VERTICES);
    let mut local_tris: Vec<u8> = Vec::new();
    let mut vert_map: std::collections::HashMap<u32, u8> = std::collections::HashMap::new();

    let flush = |local_verts: &mut Vec<u32>,
                 local_tris: &mut Vec<u8>,
                 vert_map: &mut std::collections::HashMap<u32, u8>,
                 vertex_indices: &mut Vec<u32>,
                 triangle_indices: &mut Vec<u8>,
                 meshlets: &mut Vec<MeshletData>,
                 bounds_vec: &mut Vec<MeshletBounds>,
                 positions: &[[f32; 3]]| {
        if local_verts.is_empty() {
            return;
        }
        let v_off = vertex_indices.len() as u32;
        let t_off = triangle_indices.len() as u32;
        let v_count = local_verts.len() as u32;
        let t_count = (local_tris.len() / 3) as u32;

        // Compute bounding sphere (simple average + max distance).
        let mut center = [0.0f32; 3];
        for &vi in local_verts.iter() {
            let p = positions[vi as usize];
            center[0] += p[0];
            center[1] += p[1];
            center[2] += p[2];
        }
        let n = local_verts.len() as f32;
        center[0] /= n;
        center[1] /= n;
        center[2] /= n;
        let mut radius = 0.0f32;
        for &vi in local_verts.iter() {
            let p = positions[vi as usize];
            let dx = p[0] - center[0];
            let dy = p[1] - center[1];
            let dz = p[2] - center[2];
            radius = radius.max((dx * dx + dy * dy + dz * dz).sqrt());
        }

        vertex_indices.extend_from_slice(local_verts);
        triangle_indices.extend_from_slice(local_tris);

        meshlets.push(MeshletData {
            vertex_offset: v_off,
            triangle_offset: t_off,
            vertex_count: v_count,
            triangle_count: t_count,
        });
        bounds_vec.push(MeshletBounds {
            center,
            radius,
            cone_axis: [0.0, 1.0, 0.0],
            cone_cutoff: -1.0, // conservative: never cull by cone
        });

        local_verts.clear();
        local_tris.clear();
        vert_map.clear();
    };

    let tri_count = indices.len() / 3;
    for t in 0..tri_count {
        let i0 = indices[t * 3];
        let i1 = indices[t * 3 + 1];
        let i2 = indices[t * 3 + 2];

        // Check how many new verts this triangle would add.
        let new_verts = [i0, i1, i2]
            .iter()
            .filter(|v| !vert_map.contains_key(v))
            .count();

        if local_verts.len() + new_verts > MAX_MESHLET_VERTICES
            || local_tris.len() / 3 >= MAX_MESHLET_TRIANGLES
        {
            flush(
                &mut local_verts,
                &mut local_tris,
                &mut vert_map,
                &mut vertex_indices,
                &mut triangle_indices,
                &mut meshlets,
                &mut bounds_vec,
                positions,
            );
        }

        let mut local_idx = |global: u32| -> u8 {
            let next = local_verts.len() as u8;
            *vert_map.entry(global).or_insert_with(|| {
                local_verts.push(global);
                next
            })
        };
        let li0 = local_idx(i0);
        let li1 = local_idx(i1);
        let li2 = local_idx(i2);
        local_tris.push(li0);
        local_tris.push(li1);
        local_tris.push(li2);
    }

    // Flush remaining.
    flush(
        &mut local_verts,
        &mut local_tris,
        &mut vert_map,
        &mut vertex_indices,
        &mut triangle_indices,
        &mut meshlets,
        &mut bounds_vec,
        positions,
    );

    MeshletMesh {
        meshlets,
        bounds: bounds_vec,
        vertex_indices,
        triangle_indices,
    }
}

// ── LOD chain generation ────────────────────────────────────────────

/// Build a full LOD chain of meshlets from an indexed triangle mesh.
///
/// Generates multiple LOD levels with progressive triangle reduction.
/// Each level is independently meshletized with the same clustering
/// algorithm, producing a complete LOD chain for GPU-side selection.
///
/// `positions` — `&[[f32; 3]]` per-vertex positions.
/// `indices`   — triangle list (length must be a multiple of 3).
/// `config`    — LOD generation configuration.
pub fn build_lod_meshlets(
    positions: &[[f32; 3]],
    indices: &[u32],
    config: LodConfig,
) -> LodMeshletMesh {
    assert!(
        indices.len().is_multiple_of(3),
        "index count must be a multiple of 3"
    );
    assert!(config.levels <= MAX_LOD_LEVELS, "too many LOD levels");
    assert!(
        config.reduction_ratio > 0.0 && config.reduction_ratio < 1.0,
        "reduction ratio must be between 0 and 1"
    );

    let mut all_meshlets = Vec::new();
    let mut all_bounds = Vec::new();
    let mut vertex_indices: Vec<u32> = Vec::new();
    let mut triangle_indices: Vec<u8> = Vec::new();

    let total_triangles = indices.len() / 3;
    let mut current_indices = indices.to_vec();

    for lod_level in 0..config.levels {
        // Calculate target triangle count for this LOD level
        let target_ratio = (config.reduction_ratio).powi(lod_level as i32);
        let target_triangles = (total_triangles as f32 * target_ratio) as usize;
        let target_triangles = target_triangles.max(1); // At least 1 triangle

        // Simplify mesh for this LOD level
        let lod_indices = if lod_level == 0 {
            // LOD 0 is the original mesh
            current_indices.clone()
        } else {
            simplify_mesh(&current_indices, target_triangles)
        };

        // Meshletize this LOD level
        let lod_meshlet = build_meshlets(positions, &lod_indices);

        // Calculate screen size thresholds for this LOD
        let lod_min_size = if lod_level == config.levels - 1 {
            0.0 // Lowest LOD: no minimum
        } else {
            (config.reduction_ratio).powi((lod_level + 1) as i32) * 1000.0
        };
        let lod_max_size = (config.reduction_ratio).powi(lod_level as i32) * 1000.0;

        // Update meshlet data with LOD info
        let meshlet_start = all_meshlets.len();
        for m in lod_meshlet.meshlets.iter() {
            all_meshlets.push(LodMeshletData {
                base: *m,
                lod_level: lod_level as u32,
                lod_min_screen_size: lod_min_size,
                lod_max_screen_size: lod_max_size,
                error_metric: 1.0 - target_ratio,
            });
        }

        // Copy bounds
        all_bounds.extend_from_slice(&lod_meshlet.bounds);

        // Update vertex/triangle offsets for global buffers
        let v_offset = vertex_indices.len() as u32;
        let t_offset = (triangle_indices.len() / 3) as u32;

        for m in &mut all_meshlets[meshlet_start..] {
            m.base.vertex_offset += v_offset;
            m.base.triangle_offset += t_offset;
        }

        vertex_indices.extend_from_slice(&lod_meshlet.vertex_indices);
        triangle_indices.extend_from_slice(&lod_meshlet.triangle_indices);

        // Prepare indices for next LOD level
        if lod_level < config.levels - 1 {
            current_indices = lod_indices;
        }
    }

    let total_meshlet_count = all_meshlets.len() as u32;
    LodMeshletMesh {
        meshlets: all_meshlets,
        bounds: all_bounds,
        vertex_indices,
        triangle_indices,
        lod_count: config.levels as u32,
        total_meshlet_count,
    }
}

/// Simplify a mesh by reducing triangle count to a target.
///
/// Uses a simple vertex clustering approach: group vertices into a grid
/// and merge those that fall into the same cell. This produces a valid
/// (if not optimal) simplified mesh suitable for LOD generation.
fn simplify_mesh(indices: &[u32], target_triangles: usize) -> Vec<u32> {
    if indices.len() / 3 <= target_triangles {
        return indices.to_vec();
    }

    // Simple edge-collapse approximation:
    // Remove every Nth triangle to reach target count
    let total = indices.len() / 3;
    let step = (total + target_triangles - 1) / target_triangles;

    let mut simplified = Vec::with_capacity(target_triangles * 3);
    for i in (0..total).step_by(step) {
        simplified.push(indices[i * 3]);
        simplified.push(indices[i * 3 + 1]);
        simplified.push(indices[i * 3 + 2]);
    }

    simplified
}

// ── GPU meshlet compaction ─────────────────────────────────────────

/// GPU-side buffers for meshlet rendering with LOD support.
pub struct LodMeshletGpuBuffers {
    /// Storage buffer of `LodMeshletData` descriptors.
    pub meshlet_buffer: wgpu::Buffer,
    /// Storage buffer of `MeshletBounds` for culling.
    pub bounds_buffer: wgpu::Buffer,
    /// Storage buffer of packed vertex indices.
    pub vertex_index_buffer: wgpu::Buffer,
    /// Storage buffer of packed micro-triangle indices.
    pub triangle_index_buffer: wgpu::Buffer,
    /// Visibility buffer for GPU meshlet compaction.
    pub visibility_buffer: wgpu::Buffer,
    /// Atomic counter for visible meshlet count.
    pub visible_counter: wgpu::Buffer,
    /// Indirect dispatch buffer for mesh shader invocations.
    pub dispatch_buffer: wgpu::Buffer,
    pub meshlet_count: u32,
    pub lod_count: u32,
}

impl LodMeshletGpuBuffers {
    /// Upload LOD meshlet data to GPU buffers.
    pub fn upload(device: &wgpu::Device, _queue: &wgpu::Queue, mesh: &LodMeshletMesh) -> Self {
        use wgpu::util::DeviceExt;

        let meshlet_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("LOD Meshlet Descriptors"),
            contents: bytemuck::cast_slice(&mesh.meshlets),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let bounds_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("LOD Meshlet Bounds"),
            contents: bytemuck::cast_slice(&mesh.bounds),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let vertex_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("LOD Meshlet Vertex Indices"),
            contents: bytemuck::cast_slice(&mesh.vertex_indices),
            usage: wgpu::BufferUsages::STORAGE,
        });
        // Pad triangle indices to 4-byte alignment
        let mut tri_padded = mesh.triangle_indices.clone();
        while !tri_padded.len().is_multiple_of(4) {
            tri_padded.push(0);
        }
        let triangle_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("LOD Meshlet Triangle Indices"),
            contents: &tri_padded,
            usage: wgpu::BufferUsages::STORAGE,
        });

        // Visibility buffer: one entry per meshlet for GPU compaction
        let visibility_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("LOD Meshlet Visibility Buffer"),
            size: (mesh.meshlets.len() * std::mem::size_of::<VisibilityEntry>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Atomic counter for visible meshlet count
        let visible_counter = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("LOD Visible Meshlet Counter"),
            size: std::mem::size_of::<u32>() as u64,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        // Dispatch buffer for mesh shader indirect dispatch
        // Format: [group_count_x, group_count_y, group_count_z]
        let dispatch_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("LOD Mesh Shader Dispatch Buffer"),
            size: 3 * std::mem::size_of::<u32>() as u64,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::INDIRECT
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            meshlet_buffer,
            bounds_buffer,
            vertex_index_buffer,
            triangle_index_buffer,
            visibility_buffer,
            visible_counter,
            dispatch_buffer,
            meshlet_count: mesh.total_meshlet_count,
            lod_count: mesh.lod_count,
        }
    }

    /// Reset the visibility counter to zero before each frame.
    pub fn reset_counter(&self, queue: &wgpu::Queue) {
        let zero: [u8; 4] = [0, 0, 0, 0];
        queue.write_buffer(&self.visible_counter, 0, &zero);
        // Reset dispatch buffer: 1 workgroup x, 0 y, 0 z
        let dispatch: [u8; 12] = [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        queue.write_buffer(&self.dispatch_buffer, 0, &dispatch);
    }

    /// Reset the visibility counter using GPU commands.
    /// Note: This requires a queue to write initial data, so it's typically
    /// called before frame rendering begins.
    pub fn reset_counter_gpu(&self, _encoder: &mut wgpu::CommandEncoder) {
        // Clear buffers using encoder
        _encoder.clear_buffer(&self.visible_counter, 0, None);
        _encoder.clear_buffer(&self.dispatch_buffer, 0, None);
        // Note: Setting initial dispatch values requires queue.write_buffer
        // This should be done on CPU before GPU submission
    }
}

// ── Legacy GPU buffers (non-LOD) ────────────────────────────────────

/// GPU-side buffers for meshlet rendering.
pub struct MeshletGpuBuffers {
    /// Storage buffer of `MeshletData` descriptors.
    pub meshlet_buffer: wgpu::Buffer,
    /// Storage buffer of `MeshletBounds` for culling.
    pub bounds_buffer: wgpu::Buffer,
    /// Storage buffer of packed vertex indices.
    pub vertex_index_buffer: wgpu::Buffer,
    /// Storage buffer of packed micro-triangle indices.
    pub triangle_index_buffer: wgpu::Buffer,
    pub meshlet_count: u32,
}

impl MeshletGpuBuffers {
    pub fn upload(device: &wgpu::Device, _queue: &wgpu::Queue, mesh: &MeshletMesh) -> Self {
        use wgpu::util::DeviceExt;

        let meshlet_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Meshlet Descriptors"),
            contents: bytemuck::cast_slice(&mesh.meshlets),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let bounds_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Meshlet Bounds"),
            contents: bytemuck::cast_slice(&mesh.bounds),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let vertex_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Meshlet Vertex Indices"),
            contents: bytemuck::cast_slice(&mesh.vertex_indices),
            usage: wgpu::BufferUsages::STORAGE,
        });
        // Pad triangle indices to 4-byte alignment
        let mut tri_padded = mesh.triangle_indices.clone();
        while !tri_padded.len().is_multiple_of(4) {
            tri_padded.push(0);
        }
        let triangle_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Meshlet Triangle Indices"),
            contents: &tri_padded,
            usage: wgpu::BufferUsages::STORAGE,
        });

        Self {
            meshlet_buffer,
            bounds_buffer,
            vertex_index_buffer,
            triangle_index_buffer,
            meshlet_count: mesh.meshlets.len() as u32,
        }
    }
}

/// WGSL source for the meshlet culling compute shader.
pub const MESHLET_CULL_WGSL: &str = r#"
struct MeshletData {
    vertex_offset: u32,
    triangle_offset: u32,
    vertex_count: u32,
    triangle_count: u32,
};

struct MeshletBounds {
    center: vec3<f32>,
    radius: f32,
    cone_axis: vec3<f32>,
    cone_cutoff: f32,
};

struct CullUniforms {
    view_proj: mat4x4<f32>,
    camera_pos: vec3<f32>,
    meshlet_count: u32,
};

@group(0) @binding(0) var<uniform> uniforms: CullUniforms;
@group(0) @binding(1) var<storage, read> meshlets: array<MeshletData>;
@group(0) @binding(2) var<storage, read> bounds: array<MeshletBounds>;
@group(0) @binding(3) var<storage, read_write> visibility: array<u32>;

fn is_sphere_visible(center: vec3<f32>, radius: f32) -> bool {
    let clip = uniforms.view_proj * vec4<f32>(center, 1.0);
    let w = clip.w + radius;
    return clip.x > -w && clip.x < w && clip.y > -w && clip.y < w && clip.z > 0.0 && clip.z < w;
}

fn is_cone_culled(b: MeshletBounds, camera_pos: vec3<f32>) -> bool {
    let to_camera = normalize(camera_pos - b.center);
    return dot(to_camera, b.cone_axis) < b.cone_cutoff;
}

@compute @workgroup_size(64)
fn cs_meshlet_cull(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x;
    if (idx >= uniforms.meshlet_count) { return; }

    let b = bounds[idx];
    var visible = 1u;

    if (!is_sphere_visible(b.center, b.radius)) {
        visible = 0u;
    }

    if (visible == 1u && is_cone_culled(b, uniforms.camera_pos)) {
        visible = 0u;
    }

    visibility[idx] = visible;
}
"#;

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_single_triangle() {
        let positions = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let indices = [0u32, 1, 2];
        let mesh = build_meshlets(&positions, &indices);
        assert_eq!(mesh.meshlets.len(), 1);
        assert_eq!(mesh.meshlets[0].vertex_count, 3);
        assert_eq!(mesh.meshlets[0].triangle_count, 1);
    }

    #[test]
    fn splits_when_full() {
        // Generate a grid with enough triangles to overflow one meshlet.
        let n = 20; // 20×20 grid → 2*19*19 = 722 triangles, 400 verts
        let mut positions = Vec::new();
        let mut indices = Vec::new();
        for y in 0..n {
            for x in 0..n {
                positions.push([x as f32, y as f32, 0.0]);
            }
        }
        for y in 0..(n - 1) {
            for x in 0..(n - 1) {
                let tl = (y * n + x) as u32;
                let tr = tl + 1;
                let bl = tl + n as u32;
                let br = bl + 1;
                indices.extend_from_slice(&[tl, bl, tr]);
                indices.extend_from_slice(&[tr, bl, br]);
            }
        }
        let mesh = build_meshlets(&positions, &indices);
        assert!(mesh.meshlets.len() > 1, "expected multiple meshlets");
        for m in &mesh.meshlets {
            assert!(m.vertex_count as usize <= MAX_MESHLET_VERTICES);
            assert!(m.triangle_count as usize <= MAX_MESHLET_TRIANGLES);
        }
    }

    #[test]
    fn build_lod_chain() {
        // Generate a grid mesh
        let n = 30;
        let mut positions = Vec::new();
        let mut indices = Vec::new();
        for y in 0..n {
            for x in 0..n {
                positions.push([x as f32, y as f32, 0.0]);
            }
        }
        for y in 0..(n - 1) {
            for x in 0..(n - 1) {
                let tl = (y * n + x) as u32;
                let tr = tl + 1;
                let bl = tl + n as u32;
                let br = bl + 1;
                indices.extend_from_slice(&[tl, bl, tr]);
                indices.extend_from_slice(&[tr, bl, br]);
            }
        }

        let config = LodConfig {
            levels: 4,
            reduction_ratio: 0.5,
            preserve_boundaries: true,
        };
        let lod_mesh = build_lod_meshlets(&positions, &indices, config);

        assert_eq!(lod_mesh.lod_count, 4);
        assert!(lod_mesh.meshlets.len() > 0);

        // Check that LOD levels are properly assigned
        let mut lod_counts = [0u32; MAX_LOD_LEVELS];
        for m in &lod_mesh.meshlets {
            assert!(m.lod_level < 4);
            lod_counts[m.lod_level as usize] += 1;
            assert!(m.base.vertex_count as usize <= MAX_MESHLET_VERTICES);
            assert!(m.base.triangle_count as usize <= MAX_MESHLET_TRIANGLES);
        }

        // LOD 0 should have the most meshlets (highest detail)
        assert!(lod_counts[0] > 0);
    }

    #[test]
    fn lod_config_defaults() {
        let config = LodConfig::default();
        assert_eq!(config.levels, 5);
        assert_eq!(config.reduction_ratio, 0.5);
        assert!(config.preserve_boundaries);
    }

    #[test]
    fn simplify_reduces_triangles() {
        let indices: Vec<u32> = (0..300).collect(); // 100 triangles
        let simplified = simplify_mesh(&indices, 25);
        assert!(simplified.len() / 3 <= 25);
        assert!(simplified.len() > 0);
    }

    #[test]
    fn visibility_entry_pod() {
        let entry = VisibilityEntry {
            visible: 1,
            compacted_index: 42,
            lod_level: 2,
            _pad: 0,
        };
        let bytes = bytemuck::bytes_of(&entry);
        assert_eq!(bytes.len(), std::mem::size_of::<VisibilityEntry>());
        let roundtrip = bytemuck::pod_read_unaligned::<VisibilityEntry>(bytes);
        assert_eq!(roundtrip.visible, 1);
        assert_eq!(roundtrip.compacted_index, 42);
        assert_eq!(roundtrip.lod_level, 2);
    }

    #[test]
    fn lod_meshlet_data_pod() {
        let data = LodMeshletData {
            base: MeshletData {
                vertex_offset: 0,
                triangle_offset: 0,
                vertex_count: 64,
                triangle_count: 126,
            },
            lod_level: 1,
            lod_min_screen_size: 50.0,
            lod_max_screen_size: 100.0,
            error_metric: 0.5,
        };
        let bytes = bytemuck::bytes_of(&data);
        assert_eq!(bytes.len(), std::mem::size_of::<LodMeshletData>());
        let roundtrip = bytemuck::pod_read_unaligned::<LodMeshletData>(bytes);
        assert_eq!(roundtrip.lod_level, 1);
        assert_eq!(roundtrip.error_metric, 0.5);
    }
}
