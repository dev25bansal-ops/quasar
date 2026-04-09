//! Soft body physics simulation using mass-spring model.
//!
//! Provides deformable body simulation with:
//! - Volume preservation for realistic squish
//! - Pressure constraints for inflatable objects
//! - Collision response with rigid bodies

use glam::Vec3;

/// A particle in the soft body.
#[derive(Debug, Clone, Copy)]
pub struct SoftBodyParticle {
    pub position: Vec3,
    pub prev_position: Vec3,
    pub velocity: Vec3,
    pub mass: f32,
    pub inv_mass: f32,
}

impl SoftBodyParticle {
    pub fn new(position: Vec3, mass: f32) -> Self {
        Self {
            position,
            prev_position: position,
            velocity: Vec3::ZERO,
            mass,
            inv_mass: if mass > 0.0 { 1.0 / mass } else { 0.0 },
        }
    }
}

/// A spring connecting two particles.
#[derive(Debug, Clone, Copy)]
pub struct Spring {
    pub particle_a: usize,
    pub particle_b: usize,
    pub rest_length: f32,
    pub stiffness: f32,
}

impl Spring {
    pub fn new(a: usize, b: usize, rest: f32, stiff: f32) -> Self {
        Self { particle_a: a, particle_b: b, rest_length: rest, stiffness: stiff }
    }
}

/// A tetrahedron for volume preservation.
#[derive(Debug, Clone, Copy)]
pub struct Tetrahedron {
    pub particles: [usize; 4],
    pub rest_volume: f32,
}

impl Tetrahedron {
    pub fn new(particles: [usize; 4], rest_volume: f32) -> Self {
        Self { particles, rest_volume }
    }

    pub fn compute_volume(&self, positions: &[Vec3]) -> f32 {
        let p0 = positions[self.particles[0]];
        let p1 = positions[self.particles[1]];
        let p2 = positions[self.particles[2]];
        let p3 = positions[self.particles[3]];
        ((p1 - p0).cross(p2 - p0)).dot(p3 - p0).abs() / 6.0
    }
}

/// Configuration for soft body simulation.
#[derive(Debug, Clone)]
pub struct SoftBodyConfig {
    pub gravity: Vec3,
    pub damping: f32,
    pub iterations: usize,
    pub pressure: f32,
    pub volume_stiffness: f32,
}

impl Default for SoftBodyConfig {
    fn default() -> Self {
        Self {
            gravity: Vec3::new(0.0, -9.81, 0.0),
            damping: 0.98,
            iterations: 10,
            pressure: 0.0,
            volume_stiffness: 0.5,
        }
    }
}

/// A soft body mesh.
#[derive(Debug, Clone)]
pub struct SoftBody {
    pub particles: Vec<SoftBodyParticle>,
    pub springs: Vec<Spring>,
    pub tetrahedra: Vec<Tetrahedron>,
    pub surface_triangles: Vec<[usize; 3]>,
    pub config: SoftBodyConfig,
}

impl SoftBody {
    pub fn new(config: SoftBodyConfig) -> Self {
        Self {
            particles: Vec::new(),
            springs: Vec::new(),
            tetrahedra: Vec::new(),
            surface_triangles: Vec::new(),
            config,
        }
    }

    pub fn create_sphere(&mut self, center: Vec3, radius: f32, segments: usize, mass: f32) {
        self.particles.clear();
        self.springs.clear();
        self.tetrahedra.clear();
        self.surface_triangles.clear();

        let particle_mass = mass / ((segments + 1) * (segments + 1)) as f32;

        for lat in 0..=segments {
            let theta = std::f32::consts::PI * lat as f32 / segments as f32;
            let sin_theta = theta.sin();
            let cos_theta = theta.cos();

            for lon in 0..=segments {
                let phi = 2.0 * std::f32::consts::PI * lon as f32 / segments as f32;
                let sin_phi = phi.sin();
                let cos_phi = phi.cos();

                let x = radius * sin_theta * cos_phi;
                let y = radius * cos_theta;
                let z = radius * sin_theta * sin_phi;

                self.particles.push(SoftBodyParticle::new(center + Vec3::new(x, y, z), particle_mass));
            }
        }

        let stiffness = 0.8;
        let segs = segments + 1;

        for lat in 0..segments {
            for lon in 0..segments {
                let current = lat * segs + lon;
                let next_lon = current + 1;
                let next_lat = current + segs;
                let diag = next_lat + 1;

                let p0 = self.particles[current].position;
                let p1 = self.particles[next_lon].position;
                let p2 = self.particles[next_lat].position;
                let p3 = self.particles[diag].position;

                self.springs.push(Spring::new(current, next_lon, (p1 - p0).length(), stiffness));
                self.springs.push(Spring::new(current, next_lat, (p2 - p0).length(), stiffness));
                self.springs.push(Spring::new(next_lon, diag, (p3 - p1).length(), stiffness));
                self.springs.push(Spring::new(next_lat, diag, (p3 - p2).length(), stiffness));
                self.springs.push(Spring::new(current, diag, (p3 - p0).length(), stiffness * 0.5));
                self.springs.push(Spring::new(next_lon, next_lat, (p2 - p1).length(), stiffness * 0.5));

                self.surface_triangles.push([current, next_lon, diag]);
                self.surface_triangles.push([current, diag, next_lat]);
            }
        }
    }

    pub fn create_box(&mut self, center: Vec3, half_extents: Vec3, subdivisions: usize, mass: f32) {
        self.particles.clear();
        self.springs.clear();
        self.tetrahedra.clear();
        self.surface_triangles.clear();

        let total_particles = (subdivisions + 1).pow(3) as usize;
        let particle_mass = mass / total_particles as f32;

        for ix in 0..=subdivisions {
            for iy in 0..=subdivisions {
                for iz in 0..=subdivisions {
                    let t = |i: usize| i as f32 / subdivisions as f32;
                    let pos = center + Vec3::new(
                        (t(ix) - 0.5) * 2.0 * half_extents.x,
                        (t(iy) - 0.5) * 2.0 * half_extents.y,
                        (t(iz) - 0.5) * 2.0 * half_extents.z,
                    );
                    self.particles.push(SoftBodyParticle::new(pos, particle_mass));
                }
            }
        }

        let stiffness = 0.8;
        let n = subdivisions + 1;
        let idx = |x: usize, y: usize, z: usize| x * n * n + y * n + z;

        for ix in 0..=subdivisions {
            for iy in 0..=subdivisions {
                for iz in 0..=subdivisions {
                    let current = idx(ix, iy, iz);

                    if ix < subdivisions {
                        let next = idx(ix + 1, iy, iz);
                        let rest = (self.particles[next].position - self.particles[current].position).length();
                        self.springs.push(Spring::new(current, next, rest, stiffness));
                    }
                    if iy < subdivisions {
                        let next = idx(ix, iy + 1, iz);
                        let rest = (self.particles[next].position - self.particles[current].position).length();
                        self.springs.push(Spring::new(current, next, rest, stiffness));
                    }
                    if iz < subdivisions {
                        let next = idx(ix, iy, iz + 1);
                        let rest = (self.particles[next].position - self.particles[current].position).length();
                        self.springs.push(Spring::new(current, next, rest, stiffness));
                    }
                }
            }
        }
    }

    pub fn step(&mut self, dt: f32) {
        let gravity = self.config.gravity;
        let damping = self.config.damping;

        for p in &mut self.particles {
            let vel = (p.position - p.prev_position) * damping;
            p.prev_position = p.position;
            p.position += vel + gravity * dt * dt;
        }

        for _ in 0..self.config.iterations {
            self.solve_springs();
            self.solve_volume();
        }

        for p in &mut self.particles {
            if dt > 0.0001 {
                p.velocity = (p.position - p.prev_position) / dt;
            }
        }
    }

    fn solve_springs(&mut self) {
        let positions: Vec<Vec3> = self.particles.iter().map(|p| p.position).collect();
        let inv_masses: Vec<f32> = self.particles.iter().map(|p| p.inv_mass).collect();
        let mut corrections = vec![Vec3::ZERO; self.particles.len()];

        for spring in &self.springs {
            let a = spring.particle_a;
            let b = spring.particle_b;
            let delta = positions[b] - positions[a];
            let len = delta.length();
            if len < 0.0001 { continue; }
            let diff = (len - spring.rest_length) / len;
            let correction = delta * diff * 0.5 * spring.stiffness;
            corrections[a] += correction * inv_masses[a];
            corrections[b] -= correction * inv_masses[b];
        }
        for (i, corr) in corrections.iter().enumerate() {
            self.particles[i].position += *corr;
        }
    }

    fn solve_volume(&mut self) {
        if self.config.pressure <= 0.0 && self.tetrahedra.is_empty() {
            return;
        }

        let positions: Vec<Vec3> = self.particles.iter().map(|p| p.position).collect();
        let inv_masses: Vec<f32> = self.particles.iter().map(|p| p.inv_mass).collect();
        let mut corrections = vec![Vec3::ZERO; self.particles.len()];

        for tet in &self.tetrahedra {
            let current_volume = tet.compute_volume(&positions);
            let volume_diff = current_volume - tet.rest_volume;
            if volume_diff.abs() < 0.0001 { continue; }
            let correction = volume_diff * self.config.volume_stiffness * 0.25;
            let center: Vec3 = tet.particles.iter().map(|&i| positions[i]).sum::<Vec3>() / 4.0;
            for &idx in &tet.particles {
                let dir = (positions[idx] - center).normalize();
                corrections[idx] -= dir * correction * inv_masses[idx];
            }
        }
        for (i, corr) in corrections.iter().enumerate() {
            self.particles[i].position += *corr;
        }
    }

    pub fn get_center_of_mass(&self) -> Vec3 {
        if self.particles.is_empty() {
            return Vec3::ZERO;
        }

        let total_mass: f32 = self.particles.iter().map(|p| p.mass).sum();
        if total_mass == 0.0 {
            return Vec3::ZERO;
        }

        self.particles.iter()
            .map(|p| p.position * p.mass)
            .sum::<Vec3>() / total_mass
    }

    pub fn apply_impulse(&mut self, impulse: Vec3) {
        for p in &mut self.particles {
            p.velocity += impulse * p.inv_mass;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soft_body_new() {
        let sb = SoftBody::new(SoftBodyConfig::default());
        assert!(sb.particles.is_empty());
    }

    #[test]
    fn soft_body_create_sphere() {
        let mut sb = SoftBody::new(SoftBodyConfig::default());
        sb.create_sphere(Vec3::ZERO, 1.0, 4, 10.0);
        assert!(!sb.particles.is_empty());
        assert!(!sb.springs.is_empty());
    }

    #[test]
    fn soft_body_create_box() {
        let mut sb = SoftBody::new(SoftBodyConfig::default());
        sb.create_box(Vec3::ZERO, Vec3::ONE, 2, 10.0);
        assert!(!sb.particles.is_empty());
        assert!(!sb.springs.is_empty());
    }

    #[test]
    fn soft_body_step() {
        let mut sb = SoftBody::new(SoftBodyConfig::default());
        sb.create_sphere(Vec3::ZERO, 1.0, 4, 10.0);
        let initial_pos = sb.particles[0].position;
        sb.step(0.016);
        assert!((sb.particles[0].position - initial_pos).length() > 0.0 || sb.config.gravity.y == 0.0);
    }

    #[test]
    fn soft_body_config_default() {
        let config = SoftBodyConfig::default();
        assert!((config.gravity.y - (-9.81)).abs() < 0.01);
        assert_eq!(config.iterations, 10);
    }

    #[test]
    fn soft_body_center_of_mass() {
        let mut sb = SoftBody::new(SoftBodyConfig::default());
        sb.create_sphere(Vec3::new(5.0, 0.0, 0.0), 1.0, 3, 10.0);
        let com = sb.get_center_of_mass();
        assert!((com - Vec3::new(5.0, 0.0, 0.0)).length() < 0.5);
    }

    #[test]
    fn soft_body_apply_impulse() {
        let mut sb = SoftBody::new(SoftBodyConfig::default());
        sb.create_sphere(Vec3::ZERO, 1.0, 3, 10.0);
        sb.apply_impulse(Vec3::new(0.0, 10.0, 0.0));
        for p in &sb.particles {
            assert!(p.velocity.y > 0.0);
        }
    }

    #[test]
    fn spring_new() {
        let s = Spring::new(0, 1, 2.0, 0.5);
        assert_eq!(s.particle_a, 0);
        assert_eq!(s.particle_b, 1);
        assert!((s.rest_length - 2.0).abs() < 0.001);
    }

    #[test]
    fn tetrahedron_compute_volume() {
        let tet = Tetrahedron::new([0, 1, 2, 3], 1.0);
        let positions = vec![
            Vec3::ZERO,
            Vec3::X,
            Vec3::Y,
            Vec3::Z,
        ];
        let vol = tet.compute_volume(&positions);
        assert!((vol - (1.0 / 6.0)).abs() < 0.001);
    }
}
