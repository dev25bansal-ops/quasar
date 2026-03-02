use crate::mesh::MeshData;
use crate::vertex::Vertex;
use std::path::Path;

pub fn load_gltf(path: impl AsRef<Path>) -> Result<Vec<MeshData>, String> {
    let path = path.as_ref();
    let contents = std::fs::read(path).map_err(|e| format!("IO error: {e}"))?;

    let gltf =
        gltf::Gltf::from_slice(&contents).map_err(|e| format!("Failed to parse glTF: {e}"))?;

    let mut meshes = Vec::new();

    for mesh in gltf.meshes() {
        for primitive in mesh.primitives() {
            let reader = primitive.reader(|buffer| {
                let start = buffer.index() as usize;
                let end = start + buffer.length() as usize;
                if end <= contents.len() {
                    Some(&contents[start..end])
                } else {
                    None
                }
            });

            let positions: Vec<[f32; 3]> = reader
                .read_positions()
                .map(|p| p.collect())
                .ok_or_else(|| "No positions found".to_string())?;

            let normals: Vec<[f32; 3]> = reader
                .read_normals()
                .map(|n| n.collect())
                .unwrap_or_else(|| vec![[0.0f32, 1.0, 0.0]; positions.len()]);

            let tex_coords: Vec<[f32; 2]> = reader
                .read_tex_coords(0)
                .map(|uv| uv.into_f32().collect())
                .unwrap_or_else(|| vec![[0.0, 0.0]; positions.len()]);

            let indices: Vec<u32> = reader
                .read_indices()
                .map(|i| i.into_u32().collect())
                .unwrap_or_else(|| (0u32..positions.len() as u32).collect());

            let vertices: Vec<Vertex> = positions
                .iter()
                .zip(normals.iter())
                .zip(tex_coords.iter())
                .map(|((pos, normal), uv)| Vertex {
                    position: *pos,
                    normal: *normal,
                    uv: *uv,
                    color: [1.0, 1.0, 1.0, 1.0],
                })
                .collect();

            meshes.push(MeshData { vertices, indices });
        }
    }

    if meshes.is_empty() {
        return Err("GLTF file contains no meshes".to_string());
    }

    Ok(meshes)
}
