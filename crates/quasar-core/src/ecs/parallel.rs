//! Parallel system execution — run independent systems concurrently.
//!
//! Uses rayon thread pool to execute systems with no conflicting component
//! access in parallel. Systems must be thread-safe (Send + Sync).

use std::any::TypeId;
use std::collections::{HashMap, HashSet};

use rayon;
use smallvec::SmallVec;

use super::{Commands, System, SystemStage, World};

/// Compact declaration of a system's data access requirements.
/// Used by `SystemGraph` to auto-build the dependency DAG.
#[derive(Debug, Clone, Default)]
pub struct SystemAccess {
    pub reads: SmallVec<[TypeId; 8]>,
    pub writes: SmallVec<[TypeId; 8]>,
    pub resources_read: SmallVec<[TypeId; 4]>,
    pub resources_write: SmallVec<[TypeId; 4]>,
}

impl SystemAccess {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn read<T: 'static>(mut self) -> Self {
        self.reads.push(TypeId::of::<T>());
        self
    }

    pub fn write<T: 'static>(mut self) -> Self {
        self.writes.push(TypeId::of::<T>());
        self
    }

    pub fn res_read<T: 'static>(mut self) -> Self {
        self.resources_read.push(TypeId::of::<T>());
        self
    }

    pub fn res_write<T: 'static>(mut self) -> Self {
        self.resources_write.push(TypeId::of::<T>());
        self
    }
}

/// Trait for systems that declare their access via `SystemAccess`.
/// `SystemGraph` will auto-populate `SystemNode` fields from this.
pub trait DeclareAccess: System {
    fn access(&self) -> SystemAccess;
}
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
    /// Explicit ordering: this system must run after all systems whose names
    /// are listed here.
    pub after: Vec<String>,
    /// Explicit ordering: this system must run before all systems whose names
    /// are listed here.
    pub before: Vec<String>,
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
            after: Vec::new(),
            before: Vec::new(),
        }
    }

    /// Declare that this system must run **after** the named system.
    pub fn after(mut self, name: impl Into<String>) -> Self {
        self.after.push(name.into());
        self
    }

    /// Declare that this system must run **before** the named system.
    pub fn before(mut self, name: impl Into<String>) -> Self {
        self.before.push(name.into());
        self
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
    _stage: SystemStage,
}

impl SystemGraph {
    pub fn new(stage: SystemStage) -> Self {
        Self {
            nodes: Vec::new(),
            _stage: stage,
        }
    }

    pub fn add_system(&mut self, node: SystemNode) -> usize {
        let idx = self.nodes.len();
        self.nodes.push(node);
        idx
    }

    /// Add a system that implements `DeclareAccess`; automatically populates
    /// reads/writes from the trait so the DAG builds itself.
    pub fn add_system_auto<S: DeclareAccess + 'static>(&mut self, system: S) -> usize {
        let access = system.access();
        let mut node = SystemNode::new(Box::new(system));
        node.component_reads = access.reads.iter().copied().collect();
        node.component_writes = access.writes.iter().copied().collect();
        node.resource_reads = access.resources_read.iter().copied().collect();
        node.resource_writes = access.resources_write.iter().copied().collect();
        self.add_system(node)
    }

    /// Add a plain function-system with explicit read/write type sets.
    ///
    /// This is the easiest way to register a closure-based system with
    /// auto-generated dependency edges:
    ///
    /// ```ignore
    /// graph.add_fn_system::<(Velocity,), (Position,)>("move", |w| { /* ... */ });
    /// ```
    pub fn add_fn_system<R, W>(
        &mut self,
        name: &str,
        system: impl FnMut(&mut World) + Send + Sync + 'static,
    ) -> usize
    where
        R: ReadWriteSet,
        W: ReadWriteSet,
    {
        let node = system_node_with_access::<_, R, W>(
            crate::ecs::system::FnSystem::new(name, system),
        );
        self.add_system(node)
    }

    pub fn build_dependencies(&mut self) {
        // Build name → index map for explicit ordering lookups.
        let name_map: HashMap<String, usize> = self
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.system.name().to_string(), i))
            .collect();

        // Apply explicit .after() / .before() constraints.
        let n = self.nodes.len();
        let mut explicit_edges: Vec<(usize, usize)> = Vec::new();
        for i in 0..n {
            for after_name in &self.nodes[i].after.clone() {
                if let Some(&dep_idx) = name_map.get(after_name) {
                    explicit_edges.push((dep_idx, i)); // dep_idx runs before i
                }
            }
            for before_name in &self.nodes[i].before.clone() {
                if let Some(&later_idx) = name_map.get(before_name) {
                    explicit_edges.push((i, later_idx)); // i runs before later_idx
                }
            }
        }
        for (earlier, later) in explicit_edges {
            self.nodes[later].dependencies.push(earlier);
            self.nodes[earlier].dependents.push(later);
        }

        // Auto-detect data conflicts for remaining pairs.
        for i in 0..n {
            for j in (i + 1)..n {
                if self.nodes[i].conflicts_with(&self.nodes[j]) {
                    // Only add if not already connected by explicit ordering.
                    if !self.nodes[j].dependencies.contains(&i)
                        && !self.nodes[i].dependencies.contains(&j)
                    {
                        self.nodes[j].dependencies.push(i);
                        self.nodes[i].dependents.push(j);
                    }
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
                let idx = group[0];
                world.begin_system(self.nodes[idx].system.name());
                self.nodes[idx].system.run(world);
                world.end_system(self.nodes[idx].system.name());
            } else {
                // Systems in the same topological group have been verified to have
                // no conflicting component/resource access, so they can run in parallel.
                //
                // We encode pointers as usize so closures satisfy Send + Sync.
                let world_addr = world as *mut World as usize;
                let nodes_ptr = self.nodes.as_mut_ptr();

                // Save indices for post-scope tick update (group is consumed by into_iter).
                let group_indices = group.clone();

                let work: Vec<(usize, usize)> = group
                    .into_iter()
                    .map(|idx| {
                        let node_addr = unsafe { nodes_ptr.add(idx) } as usize;
                        (node_addr, world_addr)
                    })
                    .collect();

                rayon::scope(|s| {
                    for &(node_addr, w_addr) in &work {
                        s.spawn(move |_| {
                            // SAFETY:
                            // 1. Each node pointer is unique — no two threads
                            //    touch the same SystemNode.
                            // 2. The topological grouping guarantees that systems
                            //    in the same group have disjoint component/resource
                            //    access, so concurrent &mut World access is safe.
                            unsafe {
                                let node = &mut *(node_addr as *mut SystemNode);
                                node.system.run(&mut *(w_addr as *mut World));
                            }
                        });
                    }
                });

                // After the parallel group completes, record last-run ticks.
                // NOTE: active_system_last_run is NOT set during parallel
                // execution (data-race on a single field), so FilterChanged
                // inside parallel systems conservatively treats everything as
                // changed.  The tick bookkeeping here ensures subsequent
                // sequential runs see correct last-run values.
                for idx in group_indices {
                    world.end_system(self.nodes[idx].system.name());
                }
            }
        }
    }

    pub fn run_sequential(&mut self, world: &mut World) {
        for node in &mut self.nodes {
            world.begin_system(node.system.name());
            node.system.run(world);
            world.end_system(node.system.name());
        }
    }
}

pub struct ParallelSchedule {
    stages: HashMap<SystemStage, SystemGraph>,
    parallel_enabled: bool,
}

const STAGE_ORDER: [SystemStage; 6] = [
    SystemStage::PreUpdate,
    SystemStage::FixedUpdate,
    SystemStage::Update,
    SystemStage::PostUpdate,
    SystemStage::PreRender,
    SystemStage::Render,
];

impl ParallelSchedule {
    pub fn new() -> Self {
        let mut stages = HashMap::new();
        for stage in STAGE_ORDER {
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

    fn run_graph(&mut self, stage: SystemStage, world: &mut World) {
        if let Some(graph) = self.stages.get_mut(&stage) {
            if self.parallel_enabled {
                graph.run_parallel(world);
            } else {
                graph.run_sequential(world);
            }
        }
        if let Some(mut cmds) = world.remove_resource::<Commands>() {
            cmds.apply(world);
            world.insert_resource(cmds);
        }
    }

    pub fn run(&mut self, world: &mut World) {
        for stage in STAGE_ORDER {
            self.run_graph(stage, world);
        }
    }

    /// Run with fixed-update substep loop for the `FixedUpdate` stage.
    pub fn run_with_fixed_update(&mut self, world: &mut World, frame_delta: f32) {
        use crate::time::FixedUpdateAccumulator;

        for stage in STAGE_ORDER {
            if stage == SystemStage::FixedUpdate {
                let (acc, step) = if let Some(fua) = world.resource_mut::<FixedUpdateAccumulator>() {
                    fua.acc += frame_delta;
                    (fua.acc, fua.step)
                } else {
                    self.run_graph(stage, world);
                    continue;
                };

                if step <= 0.0 {
                    continue;
                }

                let mut remaining = acc;
                while remaining >= step {
                    self.run_graph(stage, world);
                    remaining -= step;
                }

                if let Some(fua) = world.resource_mut::<FixedUpdateAccumulator>() {
                    fua.acc = remaining;
                }
            } else {
                self.run_graph(stage, world);
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

/// Declare the set of component types a system reads.
/// Returns a `Vec<ComponentAccess>` for use with `SystemNode::with_component_access`.
pub fn read_set<T: ReadWriteSet>() -> Vec<ComponentAccess> {
    T::accesses(AccessMode::Read)
}

/// Declare the set of component types a system writes.
pub fn write_set<T: ReadWriteSet>() -> Vec<ComponentAccess> {
    T::accesses(AccessMode::Write)
}

/// Helper trait for declaring component access sets.
pub trait ReadWriteSet {
    fn accesses(mode: AccessMode) -> Vec<ComponentAccess>;
}

macro_rules! impl_read_write_set {
    ($($T:ident),+) => {
        impl<$($T: 'static),+> ReadWriteSet for ($($T,)+) {
            fn accesses(mode: AccessMode) -> Vec<ComponentAccess> {
                vec![$(ComponentAccess { type_id: TypeId::of::<$T>(), mode }),+]
            }
        }
    };
}

impl_read_write_set!(A);
impl_read_write_set!(A, B);
impl_read_write_set!(A, B, C);
impl_read_write_set!(A, B, C, D);
impl_read_write_set!(A, B, C, D, E);
impl_read_write_set!(A, B, C, D, E, F);
impl_read_write_set!(A, B, C, D, E, F, G);
impl_read_write_set!(A, B, C, D, E, F, G, H);

/// Build a `SystemNode` with declared read and write sets enforced in the parallel executor.
pub fn system_node_with_access<S, R, W>(system: S) -> SystemNode
where
    S: System + 'static,
    R: ReadWriteSet,
    W: ReadWriteSet,
{
    let mut node = SystemNode::new(Box::new(system));
    for access in R::accesses(AccessMode::Read) {
        node = node.with_component_access(access);
    }
    for access in W::accesses(AccessMode::Write) {
        node = node.with_component_access(access);
    }
    node
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
