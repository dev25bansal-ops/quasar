//! Parallel system execution — run independent systems concurrently.
//!
//! Uses rayon thread pool to execute systems with no conflicting component
//! access in parallel. Systems must be thread-safe (Send + Sync).
//!
//! # Architecture
//!
//! The parallel scheduler works in four phases:
//!
//! 1. **Conflict Graph Construction** — Each system declares its read/write
//!    access on components and resources. The scheduler builds a conflict
//!    graph where edges represent data races.
//!
//! 2. **Topological Sorting** — Explicit `before`/`after` constraints and
//!    implicit data conflicts form a DAG. Kahn's algorithm computes a
//!    topological order.
//!
//! 3. **Parallel Batching** — Systems at the same topological level with
//!    no mutual conflicts are grouped into a `ParallelBatch`.
//!
//! 4. **Batch Execution** — Single-system batches run sequentially;
//!    multi-system batches execute via `rayon::scope`.
//!
//! # Feature Flag
//!
//! The `parallel` feature flag enables parallel execution in `Schedule::run()`.
//! Without it, all systems run sequentially. The `ParallelSchedule` type is
//! always available regardless of the feature flag.
//!
//! # Thread Safety
//!
//! The scheduler guarantees no data races by:
//! - Two systems writing the same component/resource → sequential
//! - One system writing, another reading the same → sequential
//! - Both systems only reading the same → parallel OK

use std::any::TypeId;
use std::collections::{HashMap, HashSet};
use std::hash::BuildHasherDefault;

use rayon;
use rustc_hash::FxHasher;
use smallvec::SmallVec;

use super::{Commands, System, SystemStage, World};

type FxHashSet<T> = HashSet<T, BuildHasherDefault<FxHasher>>;

// ===========================================================================
// SystemAccess — compact access declaration
// ===========================================================================

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

    /// Returns `true` if this access conflicts with another.
    ///
    /// A conflict exists when:
    /// - This writes something the other reads or writes
    /// - This reads something the other writes
    ///
    /// Two systems that only read the same data do NOT conflict.
    pub fn conflicts_with(&self, other: &SystemAccess) -> bool {
        // Check component conflicts
        for &write_type in &self.writes {
            if other.reads.contains(&write_type) || other.writes.contains(&write_type) {
                return true;
            }
        }
        for &read_type in &self.reads {
            if other.writes.contains(&read_type) {
                return true;
            }
        }

        // Check resource conflicts
        for &write_type in &self.resources_write {
            if other.resources_read.contains(&write_type)
                || other.resources_write.contains(&write_type)
            {
                return true;
            }
        }
        for &read_type in &self.resources_read {
            if other.resources_write.contains(&read_type) {
                return true;
            }
        }

        false
    }
}

// ===========================================================================
// DeclareAccess trait
// ===========================================================================

/// Trait for systems that declare their access via `SystemAccess`.
/// `SystemGraph` will auto-populate `SystemNode` fields from this.
pub trait DeclareAccess: System {
    fn access(&self) -> SystemAccess;
}

// ===========================================================================
// ComponentAccess — ergonomic access builder
// ===========================================================================

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

// ===========================================================================
// ParallelSystem trait
// ===========================================================================

pub trait ParallelSystem: System {
    fn component_access(&self) -> Vec<ComponentAccess>;
    fn resource_access(&self) -> Vec<ComponentAccess>;
}

// ===========================================================================
// SystemNode — a system with its access metadata and dependency edges
// ===========================================================================

pub struct SystemNode {
    pub system: Box<dyn System>,
    pub component_reads: FxHashSet<TypeId>,
    pub component_writes: FxHashSet<TypeId>,
    pub resource_reads: FxHashSet<TypeId>,
    pub resource_writes: FxHashSet<TypeId>,
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
            component_reads: FxHashSet::default(),
            component_writes: FxHashSet::default(),
            resource_reads: FxHashSet::default(),
            resource_writes: FxHashSet::default(),
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

    /// Determine whether this system has a data conflict with another.
    ///
    /// Two systems conflict if one writes something the other reads or writes.
    /// Mutual reads never conflict.
    #[inline]
    pub fn conflicts_with(&self, other: &SystemNode) -> bool {
        // Component writes vs other's reads/writes
        for &wt in &self.component_writes {
            if other.component_reads.contains(&wt) || other.component_writes.contains(&wt) {
                return true;
            }
        }
        // Component reads vs other's writes
        for &rt in &self.component_reads {
            if other.component_writes.contains(&rt) {
                return true;
            }
        }
        // Resource writes vs other's reads/writes
        for &wt in &self.resource_writes {
            if other.resource_reads.contains(&wt) || other.resource_writes.contains(&wt) {
                return true;
            }
        }
        // Resource reads vs other's writes
        for &rt in &self.resource_reads {
            if other.resource_writes.contains(&rt) {
                return true;
            }
        }
        false
    }

    /// Compute a `SystemAccess` snapshot from this node's metadata.
    pub fn to_system_access(&self) -> SystemAccess {
        let mut access = SystemAccess::new();
        access.reads = self.component_reads.iter().copied().collect();
        access.writes = self.component_writes.iter().copied().collect();
        access.resources_read = self.resource_reads.iter().copied().collect();
        access.resources_write = self.resource_writes.iter().copied().collect();
        access
    }
}

// ===========================================================================
// ConflictGraph — explicit conflict detection and resolution
// ===========================================================================

/// A conflict graph tracks which systems cannot run simultaneously.
///
/// Built from `SystemAccess` declarations, it provides an efficient
/// `has_conflict()` query and can produce maximal independent sets
/// for parallel batch construction.
#[derive(Debug, Default)]
pub struct ConflictGraph {
    /// Adjacency list: system index → set of conflicting system indices.
    edges: Vec<FxHashSet<usize>>,
}

impl ConflictGraph {
    pub fn new(num_systems: usize) -> Self {
        Self {
            edges: vec![FxHashSet::default(); num_systems],
        }
    }

    /// Record a conflict between systems `a` and `b`.
    pub fn add_conflict(&mut self, a: usize, b: usize) {
        self.edges[a].insert(b);
        self.edges[b].insert(a);
    }

    /// Check if systems `a` and `b` conflict.
    #[inline]
    pub fn has_conflict(&self, a: usize, b: usize) -> bool {
        self.edges[a].contains(&b)
    }

    /// Get the conflict set for a given system index.
    #[inline]
    pub fn conflicts_for(&self, index: usize) -> &FxHashSet<usize> {
        &self.edges[index]
    }

    /// Given a set of candidate indices, find all that can join a group
    /// without conflicting with any existing member.
    pub fn non_conflicting_with<'a>(
        &self,
        group: &[usize],
        candidates: impl Iterator<Item = &'a usize>,
    ) -> Vec<usize> {
        let group_set: FxHashSet<usize> = group.iter().copied().collect();
        candidates
            .filter(|&&c| {
                !group.iter().any(|&g| self.has_conflict(g, c))
                    && !group_set.contains(&c)
            })
            .copied()
            .collect()
    }
}

// ===========================================================================
// ParallelBatch — a group of systems that can safely run concurrently
// ===========================================================================

/// A batch of systems that have no mutual conflicts and can be executed
/// in parallel using rayon.
#[derive(Debug)]
pub struct ParallelBatch {
    /// Indices into the parent graph's node list.
    pub system_indices: Vec<usize>,
}

impl ParallelBatch {
    pub fn new(system_indices: Vec<usize>) -> Self {
        Self { system_indices }
    }

    pub fn len(&self) -> usize {
        self.system_indices.len()
    }

    pub fn is_parallel(&self) -> bool {
        self.system_indices.len() > 1
    }
}

// ===========================================================================
// SystemGraph — the full parallel scheduler for one stage
// ===========================================================================

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
        let node =
            system_node_with_access::<_, R, W>(crate::ecs::system::FnSystem::new(name, system));
        self.add_system(node)
    }

    /// Build the full dependency graph combining explicit ordering and
    /// auto-detected data conflicts.
    pub fn build_dependencies(&mut self) {
        let name_map: HashMap<String, usize> = self
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.system.name().to_string(), i))
            .collect();

        let n = self.nodes.len();

        // Phase 1: Apply explicit .after() / .before() constraints.
        let mut explicit_edges: Vec<(usize, usize)> = Vec::new();
        for i in 0..n {
            for after_name in &self.nodes[i].after.clone() {
                if let Some(&dep_idx) = name_map.get(after_name) {
                    explicit_edges.push((dep_idx, i));
                }
            }
            for before_name in &self.nodes[i].before.clone() {
                if let Some(&later_idx) = name_map.get(before_name) {
                    explicit_edges.push((i, later_idx));
                }
            }
        }
        for (earlier, later) in explicit_edges {
            self.nodes[later].dependencies.push(earlier);
            self.nodes[earlier].dependents.push(later);
        }

        // Phase 2: Build conflict graph for auto-detection.
        let mut conflict_graph = ConflictGraph::new(n);
        for i in 0..n {
            for j in (i + 1)..n {
                if self.nodes[i].conflicts_with(&self.nodes[j]) {
                    conflict_graph.add_conflict(i, j);
                }
            }
        }

        // Phase 3: For conflicting pairs without explicit ordering,
        // add a dependency edge (lower index → higher index) to enforce
        // sequential execution between them.
        for i in 0..n {
            for &j in &conflict_graph.edges[i] {
                if j > i {
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

    /// Perform topological grouping to produce parallel batches.
    ///
    /// Uses a modified Kahn's algorithm: at each level, collect all
    /// systems whose dependencies are satisfied, then greedily pack
    /// non-conflicting ones into the same batch.
    pub fn topological_groups(&self) -> Vec<ParallelBatch> {
        let n = self.nodes.len();
        if n == 0 {
            return Vec::new();
        }

        // Compute in-degrees.
        let mut in_degree = vec![0usize; n];
        for i in 0..n {
            for &dep in &self.nodes[i].dependencies {
                in_degree[dep] += 1;
            }
        }

        let mut completed = FxHashSet::default();
        let mut remaining: FxHashSet<usize> = (0..n).collect();
        let mut batches = Vec::new();

        while !remaining.is_empty() {
            // Find all systems ready to run (all dependencies completed).
            let mut ready: Vec<usize> = remaining
                .iter()
                .copied()
                .filter(|&idx| {
                    self.nodes[idx]
                        .dependencies
                        .iter()
                        .all(|dep| completed.contains(dep))
                })
                .collect();

            if ready.is_empty() {
                // Cycle detection: break by taking any remaining system.
                if let Some(&idx) = remaining.iter().next() {
                    ready.push(idx);
                } else {
                    break;
                }
            }

            // Sort ready list by index for deterministic ordering.
            ready.sort_unstable();

            // Greedily build batches: start with the first ready system,
            // then add any that don't conflict with current batch members.
            let mut batch_members: Vec<usize> = Vec::new();
            let mut batch_remaining: Vec<usize> = Vec::new();

            if let Some(&first) = ready.first() {
                batch_members.push(first);
            }

            for &idx in ready.iter().skip(1) {
                let conflicts = batch_members
                    .iter()
                    .any(|&b| self.nodes[b].conflicts_with(&self.nodes[idx]));
                if conflicts {
                    batch_remaining.push(idx);
                } else {
                    batch_members.push(idx);
                }
            }

            // The conflicting ready systems get added to a second batch
            // at the same level if they don't conflict with each other.
            if !batch_remaining.is_empty() {
                // Create additional batches for the remaining ready systems.
                let mut extra_batches = self._split_into_batches(batch_remaining);
                batches.push(ParallelBatch::new(batch_members));
                batches.append(&mut extra_batches);
            } else {
                batches.push(ParallelBatch::new(batch_members));
            }

            // Mark all batch members as completed.
            for batch in batches.iter() {
                for &idx in &batch.system_indices {
                    completed.insert(idx);
                    remaining.remove(&idx);
                }
            }
        }

        batches
    }

    /// Split a list of ready systems into batches where members don't conflict.
    fn _split_into_batches(&self, mut systems: Vec<usize>) -> Vec<ParallelBatch> {
        if systems.is_empty() {
            return Vec::new();
        }

        systems.sort_unstable();
        let mut batches: Vec<ParallelBatch> = Vec::new();
        let mut remaining = systems;

        while !remaining.is_empty() {
            let mut batch: Vec<usize> = Vec::new();
            let mut next_remaining: Vec<usize> = Vec::new();

            for &idx in &remaining {
                let conflicts = batch
                    .iter()
                    .any(|&b| self.nodes[b].conflicts_with(&self.nodes[idx]));
                if conflicts {
                    next_remaining.push(idx);
                } else {
                    batch.push(idx);
                }
            }

            if !batch.is_empty() {
                batches.push(ParallelBatch::new(batch));
            }
            remaining = next_remaining;
        }

        batches
    }

    /// Execute all systems in this graph, parallelizing where possible.
    pub fn run_parallel(&mut self, world: &mut World) {
        self.build_dependencies();
        let batches = self.topological_groups();

        for batch in batches {
            if batch.is_parallel() {
                self._run_batch_parallel(&batch.system_indices, world);
            } else if let Some(&idx) = batch.system_indices.first() {
                world.begin_system(self.nodes[idx].system.name());
                self.nodes[idx].system.run(world);
                world.end_system(self.nodes[idx].system.name());
            }
        }
    }

    /// Execute systems in a single batch using rayon::scope.
    fn _run_batch_parallel(&mut self, indices: &[usize], world: &mut World) {
        // Begin all systems in the batch.
        for &idx in indices {
            world.begin_system(self.nodes[idx].system.name());
        }

        // Encode pointers as usize for Send closure.
        let world_addr = world as *mut World as usize;
        let nodes_addr = self.nodes.as_mut_ptr() as usize;
        let indices_vec: Vec<usize> = indices.to_vec();

        // SAFETY:
        // 1. Each SystemNode pointer is unique across spawned tasks.
        // 2. The topological grouping guarantees no conflicting component/
        //    resource access within this batch, making concurrent &mut World
        //    access sound.
        // 3. rayon::scope ensures all spawned tasks complete before we
        //    continue, so the raw pointers remain valid.
        unsafe {
            rayon::scope(move |s| {
                for &idx in &indices_vec {
                    let node_addr = (nodes_addr as *mut SystemNode).add(idx) as usize;
                    let w_addr = world_addr;

                    s.spawn(move |_| {
                        let node = &mut *(node_addr as *mut SystemNode);
                        let world = &mut *(w_addr as *mut World);

                        // Set active_system_last_run for FilterChanged<T>.
                        let last_run = world.get_system_last_run(node.system.name());
                        world.set_active_system_last_run(last_run);

                        node.system.run(world);
                    });
                }
            });
        }

        // End all systems in the batch.
        for &idx in indices {
            world.end_system(self.nodes[idx].system.name());
        }
    }

    /// Execute all systems sequentially (fallback or when parallel is disabled).
    pub fn run_sequential(&mut self, world: &mut World) {
        // Ensure dependencies are built even for sequential mode.
        self.build_dependencies();

        let batches = self.topological_groups();
        for batch in batches {
            for &idx in &batch.system_indices {
                world.begin_system(self.nodes[idx].system.name());
                self.nodes[idx].system.run(world);
                world.end_system(self.nodes[idx].system.name());
            }
        }
    }

    /// Get the number of systems in this graph.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

// ===========================================================================
// ParallelSchedule — multi-stage parallel scheduler
// ===========================================================================

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

    /// Add a system that implements `DeclareAccess`; access is auto-detected.
    pub fn add_system_auto<S: DeclareAccess + 'static>(&mut self, stage: SystemStage, system: S) {
        if let Some(graph) = self.stages.get_mut(&stage) {
            graph.add_system_auto(system);
        }
    }

    /// Enable or disable parallel execution. When disabled, systems run
    /// sequentially but still respect explicit ordering constraints.
    pub fn set_parallel(&mut self, enabled: bool) {
        self.parallel_enabled = enabled;
    }

    pub fn is_parallel_enabled(&self) -> bool {
        self.parallel_enabled
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
                let (acc, step) = if let Some(fua) = world.resource_mut::<FixedUpdateAccumulator>()
                {
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

    /// Return the total number of systems across all stages.
    pub fn total_systems(&self) -> usize {
        self.stages.values().map(|g| g.len()).sum()
    }
}

impl Default for ParallelSchedule {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// Convenience constructors
// ===========================================================================

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

// ===========================================================================
// Macros
// ===========================================================================

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

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn make_system() -> Box<dyn System> {
        Box::new(crate::ecs::system::FnSystem::new("test", |_world| {}))
    }

    fn make_named_system(name: &str) -> Box<dyn System> {
        Box::new(crate::ecs::system::FnSystem::new(name, |_world| {}))
    }

    // --- SystemAccess tests ---

    #[test]
    fn system_access_write_read_conflict() {
        let a = SystemAccess::new().write::<i32>();
        let b = SystemAccess::new().read::<i32>();
        assert!(a.conflicts_with(&b));
        assert!(b.conflicts_with(&a));
    }

    #[test]
    fn system_access_write_write_conflict() {
        let a = SystemAccess::new().write::<i32>();
        let b = SystemAccess::new().write::<i32>();
        assert!(a.conflicts_with(&b));
    }

    #[test]
    fn system_access_read_read_no_conflict() {
        let a = SystemAccess::new().read::<i32>();
        let b = SystemAccess::new().read::<i32>();
        assert!(!a.conflicts_with(&b));
    }

    #[test]
    fn system_access_different_types_no_conflict() {
        let a = SystemAccess::new().write::<i32>();
        let b = SystemAccess::new().write::<String>();
        assert!(!a.conflicts_with(&b));
    }

    #[test]
    fn system_access_resource_conflict() {
        let a = SystemAccess::new().res_write::<i32>();
        let b = SystemAccess::new().res_read::<i32>();
        assert!(a.conflicts_with(&b));
    }

    #[test]
    fn system_access_resource_read_read_no_conflict() {
        let a = SystemAccess::new().res_read::<i32>();
        let b = SystemAccess::new().res_read::<i32>();
        assert!(!a.conflicts_with(&b));
    }

    #[test]
    fn system_access_cross_type_no_conflict() {
        // A reads Position, writes Velocity
        // B reads Velocity, writes Position
        // → Both write something the other reads → CONFLICT
        let a = SystemAccess::new().read::<i32>().write::<f32>();
        let b = SystemAccess::new().read::<f32>().write::<i32>();
        assert!(a.conflicts_with(&b));
    }

    // --- SystemNode tests ---

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
    fn system_node_both_write_conflict() {
        let node1 =
            SystemNode::new(make_system()).with_component_access(ComponentAccess::write::<i32>());
        let node2 =
            SystemNode::new(make_system()).with_component_access(ComponentAccess::write::<i32>());
        assert!(node1.conflicts_with(&node2));
    }

    #[test]
    fn system_node_different_components_no_conflict() {
        let node1 =
            SystemNode::new(make_system()).with_component_access(ComponentAccess::write::<i32>());
        let node2 =
            SystemNode::new(make_system()).with_component_access(ComponentAccess::write::<f32>());
        assert!(!node1.conflicts_with(&node2));
    }

    // --- ConflictGraph tests ---

    #[test]
    fn conflict_graph_basic() {
        let mut cg = ConflictGraph::new(4);
        cg.add_conflict(0, 1);
        cg.add_conflict(2, 3);

        assert!(cg.has_conflict(0, 1));
        assert!(cg.has_conflict(1, 0));
        assert!(cg.has_conflict(2, 3));
        assert!(!cg.has_conflict(0, 2));
        assert!(!cg.has_conflict(0, 3));
        assert!(!cg.has_conflict(1, 2));
    }

    #[test]
    fn conflict_graph_non_conflicting() {
        let mut cg = ConflictGraph::new(5);
        cg.add_conflict(0, 1);
        cg.add_conflict(0, 2);

        let group = vec![0];
        let candidates: Vec<usize> = (0..5).filter(|&x| x != 0).collect();
        let non_conflicting: Vec<usize> =
            cg.non_conflicting_with(&group, candidates.iter());

        // 3 and 4 don't conflict with 0; 1 and 2 do
        assert!(non_conflicting.contains(&3));
        assert!(non_conflicting.contains(&4));
        assert!(!non_conflicting.contains(&1));
        assert!(!non_conflicting.contains(&2));
    }

    // --- SystemGraph tests ---

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
        let batches = graph.topological_groups();

        // Both systems only read different types → no conflict → one batch
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].len(), 2);
    }

    #[test]
    fn topological_groups_with_conflict() {
        let mut graph = SystemGraph::new(SystemStage::Update);

        graph.add_system(
            SystemNode::new(make_named_system("writer"))
                .with_component_access(ComponentAccess::write::<i32>()),
        );
        graph.add_system(
            SystemNode::new(make_named_system("reader"))
                .with_component_access(ComponentAccess::read::<i32>()),
        );

        graph.build_dependencies();
        let batches = graph.topological_groups();

        // Writer and reader conflict → two separate batches
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].len(), 1);
        assert_eq!(batches[1].len(), 1);
    }

    #[test]
    fn topological_groups_partial_conflict() {
        // A reads X, B writes X (conflict), C reads Y (no conflict with either)
        let mut graph = SystemGraph::new(SystemStage::Update);

        graph.add_system(
            SystemNode::new(make_named_system("a_read_x"))
                .with_component_access(ComponentAccess::read::<i32>()),
        );
        graph.add_system(
            SystemNode::new(make_named_system("b_write_x"))
                .with_component_access(ComponentAccess::write::<i32>()),
        );
        graph.add_system(
            SystemNode::new(make_named_system("c_read_y"))
                .with_component_access(ComponentAccess::read::<f32>()),
        );

        graph.build_dependencies();
        let batches = graph.topological_groups();

        // a and b conflict; c has no conflict with either
        // Expected: one batch with {a, c}, one batch with {b}
        // OR: one batch with {a}, one with {b, c} depending on ordering
        assert_eq!(batches.len(), 2);

        let total: usize = batches.iter().map(|b| b.len()).sum();
        assert_eq!(total, 3);
    }

    #[test]
    fn topological_groups_three_parallel() {
        // Three systems reading different types → all parallel
        let mut graph = SystemGraph::new(SystemStage::Update);

        graph.add_system(
            SystemNode::new(make_named_system("read_a"))
                .with_component_access(ComponentAccess::read::<i32>()),
        );
        graph.add_system(
            SystemNode::new(make_named_system("read_b"))
                .with_component_access(ComponentAccess::read::<f32>()),
        );
        graph.add_system(
            SystemNode::new(make_named_system("read_c"))
                .with_component_access(ComponentAccess::read::<String>()),
        );

        graph.build_dependencies();
        let batches = graph.topological_groups();

        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].len(), 3);
        assert!(batches[0].is_parallel());
    }

    #[test]
    fn topological_groups_explicit_ordering() {
        // A must run before B, even though they don't conflict
        let mut graph = SystemGraph::new(SystemStage::Update);

        graph.add_system(
            SystemNode::new(make_named_system("system_a"))
                .with_component_access(ComponentAccess::read::<i32>()),
        );
        graph.add_system(
            SystemNode::new(make_named_system("system_b"))
                .before("system_c".to_string())
                .with_component_access(ComponentAccess::read::<f32>()),
        );
        graph.add_system(
            SystemNode::new(make_named_system("system_c"))
                .after("system_b".to_string())
                .with_component_access(ComponentAccess::read::<String>()),
        );

        graph.build_dependencies();
        let batches = graph.topological_groups();

        // a and b can be parallel; c must come after b
        assert_eq!(batches.len(), 2);
        // First batch: a and b (no conflict, no ordering between them)
        assert_eq!(batches[0].len(), 2);
        // Second batch: c (must be after b)
        assert_eq!(batches[1].len(), 1);
    }

    #[test]
    fn topological_groups_empty() {
        let graph = SystemGraph::new(SystemStage::Update);
        let batches = graph.topological_groups();
        assert!(batches.is_empty());
    }

    #[test]
    fn topological_groups_single_system() {
        let mut graph = SystemGraph::new(SystemStage::Update);
        graph.add_system(SystemNode::new(make_named_system("solo")));
        graph.build_dependencies();
        let batches = graph.topological_groups();

        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].len(), 1);
        assert!(!batches[0].is_parallel());
    }

    #[test]
    fn system_graph_len_and_is_empty() {
        let graph = SystemGraph::new(SystemStage::Update);
        assert!(graph.is_empty());
        assert_eq!(graph.len(), 0);
    }

    // --- SystemAccess ↔ SystemNode round-trip ---

    #[test]
    fn system_node_to_access_round_trip() {
        let node = SystemNode::new(make_system())
            .with_component_access(ComponentAccess::read::<i32>())
            .with_component_access(ComponentAccess::write::<f32>())
            .with_resource_access(ComponentAccess::read::<String>());

        let access = node.to_system_access();

        assert!(access.reads.contains(&TypeId::of::<i32>()));
        assert!(access.writes.contains(&TypeId::of::<f32>()));
        assert!(access.resources_read.contains(&TypeId::of::<String>()));
    }

    // --- ParallelBatch tests ---

    #[test]
    fn parallel_batch_is_parallel() {
        let batch = ParallelBatch::new(vec![0, 1, 2]);
        assert!(batch.is_parallel());
        assert_eq!(batch.len(), 3);
    }

    #[test]
    fn parallel_batch_not_parallel_single() {
        let batch = ParallelBatch::new(vec![0]);
        assert!(!batch.is_parallel());
        assert_eq!(batch.len(), 1);
    }

    #[test]
    fn parallel_batch_empty() {
        let batch = ParallelBatch::new(vec![]);
        assert!(!batch.is_parallel());
        assert_eq!(batch.len(), 0);
    }

    // --- ParallelSchedule tests ---

    #[test]
    fn parallel_schedule_default_parallel_enabled() {
        let schedule = ParallelSchedule::new();
        assert!(schedule.is_parallel_enabled());
    }

    #[test]
    fn parallel_schedule_toggle() {
        let mut schedule = ParallelSchedule::new();
        assert!(schedule.is_parallel_enabled());
        schedule.set_parallel(false);
        assert!(!schedule.is_parallel_enabled());
        schedule.set_parallel(true);
        assert!(schedule.is_parallel_enabled());
    }

    #[test]
    fn parallel_schedule_total_systems() {
        let mut schedule = ParallelSchedule::new();
        schedule.add_system(
            SystemStage::Update,
            SystemNode::new(make_named_system("a")),
        );
        schedule.add_system(
            SystemStage::Update,
            SystemNode::new(make_named_system("b")),
        );
        schedule.add_system(
            SystemStage::Render,
            SystemNode::new(make_named_system("c")),
        );

        assert_eq!(schedule.total_systems(), 3);
    }

    // --- Integration-style tests with World ---

    #[test]
    fn parallel_execution_produces_correct_results() {
        // Use shared atomic counters to verify both systems ran.
        let counter_a = Arc::new(AtomicUsize::new(0));
        let counter_b = Arc::new(AtomicUsize::new(0));

        let counter_a_clone = counter_a.clone();
        let counter_b_clone = counter_b.clone();

        let sys_a = crate::ecs::system::FnSystem::new("counter_a", move |_| {
            counter_a_clone.fetch_add(1, Ordering::SeqCst);
        });
        let sys_b = crate::ecs::system::FnSystem::new("counter_b", move |_| {
            counter_b_clone.fetch_add(1, Ordering::SeqCst);
        });

        let mut graph = SystemGraph::new(SystemStage::Update);
        graph.add_system(
            SystemNode::new(Box::new(sys_a))
                .with_component_access(ComponentAccess::read::<i32>()),
        );
        graph.add_system(
            SystemNode::new(Box::new(sys_b))
                .with_component_access(ComponentAccess::read::<f32>()),
        );

        let mut world = World::new();
        graph.run_parallel(&mut world);

        assert_eq!(counter_a.load(Ordering::SeqCst), 1);
        assert_eq!(counter_b.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn sequential_execution_respects_ordering() {
        // Verify sequential execution still respects dependency ordering.
        let order = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let order_a = order.clone();
        let order_b = order.clone();

        let sys_a = crate::ecs::system::FnSystem::new("first", move |_| {
            order_a.lock().unwrap().push("first");
        });
        let sys_b = crate::ecs::system::FnSystem::new("second", move |_| {
            order_b.lock().unwrap().push("second");
        });

        let mut graph = SystemGraph::new(SystemStage::Update);
        let idx_a = graph.add_system(
            SystemNode::new(Box::new(sys_a))
                .with_component_access(ComponentAccess::read::<i32>()),
        );
        let _idx_b = graph.add_system(
            SystemNode::new(Box::new(sys_b))
                .with_component_access(ComponentAccess::write::<i32>()),
        );

        // Manually add dependency: b depends on a (since they conflict,
        // this should happen automatically via build_dependencies)
        graph.nodes[_idx_b].dependencies.push(idx_a);

        let mut world = World::new();
        graph.run_sequential(&mut world);

        let final_order = order.lock().unwrap().clone();
        assert_eq!(final_order, vec!["first", "second"]);
    }

    #[test]
    fn add_fn_system_with_access() {
        let mut graph = SystemGraph::new(SystemStage::Update);
        graph.add_fn_system::<(i32,), (f32,)>("test_fn", |_| {});

        graph.build_dependencies();
        let batches = graph.topological_groups();

        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].len(), 1);
    }

    #[test]
    fn conflict_graph_symmetric() {
        let mut cg = ConflictGraph::new(3);
        cg.add_conflict(1, 2);

        assert!(cg.has_conflict(1, 2));
        assert!(cg.has_conflict(2, 1));
        assert!(!cg.has_conflict(0, 1));
    }
}
