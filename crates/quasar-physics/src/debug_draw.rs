//! Physics debug draw — generates wireframe line segments for colliders,
//! AABBs, joints, and contact points so a renderer can visualise the
//! physics state.

use crate::collider::ColliderShape;

/// A single debug line segment in world space.
#[derive(Debug, Clone, Copy)]
pub struct DebugLine {
    pub start: [f32; 3],
    pub end: [f32; 3],
    pub color: [f32; 4],
}

/// Colours used by the debug draw system.
pub struct DebugDrawColors {
    pub collider: [f32; 4],
    pub aabb: [f32; 4],
    pub joint: [f32; 4],
    pub contact: [f32; 4],
    pub trigger: [f32; 4],
}

impl Default for DebugDrawColors {
    fn default() -> Self {
        Self {
            collider: [0.0, 1.0, 0.0, 1.0], // green
            aabb: [1.0, 1.0, 0.0, 0.5],     // yellow translucent
            joint: [0.0, 0.5, 1.0, 1.0],    // blue
            contact: [1.0, 0.0, 0.0, 1.0],  // red
            trigger: [1.0, 0.0, 1.0, 0.6],  // magenta translucent
        }
    }
}

/// Configuration for physics debug drawing.
pub struct PhysicsDebugDraw {
    pub draw_colliders: bool,
    pub draw_aabbs: bool,
    pub draw_joints: bool,
    pub draw_contacts: bool,
    pub draw_triggers: bool,
    pub colors: DebugDrawColors,
}

impl Default for PhysicsDebugDraw {
    fn default() -> Self {
        Self {
            draw_colliders: true,
            draw_aabbs: false,
            draw_joints: true,
            draw_contacts: true,
            draw_triggers: true,
            colors: DebugDrawColors::default(),
        }
    }
}

impl PhysicsDebugDraw {
    pub fn new() -> Self {
        Self::default()
    }

    /// Generate debug lines for the entire physics world.
    pub fn generate(&self, physics: &crate::world::PhysicsWorld) -> Vec<DebugLine> {
        let mut lines = Vec::new();

        if self.draw_colliders || self.draw_aabbs || self.draw_triggers {
            for (_handle, collider) in physics.colliders.iter() {
                let pos = collider.position();
                let translation = pos.translation;
                let rotation = pos.rotation;

                let is_sensor = collider.is_sensor();
                let color = if is_sensor {
                    self.colors.trigger
                } else {
                    self.colors.collider
                };

                if (is_sensor && self.draw_triggers) || (!is_sensor && self.draw_colliders) {
                    let shape = collider.shape();
                    Self::wireframe_shape(
                        &mut lines,
                        shape,
                        [translation.x, translation.y, translation.z],
                        [rotation.i, rotation.j, rotation.k, rotation.w],
                        color,
                    );
                }

                if self.draw_aabbs {
                    let aabb = collider.compute_aabb();
                    Self::wireframe_aabb(
                        &mut lines,
                        [aabb.mins.x, aabb.mins.y, aabb.mins.z],
                        [aabb.maxs.x, aabb.maxs.y, aabb.maxs.z],
                        self.colors.aabb,
                    );
                }
            }
        }

        if self.draw_joints {
            for (_, joint) in physics.impulse_joints.iter() {
                let anchor1 = joint.data.local_anchor1();
                let anchor2 = joint.data.local_anchor2();
                // Get world positions of the attached bodies.
                if let (Some(b1), Some(b2)) = (
                    physics.bodies.get(joint.body1),
                    physics.bodies.get(joint.body2),
                ) {
                    let p1 = b1.position() * anchor1;
                    let p2 = b2.position() * anchor2;
                    lines.push(DebugLine {
                        start: [p1.x, p1.y, p1.z],
                        end: [p2.x, p2.y, p2.z],
                        color: self.colors.joint,
                    });
                }
            }
        }

        lines
    }

    /// Generate wireframe lines for a Rapier shape.
    fn wireframe_shape(
        lines: &mut Vec<DebugLine>,
        shape: &dyn rapier3d::prelude::Shape,
        pos: [f32; 3],
        rot: [f32; 4],
        color: [f32; 4],
    ) {
        // Cuboid
        if let Some(cuboid) = shape.as_cuboid() {
            let he = cuboid.half_extents;
            Self::wireframe_box(lines, pos, rot, [he.x, he.y, he.z], color);
            return;
        }
        // Ball
        if let Some(ball) = shape.as_ball() {
            Self::wireframe_sphere(lines, pos, ball.radius, color, 16);
            return;
        }
        // Capsule
        if let Some(capsule) = shape.as_capsule() {
            let r = capsule.radius;
            let hh = capsule.half_height();
            // Draw two circle rings and connecting lines.
            Self::wireframe_sphere(lines, [pos[0], pos[1] + hh, pos[2]], r, color, 12);
            Self::wireframe_sphere(lines, [pos[0], pos[1] - hh, pos[2]], r, color, 12);
            // Vertical lines connecting caps.
            for i in 0..4 {
                let angle = (i as f32) * std::f32::consts::FRAC_PI_2;
                let dx = r * angle.cos();
                let dz = r * angle.sin();
                lines.push(DebugLine {
                    start: [pos[0] + dx, pos[1] + hh, pos[2] + dz],
                    end: [pos[0] + dx, pos[1] - hh, pos[2] + dz],
                    color,
                });
            }
            return;
        }
        // Cylinder
        if let Some(cylinder) = shape.as_cylinder() {
            let r = cylinder.radius;
            let hh = cylinder.half_height;
            Self::wireframe_circle_y(lines, [pos[0], pos[1] + hh, pos[2]], r, color, 16);
            Self::wireframe_circle_y(lines, [pos[0], pos[1] - hh, pos[2]], r, color, 16);
            for i in 0..4 {
                let angle = (i as f32) * std::f32::consts::FRAC_PI_2;
                let dx = r * angle.cos();
                let dz = r * angle.sin();
                lines.push(DebugLine {
                    start: [pos[0] + dx, pos[1] + hh, pos[2] + dz],
                    end: [pos[0] + dx, pos[1] - hh, pos[2] + dz],
                    color,
                });
            }
            return;
        }
        // Fallback: draw a small cross at the position.
        let s = 0.1;
        lines.push(DebugLine {
            start: [pos[0] - s, pos[1], pos[2]],
            end: [pos[0] + s, pos[1], pos[2]],
            color,
        });
        lines.push(DebugLine {
            start: [pos[0], pos[1] - s, pos[2]],
            end: [pos[0], pos[1] + s, pos[2]],
            color,
        });
        lines.push(DebugLine {
            start: [pos[0], pos[1], pos[2] - s],
            end: [pos[0], pos[1], pos[2] + s],
            color,
        });
    }

    /// Wireframe axis-aligned bounding box.
    fn wireframe_aabb(lines: &mut Vec<DebugLine>, mins: [f32; 3], maxs: [f32; 3], color: [f32; 4]) {
        let corners = [
            [mins[0], mins[1], mins[2]],
            [maxs[0], mins[1], mins[2]],
            [maxs[0], maxs[1], mins[2]],
            [mins[0], maxs[1], mins[2]],
            [mins[0], mins[1], maxs[2]],
            [maxs[0], mins[1], maxs[2]],
            [maxs[0], maxs[1], maxs[2]],
            [mins[0], maxs[1], maxs[2]],
        ];

        let edges = [
            (0, 1),
            (1, 2),
            (2, 3),
            (3, 0), // front
            (4, 5),
            (5, 6),
            (6, 7),
            (7, 4), // back
            (0, 4),
            (1, 5),
            (2, 6),
            (3, 7), // connecting
        ];

        for (a, b) in edges {
            lines.push(DebugLine {
                start: corners[a],
                end: corners[b],
                color,
            });
        }
    }

    /// Wireframe box with rotation.
    fn wireframe_box(
        lines: &mut Vec<DebugLine>,
        pos: [f32; 3],
        _rot: [f32; 4],
        half_extents: [f32; 3],
        color: [f32; 4],
    ) {
        // Simplified: axis-aligned at position (ignoring rotation for line drawing performance).
        let mins = [
            pos[0] - half_extents[0],
            pos[1] - half_extents[1],
            pos[2] - half_extents[2],
        ];
        let maxs = [
            pos[0] + half_extents[0],
            pos[1] + half_extents[1],
            pos[2] + half_extents[2],
        ];
        Self::wireframe_aabb(lines, mins, maxs, color);
    }

    /// Wireframe sphere approximation (3 orthogonal circles).
    fn wireframe_sphere(
        lines: &mut Vec<DebugLine>,
        center: [f32; 3],
        radius: f32,
        color: [f32; 4],
        segments: u32,
    ) {
        Self::wireframe_circle_y(lines, center, radius, color, segments);
        // XY plane circle.
        let step = std::f32::consts::TAU / segments as f32;
        for i in 0..segments {
            let a0 = i as f32 * step;
            let a1 = (i + 1) as f32 * step;
            lines.push(DebugLine {
                start: [
                    center[0] + radius * a0.cos(),
                    center[1] + radius * a0.sin(),
                    center[2],
                ],
                end: [
                    center[0] + radius * a1.cos(),
                    center[1] + radius * a1.sin(),
                    center[2],
                ],
                color,
            });
        }
        // YZ plane circle.
        for i in 0..segments {
            let a0 = i as f32 * step;
            let a1 = (i + 1) as f32 * step;
            lines.push(DebugLine {
                start: [
                    center[0],
                    center[1] + radius * a0.cos(),
                    center[2] + radius * a0.sin(),
                ],
                end: [
                    center[0],
                    center[1] + radius * a1.cos(),
                    center[2] + radius * a1.sin(),
                ],
                color,
            });
        }
    }

    /// Wireframe circle on the XZ plane at the given Y.
    fn wireframe_circle_y(
        lines: &mut Vec<DebugLine>,
        center: [f32; 3],
        radius: f32,
        color: [f32; 4],
        segments: u32,
    ) {
        let step = std::f32::consts::TAU / segments as f32;
        for i in 0..segments {
            let a0 = i as f32 * step;
            let a1 = (i + 1) as f32 * step;
            lines.push(DebugLine {
                start: [
                    center[0] + radius * a0.cos(),
                    center[1],
                    center[2] + radius * a0.sin(),
                ],
                end: [
                    center[0] + radius * a1.cos(),
                    center[1],
                    center[2] + radius * a1.sin(),
                ],
                color,
            });
        }
    }

    /// Generate wireframe lines for a specific collider shape description.
    pub fn lines_for_shape(
        shape: &ColliderShape,
        position: [f32; 3],
        color: [f32; 4],
    ) -> Vec<DebugLine> {
        let mut lines = Vec::new();
        match shape {
            ColliderShape::Box { half_extents } => {
                Self::wireframe_box(
                    &mut lines,
                    position,
                    [0.0, 0.0, 0.0, 1.0],
                    *half_extents,
                    color,
                );
            }
            ColliderShape::Sphere { radius } => {
                Self::wireframe_sphere(&mut lines, position, *radius, color, 16);
            }
            ColliderShape::Capsule {
                half_height,
                radius,
            } => {
                Self::wireframe_sphere(
                    &mut lines,
                    [position[0], position[1] + half_height, position[2]],
                    *radius,
                    color,
                    12,
                );
                Self::wireframe_sphere(
                    &mut lines,
                    [position[0], position[1] - half_height, position[2]],
                    *radius,
                    color,
                    12,
                );
            }
            ColliderShape::Cylinder {
                half_height,
                radius,
            } => {
                Self::wireframe_circle_y(
                    &mut lines,
                    [position[0], position[1] + half_height, position[2]],
                    *radius,
                    color,
                    16,
                );
                Self::wireframe_circle_y(
                    &mut lines,
                    [position[0], position[1] - half_height, position[2]],
                    *radius,
                    color,
                    16,
                );
            }
            ColliderShape::Cone {
                half_height,
                radius,
            } => {
                Self::wireframe_circle_y(
                    &mut lines,
                    [position[0], position[1] - half_height, position[2]],
                    *radius,
                    color,
                    16,
                );
                // Apex
                let apex = [position[0], position[1] + half_height, position[2]];
                for i in 0..4 {
                    let angle = (i as f32) * std::f32::consts::FRAC_PI_2;
                    let base = [
                        position[0] + radius * angle.cos(),
                        position[1] - half_height,
                        position[2] + radius * angle.sin(),
                    ];
                    lines.push(DebugLine {
                        start: base,
                        end: apex,
                        color,
                    });
                }
            }
            ColliderShape::HalfSpace | ColliderShape::HeightField { .. } => {
                // Don't draw these.
            }
        }
        lines
    }
}
