use crate::mesh::MeshData;
use crate::skinning::MorphTarget;
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
    /// Interpolation mode.
    pub interpolation: GltfInterpolation,
}

/// Which transform property a channel animates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GltfChannelProperty {
    Translation,
    Rotation,
    Scale,
    MorphTargetWeights,
}

/// Interpolation mode for an animation channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GltfInterpolation {
    Step,
    Linear,
    /// Cubic spline — keyframe values include in-tangent, value, out-tangent.
    CubicSpline,
}

/// Typed keyframe output values.
#[derive(Debug, Clone)]
pub enum GltfChannelValues {
    /// Vec3 values (translation or scale).
    Vec3(Vec<[f32; 3]>),
    /// Quaternion values (rotation).
    Quat(Vec<[f32; 4]>),
    /// Morph target weight values — flat array where each keyframe has
    /// `num_morph_targets` consecutive floats.
    Weights(Vec<f32>),
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

/// Load morph targets (blend shapes) from a GLTF/GLB file.
///
/// Returns one `Vec<MorphTarget>` per mesh primitive (matching order of `load_gltf`).
/// If a primitive has no morph targets the vec is empty.
pub fn load_gltf_morph_targets(path: impl AsRef<Path>) -> Result<Vec<Vec<MorphTarget>>, String> {
    let path = path.as_ref();
    let (document, buffers, _images) =
        gltf::import(path).map_err(|e| format!("Failed to import glTF: {e}"))?;

    let mut all_targets = Vec::new();

    for mesh in document.meshes() {
        for primitive in mesh.primitives() {
            let mut targets = Vec::new();
            let reader = primitive.reader(|buf| Some(&buffers[buf.index()]));

            let morph_iter = reader.read_morph_targets();
                for (ti, (positions, normals, tangents)) in morph_iter.enumerate() {
                    let position_deltas: Vec<[f32; 3]> = positions
                        .map(|iter| iter.collect())
                        .unwrap_or_default();
                    let normal_deltas: Vec<[f32; 3]> = normals
                        .map(|iter| iter.collect())
                        .unwrap_or_default();
                    let tangent_deltas: Vec<[f32; 3]> = tangents
                        .map(|iter| iter.collect())
                        .unwrap_or_default();

                    targets.push(MorphTarget {
                        name: format!("morph_{ti}"),
                        position_deltas,
                        normal_deltas,
                        tangent_deltas,
                    });
                }
            all_targets.push(targets);
        }
    }

    Ok(all_targets)
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
            let interp = match sampler.interpolation() {
                gltf::animation::Interpolation::Linear => GltfInterpolation::Linear,
                gltf::animation::Interpolation::Step => GltfInterpolation::Step,
                gltf::animation::Interpolation::CubicSpline => GltfInterpolation::CubicSpline,
            };
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
                    property = GltfChannelProperty::MorphTargetWeights;
                    let raw = read_accessor_f32(&buffers, &output_accessor);
                    values = GltfChannelValues::Weights(raw);
                }
            }

            channels.push(GltfAnimationChannel {
                node_index,
                node_name,
                property,
                timestamps,
                values,
                interpolation: interp,
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

// ---------------------------------------------------------------------------
// Keyframe interpolation helpers
// ---------------------------------------------------------------------------

/// Sample a Vec3 channel at time `t` using the channel's interpolation mode.
pub fn sample_vec3(channel: &GltfAnimationChannel, t: f32) -> [f32; 3] {
    let ts = &channel.timestamps;
    let vals = match &channel.values {
        GltfChannelValues::Vec3(v) => v,
        _ => return [0.0; 3],
    };
    if ts.is_empty() || vals.is_empty() {
        return [0.0; 3];
    }
    if t <= ts[0] {
        return if channel.interpolation == GltfInterpolation::CubicSpline {
            vals[1] // skip in-tangent
        } else {
            vals[0]
        };
    }
    let n = ts.len();
    if t >= ts[n - 1] {
        return if channel.interpolation == GltfInterpolation::CubicSpline {
            vals[(n - 1) * 3 + 1]
        } else {
            vals[n - 1]
        };
    }
    // Find segment.
    let i = ts.partition_point(|&v| v < t).saturating_sub(1).min(n - 2);
    let t0 = ts[i];
    let t1 = ts[i + 1];
    let dt = (t1 - t0).max(1e-10);
    let u = ((t - t0) / dt).clamp(0.0, 1.0);
    match channel.interpolation {
        GltfInterpolation::Step => {
            vals[i]
        }
        GltfInterpolation::Linear => {
            lerp3(vals[i], vals[i + 1], u)
        }
        GltfInterpolation::CubicSpline => {
            // GLTF cubic spline: each keyframe stores [in_tangent, value, out_tangent].
            let v0 = vals[i * 3 + 1];
            let b0 = vals[i * 3 + 2]; // out-tangent
            let a1 = vals[(i + 1) * 3];   // in-tangent
            let v1 = vals[(i + 1) * 3 + 1];
            hermite3(v0, b0, a1, v1, u, dt)
        }
    }
}

/// Sample a Quat channel at time `t` using the channel's interpolation mode.
pub fn sample_quat(channel: &GltfAnimationChannel, t: f32) -> [f32; 4] {
    let ts = &channel.timestamps;
    let vals = match &channel.values {
        GltfChannelValues::Quat(v) => v,
        _ => return [0.0, 0.0, 0.0, 1.0],
    };
    if ts.is_empty() || vals.is_empty() {
        return [0.0, 0.0, 0.0, 1.0];
    }
    if t <= ts[0] {
        return if channel.interpolation == GltfInterpolation::CubicSpline {
            vals[1]
        } else {
            vals[0]
        };
    }
    let n = ts.len();
    if t >= ts[n - 1] {
        return if channel.interpolation == GltfInterpolation::CubicSpline {
            normalize_quat(vals[(n - 1) * 3 + 1])
        } else {
            vals[n - 1]
        };
    }
    let i = ts.partition_point(|&v| v < t).saturating_sub(1).min(n - 2);
    let t0 = ts[i];
    let t1 = ts[i + 1];
    let dt = (t1 - t0).max(1e-10);
    let u = ((t - t0) / dt).clamp(0.0, 1.0);
    match channel.interpolation {
        GltfInterpolation::Step => vals[i],
        GltfInterpolation::Linear => slerp(vals[i], vals[i + 1], u),
        GltfInterpolation::CubicSpline => {
            let v0 = vals[i * 3 + 1];
            let b0 = vals[i * 3 + 2];
            let a1 = vals[(i + 1) * 3];
            let v1 = vals[(i + 1) * 3 + 1];
            normalize_quat(hermite4(v0, b0, a1, v1, u, dt))
        }
    }
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// Cubic Hermite interpolation for Vec3.  
/// p0/p1 = values, m0/m1 = tangents, t = [0,1], dt = segment duration.
fn hermite3(p0: [f32; 3], m0: [f32; 3], m1: [f32; 3], p1: [f32; 3], t: f32, dt: f32) -> [f32; 3] {
    let t2 = t * t;
    let t3 = t2 * t;
    let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
    let h10 = (t3 - 2.0 * t2 + t) * dt;
    let h01 = -2.0 * t3 + 3.0 * t2;
    let h11 = (t3 - t2) * dt;
    [
        h00 * p0[0] + h10 * m0[0] + h01 * p1[0] + h11 * m1[0],
        h00 * p0[1] + h10 * m0[1] + h01 * p1[1] + h11 * m1[1],
        h00 * p0[2] + h10 * m0[2] + h01 * p1[2] + h11 * m1[2],
    ]
}

/// Cubic Hermite interpolation for 4-component (quaternion tangent space).
fn hermite4(p0: [f32; 4], m0: [f32; 4], m1: [f32; 4], p1: [f32; 4], t: f32, dt: f32) -> [f32; 4] {
    let t2 = t * t;
    let t3 = t2 * t;
    let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
    let h10 = (t3 - 2.0 * t2 + t) * dt;
    let h01 = -2.0 * t3 + 3.0 * t2;
    let h11 = (t3 - t2) * dt;
    [
        h00 * p0[0] + h10 * m0[0] + h01 * p1[0] + h11 * m1[0],
        h00 * p0[1] + h10 * m0[1] + h01 * p1[1] + h11 * m1[1],
        h00 * p0[2] + h10 * m0[2] + h01 * p1[2] + h11 * m1[2],
        h00 * p0[3] + h10 * m0[3] + h01 * p1[3] + h11 * m1[3],
    ]
}

fn slerp(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    let mut dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
    let mut b = b;
    if dot < 0.0 {
        dot = -dot;
        b = [-b[0], -b[1], -b[2], -b[3]];
    }
    if dot > 0.9995 {
        return normalize_quat(lerp4(a, b, t));
    }
    let theta = dot.clamp(-1.0, 1.0).acos();
    let sin_theta = theta.sin();
    let wa = ((1.0 - t) * theta).sin() / sin_theta;
    let wb = (t * theta).sin() / sin_theta;
    [a[0] * wa + b[0] * wb, a[1] * wa + b[1] * wb, a[2] * wa + b[2] * wb, a[3] * wa + b[3] * wb]
}

fn lerp4(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t, a[3] + (b[3] - a[3]) * t]
}

fn normalize_quat(q: [f32; 4]) -> [f32; 4] {
    let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if len < 1e-10 { return [0.0, 0.0, 0.0, 1.0]; }
    [q[0] / len, q[1] / len, q[2] / len, q[3] / len]
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
