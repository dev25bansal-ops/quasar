//! Navigation mesh and A* pathfinding.
//!
//! Provides a simple polygon-mesh representation for walkable surfaces and
//! an A* implementation that finds shortest paths across the mesh graph.
//! Also includes `NavMeshAgent` — an ECS component for entities that
//! follow computed paths.

use crate::ecs::{Entity, System, World};
use crate::TimeSnapshot;
use quasar_math::{Transform, Vec3};
use std::collections::{BinaryHeap, HashMap};
use std::cmp::Ordering;

// ---------------------------------------------------------------------------
// NavMesh data structure
// ---------------------------------------------------------------------------

/// A single polygon in the navigation mesh.
#[derive(Debug, Clone)]
pub struct NavPoly {
    /// Vertex indices into `NavMesh::vertices`.
    pub indices: Vec<usize>,
    /// Centroid of the polygon (pre-computed).
    pub centroid: Vec3,
    /// Indices of neighbouring polygons.
    pub neighbours: Vec<usize>,
}

/// A navigation mesh resource.
#[derive(Debug, Clone)]
pub struct NavMesh {
    pub vertices: Vec<Vec3>,
    pub polygons: Vec<NavPoly>,
}

impl NavMesh {
    pub fn new() -> Self {
        Self {
            vertices: Vec::new(),
            polygons: Vec::new(),
        }
    }

    /// Build from a list of vertices and polygon index lists.
    /// Neighbourhood is computed automatically from shared edges.
    pub fn from_polygons(vertices: Vec<Vec3>, index_lists: Vec<Vec<usize>>) -> Self {
        let mut polygons: Vec<NavPoly> = index_lists
            .into_iter()
            .map(|indices| {
                let centroid = if indices.is_empty() {
                    Vec3::ZERO
                } else {
                    let sum: Vec3 = indices.iter().map(|&i| vertices[i]).sum();
                    sum / indices.len() as f32
                };
                NavPoly {
                    indices,
                    centroid,
                    neighbours: Vec::new(),
                }
            })
            .collect();

        // Build adjacency: two polygons that share at least one edge
        // (two consecutive shared vertices) are neighbours.
        let len = polygons.len();
        for i in 0..len {
            for j in (i + 1)..len {
                if shares_edge(&polygons[i].indices, &polygons[j].indices) {
                    polygons[i].neighbours.push(j);
                    polygons[j].neighbours.push(i);
                }
            }
        }

        Self { vertices, polygons }
    }

    /// Find the polygon whose centroid is closest to `point`.
    pub fn closest_poly(&self, point: Vec3) -> Option<usize> {
        self.polygons
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                let da = a.centroid.distance_squared(point);
                let db = b.centroid.distance_squared(point);
                da.partial_cmp(&db).unwrap_or(Ordering::Equal)
            })
            .map(|(i, _)| i)
    }

    /// Build a navigation mesh from a heightmap grid.
    ///
    /// * `grid_width` / `grid_height` — number of samples in X and Z.
    /// * `heights` — row-major array of `grid_width * grid_height` heights.
    /// * `cell_size` — world-space distance between adjacent samples.
    /// * `max_slope` — maximum walkable slope angle in radians.
    ///
    /// Each grid cell that passes the slope test is converted into two
    /// triangles. Adjacency is computed automatically.
    pub fn from_height_grid(
        grid_width: usize,
        grid_height: usize,
        heights: &[f32],
        cell_size: f32,
        max_slope: f32,
    ) -> Self {
        assert_eq!(
            heights.len(),
            grid_width * grid_height,
            "heights length must equal grid_width * grid_height"
        );

        let idx = |x: usize, z: usize| -> usize { z * grid_width + x };
        let cos_max = max_slope.cos();

        // Build vertices.
        let mut vertices = Vec::with_capacity(grid_width * grid_height);
        for z in 0..grid_height {
            for x in 0..grid_width {
                let y = heights[idx(x, z)];
                vertices.push(Vec3::new(x as f32 * cell_size, y, z as f32 * cell_size));
            }
        }

        // Build triangle polygons, filtering by slope.
        let mut index_lists: Vec<Vec<usize>> = Vec::new();

        for z in 0..(grid_height - 1) {
            for x in 0..(grid_width - 1) {
                let i00 = idx(x, z);
                let i10 = idx(x + 1, z);
                let i01 = idx(x, z + 1);
                let i11 = idx(x + 1, z + 1);

                // Triangle A: (i00, i10, i01)
                let normal_a = (vertices[i10] - vertices[i00])
                    .cross(vertices[i01] - vertices[i00])
                    .normalize_or_zero();
                if normal_a.y >= cos_max {
                    index_lists.push(vec![i00, i10, i01]);
                }

                // Triangle B: (i10, i11, i01)
                let normal_b = (vertices[i11] - vertices[i10])
                    .cross(vertices[i01] - vertices[i10])
                    .normalize_or_zero();
                if normal_b.y >= cos_max {
                    index_lists.push(vec![i10, i11, i01]);
                }
            }
        }

        Self::from_polygons(vertices, index_lists)
    }
}

impl Default for NavMesh {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if two index lists share an edge (two vertices appearing
/// consecutively in both lists).
fn shares_edge(a: &[usize], b: &[usize]) -> bool {
    let edge_in = |list: &[usize]| -> Vec<(usize, usize)> {
        let mut edges = Vec::new();
        for w in list.windows(2) {
            let (u, v) = (w[0], w[1]);
            edges.push((u.min(v), u.max(v)));
        }
        if let Some(&last) = list.last() {
            let (u, v) = (last, list[0]);
            edges.push((u.min(v), u.max(v)));
        }
        edges
    };
    let edges_a = edge_in(a);
    let edges_b = edge_in(b);
    edges_a.iter().any(|e| edges_b.contains(e))
}

// ---------------------------------------------------------------------------
// A* pathfinding
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct AStarNode {
    poly_index: usize,
    g: f32,
    f: f32,
}

impl PartialEq for AStarNode {
    fn eq(&self, other: &Self) -> bool {
        self.poly_index == other.poly_index
    }
}
impl Eq for AStarNode {}

impl Ord for AStarNode {
    fn cmp(&self, other: &Self) -> Ordering {
        other.f.partial_cmp(&self.f).unwrap_or(Ordering::Equal)
    }
}
impl PartialOrd for AStarNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Compute the shortest path (as a list of polygon indices) through `mesh`
/// from `start_poly` to `goal_poly` using A*.
pub fn find_path(mesh: &NavMesh, start_poly: usize, goal_poly: usize) -> Option<Vec<usize>> {
    if start_poly == goal_poly {
        return Some(vec![start_poly]);
    }

    let goal_centroid = mesh.polygons[goal_poly].centroid;
    let heuristic = |i: usize| mesh.polygons[i].centroid.distance(goal_centroid);

    let mut open = BinaryHeap::new();
    let mut came_from: HashMap<usize, usize> = HashMap::new();
    let mut g_score: HashMap<usize, f32> = HashMap::new();

    g_score.insert(start_poly, 0.0);
    open.push(AStarNode {
        poly_index: start_poly,
        g: 0.0,
        f: heuristic(start_poly),
    });

    while let Some(current) = open.pop() {
        if current.poly_index == goal_poly {
            // Reconstruct path.
            let mut path = vec![goal_poly];
            let mut cur = goal_poly;
            while let Some(&prev) = came_from.get(&cur) {
                path.push(prev);
                cur = prev;
            }
            path.reverse();
            return Some(path);
        }

        for &neighbour in &mesh.polygons[current.poly_index].neighbours {
            let edge_cost = mesh.polygons[current.poly_index]
                .centroid
                .distance(mesh.polygons[neighbour].centroid);
            let tentative_g = current.g + edge_cost;

            if tentative_g < *g_score.get(&neighbour).unwrap_or(&f32::INFINITY) {
                came_from.insert(neighbour, current.poly_index);
                g_score.insert(neighbour, tentative_g);
                open.push(AStarNode {
                    poly_index: neighbour,
                    g: tentative_g,
                    f: tentative_g + heuristic(neighbour),
                });
            }
        }
    }

    None // No path found.
}

/// Convert a polygon-index path to a list of world-space waypoints
/// (polygon centroids).
pub fn path_to_waypoints(mesh: &NavMesh, path: &[usize]) -> Vec<Vec3> {
    path.iter().map(|&i| mesh.polygons[i].centroid).collect()
}

// ---------------------------------------------------------------------------
// NavMesh Agent component
// ---------------------------------------------------------------------------

/// ECS component for entities that navigate along a NavMesh path.
#[derive(Debug, Clone)]
pub struct NavMeshAgent {
    /// Movement speed in units per second.
    pub speed: f32,
    /// Current waypoint list.
    pub waypoints: Vec<Vec3>,
    /// Index of the next waypoint to move toward.
    pub current_waypoint: usize,
    /// Distance threshold to consider a waypoint "reached".
    pub arrival_threshold: f32,
    /// `true` once the agent has reached the final waypoint.
    pub arrived: bool,
}

impl NavMeshAgent {
    pub fn new(speed: f32) -> Self {
        Self {
            speed,
            waypoints: Vec::new(),
            current_waypoint: 0,
            arrival_threshold: 0.3,
            arrived: true,
        }
    }

    /// Set a new path for the agent.
    pub fn set_path(&mut self, waypoints: Vec<Vec3>) {
        self.waypoints = waypoints;
        self.current_waypoint = 0;
        self.arrived = self.waypoints.is_empty();
    }

    /// Set destination via navmesh lookup + pathfinding.
    pub fn navigate_to(&mut self, mesh: &NavMesh, from: Vec3, to: Vec3) {
        let start = match mesh.closest_poly(from) {
            Some(s) => s,
            None => return,
        };
        let goal = match mesh.closest_poly(to) {
            Some(g) => g,
            None => return,
        };
        if let Some(path) = find_path(mesh, start, goal) {
            let wps = path_to_waypoints(mesh, &path);
            self.set_path(wps);
        }
    }
}

// ---------------------------------------------------------------------------
// Dynamic obstacles
// ---------------------------------------------------------------------------

/// A dynamic obstacle that can carve holes in the navmesh at runtime.
#[derive(Debug, Clone)]
pub struct NavObstacle {
    /// Obstacle shape.
    pub shape: NavObstacleShape,
    /// World-space center position.
    pub position: Vec3,
    /// Whether the obstacle is currently active (blocking navigation).
    pub active: bool,
}

/// Supported obstacle shapes.
#[derive(Debug, Clone)]
pub enum NavObstacleShape {
    /// Axis-aligned box with half-extents.
    Box { half_extents: Vec3 },
    /// Cylinder with radius and height (Y-up).
    Cylinder { radius: f32, height: f32 },
}

impl NavObstacle {
    /// Create a box obstacle.
    pub fn new_box(position: Vec3, half_extents: Vec3) -> Self {
        Self {
            shape: NavObstacleShape::Box { half_extents },
            position,
            active: true,
        }
    }

    /// Create a cylinder obstacle.
    pub fn new_cylinder(position: Vec3, radius: f32, height: f32) -> Self {
        Self {
            shape: NavObstacleShape::Cylinder { radius, height },
            position,
            active: true,
        }
    }

    /// Check if a world-space point is inside this obstacle's volume.
    pub fn contains(&self, point: Vec3) -> bool {
        if !self.active {
            return false;
        }
        match &self.shape {
            NavObstacleShape::Box { half_extents } => {
                let d = point - self.position;
                d.x.abs() <= half_extents.x
                    && d.y.abs() <= half_extents.y
                    && d.z.abs() <= half_extents.z
            }
            NavObstacleShape::Cylinder { radius, height } => {
                let d = point - self.position;
                let horiz = (d.x * d.x + d.z * d.z).sqrt();
                horiz <= *radius && d.y.abs() <= *height * 0.5
            }
        }
    }
}

/// Manages the navmesh with dynamic obstacle carving.
///
/// Maintains a reference copy of the original mesh and rebuilds a
/// carved version whenever obstacles change.
pub struct DynamicNavMesh {
    /// The original, uncarved navmesh.
    pub base_mesh: NavMesh,
    /// The current navmesh with obstacles carved out.
    pub carved_mesh: NavMesh,
    /// Active obstacles.
    pub obstacles: Vec<NavObstacle>,
    /// Dirty flag — set when obstacles changed and carving is needed.
    dirty: bool,
}

impl DynamicNavMesh {
    pub fn new(base_mesh: NavMesh) -> Self {
        let carved_mesh = base_mesh.clone();
        Self {
            base_mesh,
            carved_mesh,
            obstacles: Vec::new(),
            dirty: false,
        }
    }

    /// Add a dynamic obstacle and mark the mesh as dirty.
    pub fn add_obstacle(&mut self, obstacle: NavObstacle) -> usize {
        let id = self.obstacles.len();
        self.obstacles.push(obstacle);
        self.dirty = true;
        id
    }

    /// Remove an obstacle by index.
    pub fn remove_obstacle(&mut self, index: usize) {
        if index < self.obstacles.len() {
            self.obstacles.swap_remove(index);
            self.dirty = true;
        }
    }

    /// Move an obstacle to a new position.
    pub fn move_obstacle(&mut self, index: usize, new_position: Vec3) {
        if let Some(obs) = self.obstacles.get_mut(index) {
            obs.position = new_position;
            self.dirty = true;
        }
    }

    /// Toggle obstacle active state.
    pub fn set_obstacle_active(&mut self, index: usize, active: bool) {
        if let Some(obs) = self.obstacles.get_mut(index) {
            obs.active = active;
            self.dirty = true;
        }
    }

    /// Rebuild the carved mesh if dirty. Call once per frame.
    ///
    /// Polygons whose centroid falls inside any active obstacle are
    /// removed from the carved mesh.
    pub fn rebuild_if_dirty(&mut self) {
        if !self.dirty {
            return;
        }
        self.dirty = false;

        let active_obstacles: Vec<&NavObstacle> =
            self.obstacles.iter().filter(|o| o.active).collect();

        if active_obstacles.is_empty() {
            self.carved_mesh = self.base_mesh.clone();
            return;
        }

        // Filter out polygons blocked by obstacles.
        let kept_indices: Vec<Vec<usize>> = self
            .base_mesh
            .polygons
            .iter()
            .filter(|poly| {
                !active_obstacles.iter().any(|obs| obs.contains(poly.centroid))
            })
            .map(|poly| poly.indices.clone())
            .collect();

        self.carved_mesh =
            NavMesh::from_polygons(self.base_mesh.vertices.clone(), kept_indices);
    }

    /// Find a path on the current carved mesh.
    pub fn find_path(&mut self, from: Vec3, to: Vec3) -> Option<Vec<Vec3>> {
        self.rebuild_if_dirty();
        let start = self.carved_mesh.closest_poly(from)?;
        let goal = self.carved_mesh.closest_poly(to)?;
        let path = find_path(&self.carved_mesh, start, goal)?;
        Some(path_to_waypoints(&self.carved_mesh, &path))
    }
}

// ---------------------------------------------------------------------------
// System
// ---------------------------------------------------------------------------

/// System that moves `NavMeshAgent` entities along their waypoints.
pub struct NavMeshAgentSystem;

impl System for NavMeshAgentSystem {
    fn name(&self) -> &str {
        "navmesh_agent"
    }

    fn run(&mut self, world: &mut World) {
        let delta = world
            .resource::<TimeSnapshot>()
            .map(|t| t.delta_seconds)
            .unwrap_or(1.0 / 60.0);

        let agents: Vec<(Entity, NavMeshAgent)> = world
            .query::<NavMeshAgent>()
            .into_iter()
            .filter(|(_, a)| !a.arrived)
            .map(|(e, a)| (e, a.clone()))
            .collect();

        for (entity, mut agent) in agents {
            let pos = match world.get::<Transform>(entity) {
                Some(tf) => tf.position,
                None => continue,
            };

            if agent.current_waypoint >= agent.waypoints.len() {
                agent.arrived = true;
            } else {
                let target = agent.waypoints[agent.current_waypoint];
                let diff = target - pos;
                let dist = diff.length();

                if dist <= agent.arrival_threshold {
                    agent.current_waypoint += 1;
                    if agent.current_waypoint >= agent.waypoints.len() {
                        agent.arrived = true;
                    }
                } else {
                    let step = diff.normalize() * agent.speed * delta;
                    if let Some(tf) = world.get_mut::<Transform>(entity) {
                        tf.position += step;
                    }
                }
            }

            if let Some(a) = world.get_mut::<NavMeshAgent>(entity) {
                *a = agent;
            }
        }
    }
}
