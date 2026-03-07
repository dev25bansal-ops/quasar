use crate::mesh::MeshData;
use crate::vertex::Vertex;
use std::path::Path;

/// A single animation channel imported from GLTF, targeting one node.
#[derive(Debug, Clone)]
pub struct GltfAnimationChannel {
    /// Index of the target node in the GLTF scene.
    pub node_index: usize,
    /// Name of the target node (if present).
    pub node_name: Option<String>,
    /// The property being animated.
    pub property: GltfChannelProperty,
    /// Timestamps (seconds) for each keyframe.
    pub timestamps: Vec<f32>,
    /// Values for each keyframe — interpretation depends on `property`.
    pub values: GltfChannelValues,
}

/// Which transform property a channel animates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GltfChannelProperty {
    Translation,
    Rotation,
    Scale,
}

/// Typed keyframe output values.
#[derive(Debug, Clone)]
pub enum GltfChannelValues {
    /// Vec3 values (translation or scale).
    Vec3(Vec<[f32; 3]>),
    /// Quaternion values (rotation).
    Quat(Vec<[f32; 4]>),
}

/// A complete animation clip imported from GLTF.
#[derive(Debug, Clone)]
pub struct GltfAnimationClip {
    pub name: String,
    pub duration: f32,
    pub channels: Vec<GltfAnimationChannel>,
}

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

/// Load animation clips from a GLTF/GLB file.
///
/// Returns one [`GltfAnimationClip`] per animation defined in the file.
/// Each clip contains per-node channels with translation/rotation/scale keyframes.
pub fn load_gltf_animations(path: impl AsRef<Path>) -> Result<Vec<GltfAnimationClip>, String> {
    let path = path.as_ref();
    let (document, buffers, _images) =
        gltf::import(path).map_err(|e| format!("Failed to import glTF: {e}"))?;

    // Build node index → name map.
    let node_names: Vec<Option<String>> = document
        .nodes()
        .map(|n| n.name().map(|s| s.to_string()))
        .collect();

    let mut clips = Vec::new();

    for anim in document.animations() {
        let name = anim
            .name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("animation_{}", anim.index()));

        let mut channels = Vec::new();
        let mut duration: f32 = 0.0;

        for channel in anim.channels() {
            let target = channel.target();
            let node_index = target.node().index();
            let node_name = node_names.get(node_index).cloned().flatten();

            let sampler = channel.sampler();
            let input_accessor = sampler.input();
            let output_accessor = sampler.output();

            // Read timestamps.
            let timestamps = read_accessor_f32(&buffers, &input_accessor);

            if let Some(&last_t) = timestamps.last() {
                duration = duration.max(last_t);
            }

            let property;
            let values;

            match target.property() {
                gltf::animation::Property::Translation => {
                    property = GltfChannelProperty::Translation;
                    let raw = read_accessor_f32(&buffers, &output_accessor);
                    let v3: Vec<[f32; 3]> = raw.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
                    values = GltfChannelValues::Vec3(v3);
                }
                gltf::animation::Property::Rotation => {
                    property = GltfChannelProperty::Rotation;
                    let raw = read_accessor_f32(&buffers, &output_accessor);
                    let q: Vec<[f32; 4]> = raw.chunks_exact(4).map(|c| [c[0], c[1], c[2], c[3]]).collect();
                    values = GltfChannelValues::Quat(q);
                }
                gltf::animation::Property::Scale => {
                    property = GltfChannelProperty::Scale;
                    let raw = read_accessor_f32(&buffers, &output_accessor);
                    let v3: Vec<[f32; 3]> = raw.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
                    values = GltfChannelValues::Vec3(v3);
                }
                gltf::animation::Property::MorphTargetWeights => {
                    // Morph target weights not yet supported.
                    continue;
                }
            }

            channels.push(GltfAnimationChannel {
                node_index,
                node_name,
                property,
                timestamps,
                values,
            });
        }

        clips.push(GltfAnimationClip {
            name,
            duration,
            channels,
        });
    }

    Ok(clips)
}

/// Read floats from a GLTF accessor.
fn read_accessor_f32(buffers: &[gltf::buffer::Data], accessor: &gltf::Accessor) -> Vec<f32> {
    let view = match accessor.view() {
        Some(v) => v,
        None => return Vec::new(),
    };
    let buffer = &buffers[view.buffer().index()];
    let stride = view.stride().unwrap_or(0);
    let offset = view.offset() + accessor.offset();
    let count = accessor.count();
    let components = match accessor.dimensions() {
        gltf::accessor::Dimensions::Scalar => 1,
        gltf::accessor::Dimensions::Vec2 => 2,
        gltf::accessor::Dimensions::Vec3 => 3,
        gltf::accessor::Dimensions::Vec4 => 4,
        _ => 1,
    };

    let component_size = 4; // f32
    let element_size = components * component_size;
    let effective_stride = if stride > 0 { stride } else { element_size };

    let mut result = Vec::with_capacity(count * components);
    for i in 0..count {
        let base = offset + i * effective_stride;
        for c in 0..components {
            let start = base + c * component_size;
            if start + 4 <= buffer.len() {
                let bytes = [buffer[start], buffer[start + 1], buffer[start + 2], buffer[start + 3]];
                result.push(f32::from_le_bytes(bytes));
            }
        }
    }
    result
}
