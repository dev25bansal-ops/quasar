//! Simple OBJ mesh loader.
//!
//! Parses the most common OBJ subset (v, vn, vt, f) and produces a
//! [`MeshData`](crate::mesh::MeshData) that can be uploaded to the GPU via
//! [`Mesh::from_data`](crate::mesh::Mesh::from_data).
//!
//! # Example
//! ```rust,no_run
//! # use quasar_render::loader::load_obj;
//! let mesh_data = load_obj("assets/models/monkey.obj").unwrap();
//! ```

use std::path::Path;

use crate::mesh::MeshData;
use crate::vertex::Vertex;

/// Load a Wavefront OBJ file and return the first mesh found as [`MeshData`].
///
/// Supports `v`, `vt`, `vn`, and triangulated `f` directives.  Faces with
/// more than 3 vertices are fan-triangulated.
pub fn load_obj(path: impl AsRef<Path>) -> Result<MeshData, String> {
    let contents =
        std::fs::read_to_string(path.as_ref()).map_err(|e| format!("IO error: {e}"))?;

    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut texcoords: Vec<[f32; 2]> = Vec::new();
    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    // Deduplicate identical vertex tuples.
    let mut vertex_map: std::collections::HashMap<(usize, usize, usize), u32> =
        std::collections::HashMap::new();

    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let mut parts = line.split_whitespace();
        match parts.next() {
            Some("v") => {
                let x = parse_f32(parts.next())?;
                let y = parse_f32(parts.next())?;
                let z = parse_f32(parts.next())?;
                positions.push([x, y, z]);
            }
            Some("vn") => {
                let x = parse_f32(parts.next())?;
                let y = parse_f32(parts.next())?;
                let z = parse_f32(parts.next())?;
                normals.push([x, y, z]);
            }
            Some("vt") => {
                let u = parse_f32(parts.next())?;
                let v = parse_f32(parts.next())?;
                texcoords.push([u, v]);
            }
            Some("f") => {
                let face_verts: Vec<_> = parts
                    .map(parse_face_vertex)
                    .collect::<Result<Vec<_>, _>>()?;

                if face_verts.len() < 3 {
                    return Err("Face with fewer than 3 vertices".into());
                }

                // Fan-triangulate (works for convex polygons).
                for i in 1..face_verts.len() - 1 {
                    for &vi in &[face_verts[0], face_verts[i], face_verts[i + 1]] {
                        let key = vi;
                        if let Some(&idx) = vertex_map.get(&key) {
                            indices.push(idx);
                        } else {
                            let pos = positions
                                .get(vi.0)
                                .copied()
                                .unwrap_or([0.0; 3]);
                            let normal = if vi.2 > 0 {
                                normals.get(vi.2 - 1).copied().unwrap_or([0.0, 1.0, 0.0])
                            } else {
                                [0.0, 1.0, 0.0]
                            };
                            let uv = if vi.1 > 0 {
                                texcoords.get(vi.1 - 1).copied().unwrap_or([0.0; 2])
                            } else {
                                [0.0; 2]
                            };

                            let vertex = Vertex {
                                position: pos,
                                normal,
                                uv,
                                color: [1.0, 1.0, 1.0, 1.0],
                            };
                            let idx = vertices.len() as u32;
                            vertices.push(vertex);
                            vertex_map.insert(key, idx);
                            indices.push(idx);
                        }
                    }
                }
            }
            _ => { /* ignore mtllib, usemtl, s, o, g, etc. */ }
        }
    }

    if vertices.is_empty() {
        return Err("OBJ file contains no geometry".into());
    }

    Ok(MeshData { vertices, indices })
}

/// Parse a single `f` token — supports `v`, `v/vt`, `v/vt/vn`, `v//vn`.
/// Returns (position_idx, texcoord_idx, normal_idx), 0-based for position,
/// 1-based (0 = absent) for vt/vn.
fn parse_face_vertex(token: &str) -> Result<(usize, usize, usize), String> {
    let parts: Vec<&str> = token.split('/').collect();
    let v = parts
        .first()
        .and_then(|s| s.parse::<usize>().ok())
        .ok_or_else(|| format!("Bad face index: {token}"))?;
    // OBJ indices are 1-based.
    let vi = v.checked_sub(1).ok_or("Face index 0 is invalid")?;
    let vt = parts
        .get(1)
        .and_then(|s| if s.is_empty() { None } else { s.parse::<usize>().ok() })
        .unwrap_or(0);
    let vn = parts
        .get(2)
        .and_then(|s| if s.is_empty() { None } else { s.parse::<usize>().ok() })
        .unwrap_or(0);
    Ok((vi, vt, vn))
}

fn parse_f32(token: Option<&str>) -> Result<f32, String> {
    token
        .ok_or_else(|| "Missing float value".to_string())?
        .parse::<f32>()
        .map_err(|e| format!("Bad float: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_face_vertex_variants() {
        assert_eq!(parse_face_vertex("1").unwrap(), (0, 0, 0));
        assert_eq!(parse_face_vertex("3/2").unwrap(), (2, 2, 0));
        assert_eq!(parse_face_vertex("3/2/1").unwrap(), (2, 2, 1));
        assert_eq!(parse_face_vertex("3//1").unwrap(), (2, 0, 1));
    }
}
