//! Navigation - Pathfinding for AI Agents

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NavNodeId(pub u32);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavNode {
    pub id: NavNodeId,
    pub position: [f32; 3],
    pub neighbors: Vec<NavNodeId>,
    pub flags: u32,
}

impl NavNode {
    pub fn new(id: NavNodeId, position: [f32; 3]) -> Self {
        Self {
            id,
            position,
            neighbors: Vec::new(),
            flags: 0,
        }
    }

    pub fn connect(&mut self, other: NavNodeId) {
        if !self.neighbors.contains(&other) {
            self.neighbors.push(other);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavMesh {
    nodes: HashMap<NavNodeId, NavNode>,
    next_id: u32,
}

impl Default for NavMesh {
    fn default() -> Self {
        Self::new()
    }
}

impl NavMesh {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            next_id: 0,
        }
    }

    pub fn add_node(&mut self, position: [f32; 3]) -> NavNodeId {
        let id = NavNodeId(self.next_id);
        self.next_id += 1;
        self.nodes.insert(id, NavNode::new(id, position));
        id
    }

    pub fn connect(&mut self, a: NavNodeId, b: NavNodeId) {
        if !self.nodes.contains_key(&a) || !self.nodes.contains_key(&b) {
            return;
        }
        if let Some(node_a) = self.nodes.get_mut(&a) {
            node_a.connect(b);
        }
        if let Some(node_b) = self.nodes.get_mut(&b) {
            node_b.connect(a);
        }
    }

    pub fn get_node(&self, id: NavNodeId) -> Option<&NavNode> {
        self.nodes.get(&id)
    }

    pub fn find_nearest(&self, position: [f32; 3]) -> Option<NavNodeId> {
        self.nodes
            .values()
            .min_by(|a, b| {
                let dist_a = distance_sq(a.position, position);
                let dist_b = distance_sq(b.position, position);
                dist_a.partial_cmp(&dist_b).unwrap()
            })
            .map(|n| n.id)
    }

    pub fn path(&self, start: NavNodeId, end: NavNodeId) -> Option<Vec<NavNodeId>> {
        if start == end {
            return Some(vec![start]);
        }

        let _start_node = self.nodes.get(&start)?;
        let end_node = self.nodes.get(&end)?;

        #[derive(Copy, Clone, Eq, PartialEq)]
        struct State {
            cost: u32,
            node: NavNodeId,
        }

        impl Ord for State {
            fn cmp(&self, other: &Self) -> Ordering {
                other.cost.cmp(&self.cost)
            }
        }

        impl PartialOrd for State {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }

        let mut frontier = BinaryHeap::new();
        let mut came_from: HashMap<NavNodeId, NavNodeId> = HashMap::new();
        let mut cost_so_far: HashMap<NavNodeId, u32> = HashMap::new();

        frontier.push(State {
            cost: 0,
            node: start,
        });
        came_from.insert(start, start);
        cost_so_far.insert(start, 0);

        while let Some(State { node: current, .. }) = frontier.pop() {
            if current == end {
                let mut path = vec![end];
                let mut current = end;
                while current != start {
                    current = *came_from.get(&current)?;
                    path.push(current);
                }
                path.reverse();
                return Some(path);
            }

            let current_node = self.nodes.get(&current)?;
            for &next in &current_node.neighbors {
                let next_node = self.nodes.get(&next)?;
                let new_cost = cost_so_far.get(&current).copied().unwrap_or(u32::MAX)
                    + distance(current_node.position, next_node.position) as u32;

                if new_cost < cost_so_far.get(&next).copied().unwrap_or(u32::MAX) {
                    cost_so_far.insert(next, new_cost);
                    let heuristic = distance(next_node.position, end_node.position) as u32;
                    frontier.push(State {
                        cost: new_cost + heuristic,
                        node: next,
                    });
                    came_from.insert(next, current);
                }
            }
        }

        None
    }
}

fn distance_sq(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    distance_sq(a, b).sqrt()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PathStatus {
    None,
    Requested,
    Computing,
    Ready,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathRequest {
    pub id: u64,
    pub start: [f32; 3],
    pub end: [f32; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathResult {
    pub request_id: u64,
    pub status: PathStatus,
    pub waypoints: Vec<[f32; 3]>,
    pub total_distance: f32,
}

impl PathResult {
    pub fn new(request_id: u64) -> Self {
        Self {
            request_id,
            status: PathStatus::None,
            waypoints: Vec::new(),
            total_distance: 0.0,
        }
    }

    pub fn is_ready(&self) -> bool {
        self.status == PathStatus::Ready
    }

    pub fn current_waypoint(&self) -> Option<[f32; 3]> {
        self.waypoints.first().copied()
    }

    pub fn advance(&mut self) -> Option<[f32; 3]> {
        if !self.waypoints.is_empty() {
            Some(self.waypoints.remove(0))
        } else {
            None
        }
    }

    pub fn remaining_distance(&self, position: [f32; 3]) -> f32 {
        let mut total = 0.0;
        let mut prev = position;
        for &wp in &self.waypoints {
            total += distance(prev, wp);
            prev = wp;
        }
        total
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavAgent {
    pub id: u64,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub target: Option<[f32; 3]>,
    pub path: Option<PathResult>,
    pub speed: f32,
    pub stopping_distance: f32,
    pub acceleration: f32,
}

impl NavAgent {
    pub fn new(id: u64, position: [f32; 3]) -> Self {
        Self {
            id,
            position,
            velocity: [0.0; 3],
            target: None,
            path: None,
            speed: 5.0,
            stopping_distance: 0.5,
            acceleration: 10.0,
        }
    }

    pub fn set_target(&mut self, target: [f32; 3]) {
        self.target = Some(target);
        self.path = None;
    }

    pub fn clear_target(&mut self) {
        self.target = None;
        self.path = None;
        self.velocity = [0.0; 3];
    }

    pub fn has_reached_target(&self) -> bool {
        if let Some(target) = self.target {
            distance(self.position, target) <= self.stopping_distance
        } else {
            true
        }
    }

    pub fn update(&mut self, dt: f32) {
        if let Some(ref mut path) = self.path {
            if path.is_ready() {
                if let Some(waypoint) = path.current_waypoint() {
                    let to_waypoint = [
                        waypoint[0] - self.position[0],
                        waypoint[1] - self.position[1],
                        waypoint[2] - self.position[2],
                    ];
                    let dist = distance(self.position, waypoint);

                    if dist < self.stopping_distance {
                        path.advance();
                    } else {
                        let dir = [
                            to_waypoint[0] / dist,
                            to_waypoint[1] / dist,
                            to_waypoint[2] / dist,
                        ];
                        let target_vel = [
                            dir[0] * self.speed,
                            dir[1] * self.speed,
                            dir[2] * self.speed,
                        ];

                        let accel = self.acceleration * dt;
                        self.velocity = [
                            self.velocity[0]
                                + (target_vel[0] - self.velocity[0]).clamp(-accel, accel),
                            self.velocity[1]
                                + (target_vel[1] - self.velocity[1]).clamp(-accel, accel),
                            self.velocity[2]
                                + (target_vel[2] - self.velocity[2]).clamp(-accel, accel),
                        ];

                        self.position = [
                            self.position[0] + self.velocity[0] * dt,
                            self.position[1] + self.velocity[1] * dt,
                            self.position[2] + self.velocity[2] * dt,
                        ];
                    }
                } else {
                    self.target = None;
                    self.velocity = [0.0; 3];
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nav_mesh_add_node() {
        let mut mesh = NavMesh::new();
        let id = mesh.add_node([0.0, 0.0, 0.0]);
        assert!(mesh.get_node(id).is_some());
    }

    #[test]
    fn nav_mesh_connect() {
        let mut mesh = NavMesh::new();
        let a = mesh.add_node([0.0, 0.0, 0.0]);
        let b = mesh.add_node([1.0, 0.0, 0.0]);
        mesh.connect(a, b);

        let node_a = mesh.get_node(a).unwrap();
        let node_b = mesh.get_node(b).unwrap();

        assert!(node_a.neighbors.contains(&b));
        assert!(node_b.neighbors.contains(&a));
    }

    #[test]
    fn nav_mesh_path_simple() {
        let mut mesh = NavMesh::new();
        let a = mesh.add_node([0.0, 0.0, 0.0]);
        let b = mesh.add_node([1.0, 0.0, 0.0]);
        let c = mesh.add_node([2.0, 0.0, 0.0]);

        mesh.connect(a, b);
        mesh.connect(b, c);

        let path = mesh.path(a, c);
        assert!(path.is_some());
        assert_eq!(path.unwrap(), vec![a, b, c]);
    }

    #[test]
    fn nav_mesh_path_no_path() {
        let mut mesh = NavMesh::new();
        let a = mesh.add_node([0.0, 0.0, 0.0]);
        let b = mesh.add_node([10.0, 0.0, 0.0]);

        let path = mesh.path(a, b);
        assert!(path.is_none());
    }

    #[test]
    fn nav_agent_reach_target() {
        let mut agent = NavAgent::new(1, [0.0, 0.0, 0.0]);
        agent.speed = 10.0;
        agent.stopping_distance = 0.5;

        agent.path = Some(PathResult {
            request_id: 0,
            status: PathStatus::Ready,
            waypoints: vec![[1.0, 0.0, 0.0]],
            total_distance: 1.0,
        });

        for _ in 0..100 {
            agent.update(0.016);
        }

        assert!(agent.position[0] > 0.5);
    }

    #[test]
    fn path_result_remaining_distance() {
        let result = PathResult {
            request_id: 0,
            status: PathStatus::Ready,
            waypoints: vec![[1.0, 0.0, 0.0], [2.0, 0.0, 0.0]],
            total_distance: 2.0,
        };

        let remaining = result.remaining_distance([0.0, 0.0, 0.0]);
        assert!((remaining - 2.0).abs() < 0.001);
    }
}
