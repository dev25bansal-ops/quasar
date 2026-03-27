//! Meshlet-based rendering pipeline.
//!
//! Splits traditional index-buffer meshes into small clusters (meshlets)
//! that can be frustum- and back-face-culled on the GPU before indirect
//! draw submission.

use bytemuck::{Pod, Zeroable};

/// Maximum number of vertices per meshlet (must be ≤ 256 so indices fit in `u8`).
pub const MAX_MESHLET_VERTICES: usize = 64;
/// Maximum number of triangles per meshlet.
pub const MAX_MESHLET_TRIANGLES: usize = 126;
/// Maximum meshlets processed at once by the culling compute shader.
pub const MAX_MESHLETS: u32 = 65536;

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

// ── GPU buffers ──────────────────────────────────────────────────────

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
}
