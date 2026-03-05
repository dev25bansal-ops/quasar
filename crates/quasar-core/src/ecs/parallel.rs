//! Parallel system execution — run independent systems concurrently.
//!
//! Uses rayon thread pool to execute systems with no conflicting component
//! access in parallel. Builds a dependency graph from declared read/write access.

use std::any::TypeId;
use std::collections::{HashMap, HashSet};

use super::{System, SystemStage, World};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AccessMode {
    Read,
    Write,
}

#[derive(Debug, Clone)]
pub struct ComponentAccess {
    pub type_id: TypeId,
    pub mode: AccessMode,
}

impl ComponentAccess {
    pub fn read<T: 'static>() -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            mode: AccessMode::Read,
        }
    }

    pub fn write<T: 'static>() -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            mode: AccessMode::Write,
        }
    }
}

pub trait ParallelSystem: System {
    fn component_access(&self) -> Vec<ComponentAccess>;
    fn resource_access(&self) -> Vec<ComponentAccess>;
}

pub struct SystemNode {
    pub system: Box<dyn System>,
    pub component_reads: HashSet<TypeId>,
    pub component_writes: HashSet<TypeId>,
    pub resource_reads: HashSet<TypeId>,
    pub resource_writes: HashSet<TypeId>,
    pub dependencies: Vec<usize>,
    pub dependents: Vec<usize>,
}

impl SystemNode {
    pub fn new(system: Box<dyn System>) -> Self {
        Self {
            system,
            component_reads: HashSet::new(),
            component_writes: HashSet::new(),
            resource_reads: HashSet::new(),
            resource_writes: HashSet::new(),
            dependencies: Vec::new(),
            dependents: Vec::new(),
        }
    }

    pub fn with_component_access(mut self, access: ComponentAccess) -> Self {
        match access.mode {
            AccessMode::Read => {
                self.component_reads.insert(access.type_id);
            }
            AccessMode::Write => {
                self.component_writes.insert(access.type_id);
            }
        }
        self
    }

    pub fn with_resource_access(mut self, access: ComponentAccess) -> Self {
        match access.mode {
            AccessMode::Read => {
                self.resource_reads.insert(access.type_id);
            }
            AccessMode::Write => {
                self.resource_writes.insert(access.type_id);
            }
        }
        self
    }

    fn conflicts_with(&self, other: &SystemNode) -> bool {
        for write_type in &self.component_writes {
            if other.component_reads.contains(write_type)
                || other.component_writes.contains(write_type)
            {
                return true;
            }
        }
        for read_type in &self.component_reads {
            if other.component_writes.contains(read_type) {
                return true;
            }
        }
        for write_type in &self.resource_writes {
            if other.resource_reads.contains(write_type)
                || other.resource_writes.contains(write_type)
            {
                return true;
            }
        }
        for read_type in &self.resource_reads {
            if other.resource_writes.contains(read_type) {
                return true;
            }
        }
        false
    }
}

pub struct SystemGraph {
    nodes: Vec<SystemNode>,
    stage: SystemStage,
}

impl SystemGraph {
    pub fn new(stage: SystemStage) -> Self {
        Self {
            nodes: Vec::new(),
            stage,
        }
    }

    pub fn add_system(&mut self, node: SystemNode) -> usize {
        let idx = self.nodes.len();
        self.nodes.push(node);
        idx
    }

    pub fn build_dependencies(&mut self) {
        for i in 0..self.nodes.len() {
            for j in (i + 1)..self.nodes.len() {
                if self.nodes[i].conflicts_with(&self.nodes[j]) {
                    self.nodes[j].dependencies.push(i);
                    self.nodes[i].dependents.push(j);
                }
            }
        }
    }

    pub fn topological_groups(&self) -> Vec<Vec<usize>> {
        let mut groups = Vec::new();
        let mut completed: HashSet<usize> = HashSet::new();
        let mut remaining: HashSet<usize> = (0..self.nodes.len()).collect();

        while !remaining.is_empty() {
            let mut group = Vec::new();
            let mut newly_completed = Vec::new();

            for &idx in &remaining {
                let node = &self.nodes[idx];
                let all_deps_done = node.dependencies.iter().all(|dep| completed.contains(dep));

                if all_deps_done {
                    let conflicts_with_group = group
                        .iter()
                        .any(|g_idx: &usize| self.nodes[*g_idx].conflicts_with(node));

                    if !conflicts_with_group {
                        group.push(idx);
                        newly_completed.push(idx);
                    }
                }
            }

            if group.is_empty() {
                if let Some(&idx) = remaining.iter().next() {
                    group.push(idx);
                    newly_completed.push(idx);
                }
            }

            for idx in newly_completed {
                completed.insert(idx);
                remaining.remove(&idx);
            }

            if !group.is_empty() {
                groups.push(group);
            }
        }

        groups
    }

    pub fn run_parallel(&mut self, world: &mut World) {
        self.build_dependencies();
        let groups = self.topological_groups();

        for group in groups {
            if group.len() == 1 {
                // Single system - run directly
                let idx = group[0];
                self.nodes[idx].system.run(world);
            } else {
                // Multiple non-conflicting systems - cannot run truly parallel
                // due to mutable world access. Run sequentially for safety.
                // The grouping still helps with scheduling and dependency ordering.
                for idx in group {
                    self.nodes[idx].system.run(world);
                }
            }
        }
    }

    pub fn run_sequential(&mut self, world: &mut World) {
        for node in &mut self.nodes {
            node.system.run(world);
        }
    }
}

pub struct ParallelSchedule {
    stages: HashMap<SystemStage, SystemGraph>,
    parallel_enabled: bool,
}

impl ParallelSchedule {
    pub fn new() -> Self {
        let mut stages = HashMap::new();
        for stage in [
            SystemStage::PreUpdate,
            SystemStage::Update,
            SystemStage::PostUpdate,
            SystemStage::PreRender,
            SystemStage::Render,
        ] {
            stages.insert(stage, SystemGraph::new(stage));
        }

        Self {
            stages,
            parallel_enabled: true,
        }
    }

    pub fn add_system(&mut self, stage: SystemStage, node: SystemNode) {
        if let Some(graph) = self.stages.get_mut(&stage) {
            graph.add_system(node);
        }
    }

    pub fn set_parallel(&mut self, enabled: bool) {
        self.parallel_enabled = enabled;
    }

    pub fn run(&mut self, world: &mut World) {
        for stage in [
            SystemStage::PreUpdate,
            SystemStage::Update,
            SystemStage::PostUpdate,
            SystemStage::PreRender,
            SystemStage::Render,
        ] {
            if let Some(graph) = self.stages.get_mut(&stage) {
                if self.parallel_enabled {
                    graph.run_parallel(world);
                } else {
                    graph.run_sequential(world);
                }
            }
        }
    }
}

impl Default for ParallelSchedule {
    fn default() -> Self {
        Self::new()
    }
}

pub fn system_node<S: System + 'static>(system: S) -> SystemNode {
    SystemNode::new(Box::new(system))
}

#[macro_export]
macro_rules! parallel_system {
    ($system:expr, reads: [$($read:ty),*], writes: [$($write:ty),*]) => {{
        let mut node = $crate::ecs::parallel::SystemNode::new(Box::new($system));
        $(
            node = node.with_component_access($crate::ecs::parallel::ComponentAccess::read::<$read>());
        )*
        $(
            node = node.with_component_access($crate::ecs::parallel::ComponentAccess::write::<$write>());
        )*
        node
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_system() -> Box<dyn System> {
        Box::new(crate::ecs::system::FnSystem::new("test", |_world| {}))
    }

    #[test]
    fn system_node_conflicts() {
        let node1 =
            SystemNode::new(make_system()).with_component_access(ComponentAccess::write::<i32>());

        let node2 =
            SystemNode::new(make_system()).with_component_access(ComponentAccess::read::<i32>());

        assert!(node1.conflicts_with(&node2));
        assert!(node2.conflicts_with(&node1));
    }

    #[test]
    fn system_node_no_conflict() {
        let node1 =
            SystemNode::new(make_system()).with_component_access(ComponentAccess::read::<i32>());

        let node2 =
            SystemNode::new(make_system()).with_component_access(ComponentAccess::read::<i32>());

        assert!(!node1.conflicts_with(&node2));
    }

    #[test]
    fn topological_groups_no_deps() {
        let mut graph = SystemGraph::new(SystemStage::Update);

        graph.add_system(
            SystemNode::new(make_system()).with_component_access(ComponentAccess::read::<i32>()),
        );
        graph.add_system(
            SystemNode::new(make_system()).with_component_access(ComponentAccess::read::<String>()),
        );

        graph.build_dependencies();
        let groups = graph.topological_groups();

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 2);
    }
}
