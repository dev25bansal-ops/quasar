//! Cloth physics simulation using mass-spring model.

use glam::{Vec3, Quat};

/// A particle in the cloth simulation.
#[derive(Debug, Clone, Copy)]
pub struct ClothParticle {
    pub position: Vec3,
    pub prev_position: Vec3,
    pub velocity: Vec3,
    pub mass: f32,
    pub inv_mass: f32,
    pub pinned: bool,
}

impl ClothParticle {
    pub fn new(position: Vec3, mass: f32) -> Self {
        Self {
            position,
            prev_position: position,
            velocity: Vec3::ZERO,
            mass,
            inv_mass: 1.0 / mass,
            pinned: false,
        }
    }

    pub fn pinned(position: Vec3) -> Self {
        Self {
            position,
            prev_position: position,
            velocity: Vec3::ZERO,
            mass: 1.0,
            inv_mass: 0.0,
            pinned: true,
        }
    }
}

/// A distance constraint between two particles.
#[derive(Debug, Clone, Copy)]
pub struct DistanceConstraint {
    pub particle_a: usize,
    pub particle_b: usize,
    pub rest_length: f32,
    pub stiffness: f32,
}

impl DistanceConstraint {
    pub fn new(a: usize, b: usize, rest: f32, stiff: f32) -> Self {
        Self { particle_a: a, particle_b: b, rest_length: rest, stiffness: stiff }
    }
}

/// Configuration for cloth simulation.
#[derive(Debug, Clone)]
pub struct ClothConfig {
    pub gravity: Vec3,
    pub damping: f32,
    pub iterations: usize,
    pub self_collision_radius: f32,
}

impl Default for ClothConfig {
    fn default() -> Self {
        Self {
            gravity: Vec3::new(0.0, -9.81, 0.0),
            damping: 0.98,
            iterations: 8,
            self_collision_radius: 0.05,
        }
    }
}

/// A cloth mesh with particles and constraints.
#[derive(Debug, Clone)]
pub struct ClothMesh {
    pub particles: Vec<ClothParticle>,
    pub constraints: Vec<DistanceConstraint>,
    pub triangles: Vec<[usize; 3]>,
    pub width: usize,
    pub height: usize,
}

impl ClothMesh {
    pub fn new(width: usize, height: usize, cell_size: f32, mass: f32) -> Self {
        let mut particles = Vec::with_capacity(width * height);
        let mut constraints = Vec::new();
        let mut triangles = Vec::new();

        for y in 0..height {
            for x in 0..width {
                let pos = Vec3::new(x as f32 * cell_size, 0.0, y as f32 * cell_size);
                particles.push(ClothParticle::new(pos, mass));
            }
        }

        for y in 0..height {
            for x in 0..width {
                let idx = y * width + x;
                if x + 1 < width {
                    constraints.push(DistanceConstraint::new(idx, idx + 1, cell_size, 1.0));
                }
                if y + 1 < height {
                    constraints.push(DistanceConstraint::new(idx, idx + width, cell_size, 1.0));
                }
                if x + 1 < width && y + 1 < height {
                    let d = cell_size * std::f32::consts::SQRT_2;
                    constraints.push(DistanceConstraint::new(idx, idx + width + 1, d, 0.5));
                    constraints.push(DistanceConstraint::new(idx + 1, idx + width, d, 0.5));
                    triangles.push([idx, idx + 1, idx + width + 1]);
                    triangles.push([idx, idx + width + 1, idx + width]);
                }
            }
        }

        Self { particles, constraints, triangles, width, height }
    }

    pub fn pin_row(&mut self, y: usize) {
        for x in 0..self.width {
            let idx = y * self.width + x;
            if idx < self.particles.len() {
                self.particles[idx].pinned = true;
                self.particles[idx].inv_mass = 0.0;
            }
        }
    }

    pub fn step(&mut self, config: &ClothConfig, dt: f32) {
        let gravity = config.gravity;
        let damping = config.damping;

        for p in &mut self.particles {
            if !p.pinned {
                let vel = (p.position - p.prev_position) * damping;
                p.prev_position = p.position;
                p.position += vel + gravity * dt * dt;
            }
        }

        for _ in 0..config.iterations {
            for c in &self.constraints {
                let a = c.particle_a;
                let b = c.particle_b;
                let delta = self.particles[b].position - self.particles[a].position;
                let len = delta.length();
                if len < 0.0001 { continue; }
                let diff = (len - c.rest_length) / len;
                let corr = delta * diff * 0.5 * c.stiffness;
                if !self.particles[a].pinned { self.particles[a].position += corr; }
                if !self.particles[b].pinned { self.particles[b].position -= corr; }
            }
        }

        for p in &mut self.particles {
            if !p.pinned {
                p.velocity = (p.position - p.prev_position) / dt.max(0.0001);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloth_mesh_creation() {
        let mesh = ClothMesh::new(5, 5, 0.5, 1.0);
        assert_eq!(mesh.particles.len(), 25);
        assert!(!mesh.constraints.is_empty());
    }

    #[test]
    fn cloth_particle_new() {
        let p = ClothParticle::new(Vec3::new(1.0, 2.0, 3.0), 0.5);
        assert!((p.mass - 0.5).abs() < 0.001);
        assert!(!p.pinned);
    }

    #[test]
    fn cloth_particle_pinned() {
        let p = ClothParticle::pinned(Vec3::ZERO);
        assert!(p.pinned);
        assert_eq!(p.inv_mass, 0.0);
    }

    #[test]
    fn cloth_mesh_pin_row() {
        let mut mesh = ClothMesh::new(3, 3, 1.0, 1.0);
        mesh.pin_row(0);
        assert!(mesh.particles[0].pinned);
        assert!(mesh.particles[1].pinned);
    }

    #[test]
    fn cloth_config_default() {
        let config = ClothConfig::default();
        assert!((config.gravity.y - (-9.81)).abs() < 0.001);
        assert_eq!(config.iterations, 8);
    }
}
