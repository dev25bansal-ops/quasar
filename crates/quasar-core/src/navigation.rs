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
        if list.len() > 2 {
            let (u, v) = (*list.last().unwrap(), list[0]);
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
