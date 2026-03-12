//! Editor gizmos — translate/rotate/scale handles.
//!
//! Renders 3D overlay handles in the viewport and handles mouse-ray
//! intersection for drag operations.

use quasar_math::{Mat4, Vec2, Vec3};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GizmoMode {
    Translate,
    Rotate,
    Scale,
}

#[derive(Debug, Clone, Copy)]
pub enum GizmoAxis {
    X,
    Y,
    Z,
    XY,
    XZ,
    YZ,
    Free,
}

#[derive(Debug, Clone)]
pub struct GizmoState {
    pub mode: GizmoMode,
    pub active_axis: Option<GizmoAxis>,
    pub is_dragging: bool,
    pub drag_start: Vec3,
    pub drag_current: Vec3,
}

impl Default for GizmoState {
    fn default() -> Self {
        Self {
            mode: GizmoMode::Translate,
            active_axis: None,
            is_dragging: false,
            drag_start: Vec3::ZERO,
            drag_current: Vec3::ZERO,
        }
    }
}

pub struct GizmoRenderer {
    pub translate_mesh: crate::renderer::GizmoMesh,
    pub rotate_mesh: crate::renderer::GizmoMesh,
    pub scale_mesh: crate::renderer::GizmoMesh,
    pub pipeline: wgpu::RenderPipeline,
    pub bind_group_layout: wgpu::BindGroupLayout,
}

impl GizmoRenderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Gizmo Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Gizmo Shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../assets/shaders/gizmo.wgsl").into(),
            ),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Gizmo Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Gizmo Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[crate::renderer::GizmoVertex::buffer_layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let translate_mesh = create_translate_gizmo(device);
        let rotate_mesh = create_rotate_gizmo(device);
        let scale_mesh = create_scale_gizmo(device);

        Self {
            translate_mesh,
            rotate_mesh,
            scale_mesh,
            pipeline,
            bind_group_layout,
        }
    }

    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        camera_bind_group: &wgpu::BindGroup,
        model_bind_group: &wgpu::BindGroup,
        mode: GizmoMode,
    ) {
        let mesh = match mode {
            GizmoMode::Translate => &self.translate_mesh,
            GizmoMode::Rotate => &self.rotate_mesh,
            GizmoMode::Scale => &self.scale_mesh,
        };

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Gizmo Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, camera_bind_group, &[]);
        pass.set_bind_group(1, model_bind_group, &[]);
        pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
        pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..mesh.index_count, 0, 0..1);
    }
}

fn create_translate_gizmo(device: &wgpu::Device) -> crate::renderer::GizmoMesh {
    let vertices = create_axis_arrows();
    let indices: Vec<u32> = (0..vertices.len() as u32).collect();

    let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Translate Gizmo Vertex Buffer"),
        size: (vertices.len() * std::mem::size_of::<crate::renderer::GizmoVertex>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Translate Gizmo Index Buffer"),
        size: (indices.len() * std::mem::size_of::<u32>()) as u64,
        usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    crate::renderer::GizmoMesh {
        vertex_buffer,
        index_buffer,
        index_count: indices.len() as u32,
    }
}

fn create_rotate_gizmo(device: &wgpu::Device) -> crate::renderer::GizmoMesh {
    let vertices = create_rotation_rings();
    let indices: Vec<u32> = (0..vertices.len() as u32).collect();

    let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Rotate Gizmo Vertex Buffer"),
        size: (vertices.len() * std::mem::size_of::<crate::renderer::GizmoVertex>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Rotate Gizmo Index Buffer"),
        size: (indices.len() * std::mem::size_of::<u32>()) as u64,
        usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    crate::renderer::GizmoMesh {
        vertex_buffer,
        index_buffer,
        index_count: indices.len() as u32,
    }
}

fn create_scale_gizmo(device: &wgpu::Device) -> crate::renderer::GizmoMesh {
    let vertices = create_scale_handles();
    let indices: Vec<u32> = (0..vertices.len() as u32).collect();

    let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Scale Gizmo Vertex Buffer"),
        size: (vertices.len() * std::mem::size_of::<crate::renderer::GizmoVertex>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Scale Gizmo Index Buffer"),
        size: (indices.len() * std::mem::size_of::<u32>()) as u64,
        usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    crate::renderer::GizmoMesh {
        vertex_buffer,
        index_buffer,
        index_count: indices.len() as u32,
    }
}

fn create_axis_arrows() -> Vec<crate::renderer::GizmoVertex> {
    let mut vertices = Vec::new();
    let axis_length = 1.0;
    let arrow_size = 0.1;

    // X axis (red)
    let x_color = [1.0, 0.0, 0.0, 1.0];
    vertices.extend(create_arrow_axis(Vec3::X, axis_length, arrow_size, x_color));

    // Y axis (green)
    let y_color = [0.0, 1.0, 0.0, 1.0];
    vertices.extend(create_arrow_axis(Vec3::Y, axis_length, arrow_size, y_color));

    // Z axis (blue)
    let z_color = [0.0, 0.0, 1.0, 1.0];
    vertices.extend(create_arrow_axis(Vec3::Z, axis_length, arrow_size, z_color));

    vertices
}

fn create_arrow_axis(
    axis: Vec3,
    length: f32,
    arrow_size: f32,
    color: [f32; 4],
) -> Vec<crate::renderer::GizmoVertex> {
    use crate::renderer::GizmoVertex;

    let mut vertices = Vec::new();
    let start = Vec3::ZERO;
    let end = axis * length;

    vertices.push(GizmoVertex {
        position: start.to_array(),
        color,
    });
    vertices.push(GizmoVertex {
        position: end.to_array(),
        color,
    });

    let perpendicular = if axis == Vec3::Y { Vec3::X } else { Vec3::Y };

    for i in 0..8 {
        let angle = (i as f32 / 8.0) * std::f32::consts::TAU;
        let offset = perpendicular * angle.cos() * arrow_size
            + axis.cross(perpendicular) * angle.sin() * arrow_size;
        vertices.push(GizmoVertex {
            position: (end + offset).to_array(),
            color,
        });
    }

    vertices
}

fn create_rotation_rings() -> Vec<crate::renderer::GizmoVertex> {
    use crate::renderer::GizmoVertex;

    let mut vertices = Vec::new();
    let radius = 1.0;
    let segments = 32;

    for (axis, color) in [
        (Vec3::X, [1.0, 0.0, 0.0, 1.0]),
        (Vec3::Y, [0.0, 1.0, 0.0, 1.0]),
        (Vec3::Z, [0.0, 0.0, 1.0, 1.0]),
    ] {
        for i in 0..segments {
            let angle1 = (i as f32 / segments as f32) * std::f32::consts::TAU;
            let angle2 = ((i + 1) as f32 / segments as f32) * std::f32::consts::TAU;

            let (p1, p2) = if axis == Vec3::X {
                (
                    Vec3::new(0.0, radius * angle1.cos(), radius * angle1.sin()),
                    Vec3::new(0.0, radius * angle2.cos(), radius * angle2.sin()),
                )
            } else if axis == Vec3::Y {
                (
                    Vec3::new(radius * angle1.cos(), 0.0, radius * angle1.sin()),
                    Vec3::new(radius * angle2.cos(), 0.0, radius * angle2.sin()),
                )
            } else {
                (
                    Vec3::new(radius * angle1.cos(), radius * angle1.sin(), 0.0),
                    Vec3::new(radius * angle2.cos(), radius * angle2.sin(), 0.0),
                )
            };

            vertices.push(GizmoVertex {
                position: p1.to_array(),
                color,
            });
            vertices.push(GizmoVertex {
                position: p2.to_array(),
                color,
            });
        }
    }

    vertices
}

fn create_scale_handles() -> Vec<crate::renderer::GizmoVertex> {
    use crate::renderer::GizmoVertex;

    let mut vertices = Vec::new();
    let length = 1.0;
    let box_size = 0.1;

    for (axis, color) in [
        (Vec3::X, [1.0, 0.0, 0.0, 1.0]),
        (Vec3::Y, [0.0, 1.0, 0.0, 1.0]),
        (Vec3::Z, [0.0, 0.0, 1.0, 1.0]),
    ] {
        let end = axis * length;
        vertices.push(GizmoVertex {
            position: [0.0, 0.0, 0.0],
            color,
        });
        vertices.push(GizmoVertex {
            position: end.to_array(),
            color,
        });

        for dx in [-box_size, box_size] {
            for dy in [-box_size, box_size] {
                for _dz in [-box_size, box_size] {
                    let offset = if axis == Vec3::X {
                        Vec3::new(0.0, dx, dy)
                    } else if axis == Vec3::Y {
                        Vec3::new(dx, 0.0, dy)
                    } else {
                        Vec3::new(dx, dy, 0.0)
                    };
                    vertices.push(GizmoVertex {
                        position: (end + offset).to_array(),
                        color,
                    });
                }
            }
        }
    }

    vertices
}

pub fn raycast_gizmo(
    ray_origin: Vec3,
    ray_direction: Vec3,
    gizmo_position: Vec3,
    mode: GizmoMode,
) -> Option<GizmoAxis> {
    let scale = 0.1;
    let threshold = scale * 2.0;

    let relative_origin = ray_origin - gizmo_position;

    match mode {
        GizmoMode::Translate | GizmoMode::Scale => {
            let t_x = ray_axis_intersection(relative_origin, ray_direction, Vec3::X);
            let t_y = ray_axis_intersection(relative_origin, ray_direction, Vec3::Y);
            let t_z = ray_axis_intersection(relative_origin, ray_direction, Vec3::Z);

            if let Some(t) = t_x {
                if t > 0.0 && t < threshold {
                    return Some(GizmoAxis::X);
                }
            }
            if let Some(t) = t_y {
                if t > 0.0 && t < threshold {
                    return Some(GizmoAxis::Y);
                }
            }
            if let Some(t) = t_z {
                if t > 0.0 && t < threshold {
                    return Some(GizmoAxis::Z);
                }
            }
        }
        GizmoMode::Rotate => {
            let dist_x = (relative_origin - relative_origin.project_onto(Vec3::X)).length();
            let dist_y = (relative_origin - relative_origin.project_onto(Vec3::Y)).length();
            let dist_z = (relative_origin - relative_origin.project_onto(Vec3::Z)).length();

            if dist_x < threshold {
                return Some(GizmoAxis::X);
            }
            if dist_y < threshold {
                return Some(GizmoAxis::Y);
            }
            if dist_z < threshold {
                return Some(GizmoAxis::Z);
            }
        }
    }

    None
}

fn ray_axis_intersection(origin: Vec3, direction: Vec3, axis: Vec3) -> Option<f32> {
    let denom = direction.dot(axis);
    if denom.abs() < 0.0001 {
        return None;
    }

    let t = -origin.dot(axis) / denom;
    Some(t)
}

pub fn calculate_drag_delta(
    mode: GizmoMode,
    axis: GizmoAxis,
    ray_origin: Vec3,
    ray_direction: Vec3,
    gizmo_position: Vec3,
    drag_start: Vec3,
) -> Vec3 {
    let plane_normal = match axis {
        GizmoAxis::X => Vec3::Y,
        GizmoAxis::Y => Vec3::Z,
        GizmoAxis::Z => Vec3::X,
        GizmoAxis::XY => Vec3::Z,
        GizmoAxis::XZ => Vec3::Y,
        GizmoAxis::YZ => Vec3::X,
        GizmoAxis::Free => Vec3::Z,
    };

    let plane_point = gizmo_position;
    let t = plane_point.dot(plane_normal) - ray_origin.dot(plane_normal);
    let t = t / ray_direction.dot(plane_normal);

    if t < 0.0 {
        return Vec3::ZERO;
    }

    let hit_point = ray_origin + ray_direction * t;

    match mode {
        GizmoMode::Translate => {
            let delta = hit_point - drag_start;
            match axis {
                GizmoAxis::X => Vec3::new(delta.x, 0.0, 0.0),
                GizmoAxis::Y => Vec3::new(0.0, delta.y, 0.0),
                GizmoAxis::Z => Vec3::new(0.0, 0.0, delta.z),
                _ => delta,
            }
        }
        GizmoMode::Rotate => Vec3::ZERO,
        GizmoMode::Scale => {
            let delta = hit_point - drag_start;
            let scale_factor = 1.0 + delta.length() * 0.1;
            Vec3::splat(scale_factor)
        }
    }
}

/// Convert mouse pixel coordinates to a world-space ray.
///
/// Returns `(ray_origin, ray_direction)`.
/// `inv_view_proj` is the inverse of `projection * view`.
pub fn mouse_to_ray(mouse: Vec2, screen_size: Vec2, inv_view_proj: Mat4) -> (Vec3, Vec3) {
    // Normalised device coords: -1..1
    let ndc_x = (mouse.x / screen_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (mouse.y / screen_size.y) * 2.0; // flip Y

    let near_ndc = quasar_math::Vec4::new(ndc_x, ndc_y, 0.0, 1.0);
    let far_ndc = quasar_math::Vec4::new(ndc_x, ndc_y, 1.0, 1.0);

    let near_world = inv_view_proj * near_ndc;
    let far_world = inv_view_proj * far_ndc;

    let near_world = near_world.truncate() / near_world.w;
    let far_world = far_world.truncate() / far_world.w;

    let direction = (far_world - near_world).normalize();
    (near_world, direction)
}

/// Axis-aligned bounding box (min / max corners).
#[derive(Debug, Clone, Copy)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

/// Ray–AABB slab test.  Returns `Some(t)` at the nearest hit or `None`.
fn ray_aabb(origin: Vec3, inv_dir: Vec3, aabb: &Aabb) -> Option<f32> {
    let t1 = (aabb.min - origin) * inv_dir;
    let t2 = (aabb.max - origin) * inv_dir;

    let tmin = t1.min(t2);
    let tmax = t1.max(t2);

    let enter = tmin.x.max(tmin.y).max(tmin.z);
    let exit = tmax.x.min(tmax.y).min(tmax.z);

    if enter <= exit && exit >= 0.0 {
        Some(enter.max(0.0))
    } else {
        None
    }
}

/// Pick the closest entity whose AABB is hit by the given ray.
///
/// `entities` is an iterator over (entity-id, world-space AABB) pairs.
/// Returns the picked entity id and the hit distance `t`.
pub fn pick_entity<E: Copy>(
    ray_origin: Vec3,
    ray_direction: Vec3,
    entities: impl IntoIterator<Item = (E, Aabb)>,
) -> Option<(E, f32)> {
    let inv_dir = Vec3::new(
        1.0 / ray_direction.x,
        1.0 / ray_direction.y,
        1.0 / ray_direction.z,
    );

    let mut best: Option<(E, f32)> = None;

    for (id, aabb) in entities {
        if let Some(t) = ray_aabb(ray_origin, inv_dir, &aabb) {
            if best.map_or(true, |(_, bt)| t < bt) {
                best = Some((id, t));
            }
        }
    }

    best
}

/// Convenience: handle a mouse click for entity selection.
///
/// Combines `mouse_to_ray` → gizmo check → entity pick.
/// Returns `PickResult::Gizmo` if a gizmo handle was clicked, or
/// `PickResult::Entity` with the entity id, or `PickResult::None`.
pub enum PickResult<E> {
    Gizmo(GizmoAxis),
    Entity(E, f32),
    None,
}

pub fn handle_click<E: Copy>(
    mouse: Vec2,
    screen_size: Vec2,
    inv_view_proj: Mat4,
    gizmo_position: Vec3,
    gizmo_mode: GizmoMode,
    entities: impl IntoIterator<Item = (E, Aabb)>,
) -> PickResult<E> {
    let (origin, dir) = mouse_to_ray(mouse, screen_size, inv_view_proj);

    // Gizmo handles take priority.
    if let Some(axis) = raycast_gizmo(origin, dir, gizmo_position, gizmo_mode) {
        return PickResult::Gizmo(axis);
    }

    if let Some((entity, t)) = pick_entity(origin, dir, entities) {
        return PickResult::Entity(entity, t);
    }

    PickResult::None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gizmo_mode_default() {
        let state = GizmoState::default();
        assert_eq!(state.mode, GizmoMode::Translate);
        assert!(!state.is_dragging);
    }

    #[test]
    fn raycast_gizmo_miss() {
        let ray_origin = Vec3::new(10.0, 10.0, 5.0);
        let ray_dir = Vec3::new(0.0, 0.0, -1.0);
        let gizmo_pos = Vec3::ZERO;

        let hit = raycast_gizmo(ray_origin, ray_dir, gizmo_pos, GizmoMode::Translate);
        assert!(hit.is_none());
    }

    #[test]
    fn ray_aabb_hit() {
        let aabb = Aabb {
            min: Vec3::new(-1.0, -1.0, -1.0),
            max: Vec3::new(1.0, 1.0, 1.0),
        };
        let origin = Vec3::new(0.0, 0.0, 5.0);
        let dir = Vec3::new(0.0, 0.0, -1.0);
        let result = pick_entity(origin, dir, [(0u32, aabb)]);
        assert!(result.is_some());
        let (id, t) = result.unwrap();
        assert_eq!(id, 0u32);
        assert!((t - 4.0).abs() < 0.001);
    }

    #[test]
    fn ray_aabb_miss() {
        let aabb = Aabb {
            min: Vec3::new(-1.0, -1.0, -1.0),
            max: Vec3::new(1.0, 1.0, 1.0),
        };
        let origin = Vec3::new(5.0, 5.0, 5.0);
        let dir = Vec3::new(0.0, 0.0, -1.0);
        let result = pick_entity(origin, dir, [(0u32, aabb)]);
        assert!(result.is_none());
    }
}
