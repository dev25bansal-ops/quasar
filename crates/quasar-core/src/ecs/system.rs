//! System scheduling — defines how game logic runs each frame.
//!
//! # Parallel Execution
//!
//! When the `parallel` feature is enabled (default), `Schedule::run()`
//! automatically detects systems with non-conflicting access declarations
//! and executes them concurrently using rayon's thread pool.
//!
//! Systems declare their read/write access on components and resources via
//! [`super::parallel::DeclareAccess`] or the [`super::parallel::SystemNode`]
//! builder. The scheduler builds a conflict graph and groups independent
//! systems into parallel batches.
//!
//! # Feature Flag
//!
//! Enable with `features = ["parallel"]` in `Cargo.toml`.
//! When disabled, all systems run sequentially but ordering constraints
//! are still respected.

use std::collections::{HashMap, HashSet, VecDeque};

use super::{Commands, World};

#[cfg(feature = "parallel")]
use super::parallel::{ConflictGraph, ParallelBatch, SystemAccess};

/// A system is a function that operates on the [`World`].
///
/// Systems are the "S" in ECS — they contain the game logic that reads and
/// writes component data.
pub trait System: Send + Sync {
    /// Human-readable name for debugging and profiling.
    fn name(&self) -> &str;

    /// Execute the system for one tick.
    fn run(&mut self, world: &mut World);
}

/// Wrapper allowing plain closures to be used as systems.
pub struct FnSystem<F: FnMut(&mut World) + Send + Sync> {
    name: String,
    func: F,
}

impl<F: FnMut(&mut World) + Send + Sync> FnSystem<F> {
    pub fn new(name: impl Into<String>, func: F) -> Self {
        Self {
            name: name.into(),
            func,
        }
    }
}

impl<F: FnMut(&mut World) + Send + Sync> System for FnSystem<F> {
    fn name(&self) -> &str {
        &self.name
    }

    fn run(&mut self, world: &mut World) {
        (self.func)(world);
    }
}

/// The stage at which a system should run within a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SystemStage {
    /// Runs before the main update (input processing, event dispatch).
    PreUpdate,
    /// Fixed-rate update for physics and deterministic simulation.
    FixedUpdate,
    /// Main game logic.
    Update,
    /// Runs after update (physics sync, transform propagation).
    PostUpdate,
    /// Rendering preparation.
    PreRender,
    /// Actual rendering.
    Render,
}

/// An ordered collection of systems grouped by stage.
///
/// Commands are flushed between stages to apply deferred mutations.
/// Systems within a stage can be ordered using `before` / `after` constraints.
///
/// # Parallel Execution
///
/// When the `parallel` feature is enabled, systems that declare their
/// access via [`super::parallel::DeclareAccess`] are automatically grouped
/// into parallel batches. Systems without access declarations fall back
/// to sequential execution within their stage.
///
/// Topological sort is computed once when systems are finalized (on first `run`)
/// and cached for reuse. Adding new systems or constraints invalidates the cache.
pub struct Schedule {
    stages: Vec<(SystemStage, Vec<Box<dyn System>>)>,
    /// Ordering constraints: `"A" -> ["B", "C"]` means A must run before B and C.
    before: HashMap<String, Vec<String>>,
    /// Cached topological order per stage. Computed on first `run`, invalidated on mutation.
    cached_orders: Vec<Option<Vec<usize>>>,
    /// Whether the cache is dirty (needs recomputation).
    cache_dirty: bool,
    /// Parallel execution flag. When enabled, systems with access declarations
    /// run concurrently where possible.
    #[cfg(feature = "parallel")]
    parallel_enabled: bool,
    /// System access declarations keyed by system name.
    /// Populated when systems implement `DeclareAccess`.
    #[cfg(feature = "parallel")]
    system_access: HashMap<String, SystemAccess>,
}

impl Schedule {
    pub fn new() -> Self {
        let stages = vec![
            (SystemStage::PreUpdate, Vec::new()),
            (SystemStage::FixedUpdate, Vec::new()),
            (SystemStage::Update, Vec::new()),
            (SystemStage::PostUpdate, Vec::new()),
            (SystemStage::PreRender, Vec::new()),
            (SystemStage::Render, Vec::new()),
        ];
        Self {
            stages,
            before: HashMap::new(),
            cached_orders: vec![None; 6],
            cache_dirty: false,
            #[cfg(feature = "parallel")]
            parallel_enabled: true,
            #[cfg(feature = "parallel")]
            system_access: HashMap::new(),
        }
    }

    /// Add a system to a specific stage.
    pub fn add_system(&mut self, stage: SystemStage, system: Box<dyn System>) {
        #[cfg(feature = "parallel")]
        {
            // If the system implements DeclareAccess, record its access pattern.
            // We can't downcast Box<dyn System> directly, so users should use
            // add_system_with_access for parallel scheduling.
        }

        for (si, (s, systems)) in &mut self.stages.iter_mut().enumerate() {
            if *s == stage {
                systems.push(system);
                self.cache_dirty = true;
                self.cached_orders[si] = None;
                return;
            }
        }
    }

    /// Add a system with explicit access declarations for parallel scheduling.
    ///
    /// This registers the system's read/write access so the parallel scheduler
    /// can detect conflicts and group independent systems into batches.
    ///
    /// # Example
    /// ```ignore
    /// use quasar_core::ecs::parallel::SystemAccess;
    ///
    /// schedule.add_system_with_access(
    ///     SystemStage::Update,
    ///     Box::new(my_system),
    ///     SystemAccess::new().read::<Position>().write::<Velocity>(),
    /// );
    /// ```
    #[cfg(feature = "parallel")]
    pub fn add_system_with_access(
        &mut self,
        stage: SystemStage,
        system: Box<dyn System>,
        access: SystemAccess,
    ) {
        let name = system.name().to_string();
        self.system_access.insert(name, access);

        for (si, (s, systems)) in &mut self.stages.iter_mut().enumerate() {
            if *s == stage {
                systems.push(system);
                self.cache_dirty = true;
                self.cached_orders[si] = None;
                return;
            }
        }
    }

    /// Add a closure as a system in the Update stage.
    pub fn add_system_fn(
        &mut self,
        name: impl Into<String>,
        func: impl FnMut(&mut World) + Send + Sync + 'static,
    ) {
        self.add_system(SystemStage::Update, Box::new(FnSystem::new(name, func)));
    }

    /// Add a closure as a system with explicit access declarations.
    ///
    /// # Example
    /// ```ignore
    /// schedule.add_system_fn_with_access(
    ///     "movement",
    ///     |world| { /* ... */ },
    ///     SystemAccess::new().read::<Position>().write::<Velocity>(),
    /// );
    /// ```
    #[cfg(feature = "parallel")]
    pub fn add_system_fn_with_access(
        &mut self,
        name: impl Into<String>,
        func: impl FnMut(&mut World) + Send + Sync + 'static,
        access: SystemAccess,
    ) {
        let name_str = name.into();
        self.add_system_with_access(
            SystemStage::Update,
            Box::new(FnSystem::new(&name_str, func)),
            access,
        );
    }

    /// Look up a system by name and return (stage_index, system_index) or None.
    pub fn find_system(&self, name: &str) -> Option<(usize, usize)> {
        for (si, (_stage, systems)) in self.stages.iter().enumerate() {
            for (idx, sys) in systems.iter().enumerate() {
                if sys.name() == name {
                    return Some((si, idx));
                }
            }
        }
        None
    }

    /// Declare that `before_name` must run before `after_name` within the same stage.
    pub fn add_order(&mut self, before_name: &str, after_name: &str) {
        self.before
            .entry(before_name.to_string())
            .or_default()
            .push(after_name.to_string());
        self.cache_dirty = true;
        // Invalidate all cached orders since constraints are global across stages.
        for cached in &mut self.cached_orders {
            *cached = None;
        }
    }

    /// Enable or disable parallel execution.
    ///
    /// When disabled, all systems run sequentially but ordering constraints
    /// are still respected. This is useful for debugging or when deterministic
    /// execution order is required.
    #[cfg(feature = "parallel")]
    pub fn set_parallel(&mut self, enabled: bool) {
        self.parallel_enabled = enabled;
    }

    /// Check if parallel execution is enabled.
    #[cfg(feature = "parallel")]
    pub fn is_parallel_enabled(&self) -> bool {
        self.parallel_enabled
    }

    /// Run all systems in stage order, flushing Commands between stages.
    ///
    /// Within each stage, systems are topologically sorted according to
    /// the constraints registered via `add_order`. Sorting is cached after
    /// the first computation and reused until systems or constraints change.
    ///
    /// # Parallel Execution
    ///
    /// When the `parallel` feature is enabled and systems have declared their
    /// access patterns, non-conflicting systems are executed concurrently
    /// using rayon's thread pool.
    pub fn run(&mut self, world: &mut World) {
        let num_stages = self.stages.len();
        for si in 0..num_stages {
            #[cfg(feature = "parallel")]
            {
                if self.parallel_enabled {
                    // Check if any systems in this stage have access declarations.
                    let systems = &self.stages[si].1;
                    let has_access = systems
                        .iter()
                        .any(|s| self.system_access.contains_key(s.name()));

                    if has_access {
                        self.run_stage_parallel(si, world);
                    } else {
                        self.run_stage_sequential(si, world);
                    }
                } else {
                    self.run_stage_sequential(si, world);
                }
            }
            #[cfg(not(feature = "parallel"))]
            {
                self.run_stage_sequential(si, world);
            }

            // Flush Commands between stages
            if let Some(mut cmds) = world.remove_resource::<Commands>() {
                cmds.apply(world);
                world.insert_resource(cmds);
            }
        }
    }

    /// Run a single stage sequentially.
    fn run_stage_sequential(&mut self, si: usize, world: &mut World) {
        // Compute or retrieve cached topological order.
        if self.cached_orders[si].is_none() {
            let systems = &self.stages[si].1;
            self.cached_orders[si] = Some(topo_sort_systems(systems, &self.before));
        }
        let order = self.cached_orders[si].as_ref().unwrap().clone();

        let systems = &mut self.stages[si].1;
        for idx in order {
            world.begin_system(systems[idx].name());
            systems[idx].run(world);
            world.end_system(systems[idx].name());
        }
    }

    /// Run a single stage with parallel batch execution.
    #[cfg(feature = "parallel")]
    fn run_stage_parallel(&mut self, si: usize, world: &mut World) {
        let systems = &mut self.stages[si].1;
        if systems.is_empty() {
            return;
        }

        let n = systems.len();

        // Build access array.
        let accesses: Vec<SystemAccess> = systems
            .iter()
            .map(|s| {
                self.system_access
                    .get(s.name())
                    .cloned()
                    .unwrap_or_default()
            })
            .collect();

        // Build name → index map for ordering constraints.
        let name_to_idx: HashMap<&str, usize> = systems
            .iter()
            .enumerate()
            .map(|(i, s)| (s.name(), i))
            .collect();

        // Build conflict graph.
        let mut conflict_graph = ConflictGraph::new(n);
        for i in 0..n {
            for j in (i + 1)..n {
                if accesses[i].conflicts_with(&accesses[j]) {
                    conflict_graph.add_conflict(i, j);
                }
            }
        }

        // Build ordering constraints as dependency edges.
        let mut dependencies: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (before_name, after_names) in &self.before {
            if let Some(&from) = name_to_idx.get(before_name.as_str()) {
                for after_name in after_names {
                    if let Some(&to) = name_to_idx.get(after_name.as_str()) {
                        dependencies[to].push(from); // to depends on from
                    }
                }
            }
        }

        // Add conflict-based dependencies for systems without explicit ordering.
        for i in 0..n {
            for &j in conflict_graph.conflicts_for(i) {
                if j > i {
                    if !dependencies[j].contains(&i) && !dependencies[i].contains(&j) {
                        dependencies[j].push(i);
                    }
                }
            }
        }

        // Compute parallel batches using topological grouping.
        let batches = self._compute_parallel_batches(n, &dependencies, &conflict_graph);

        // Execute batches.
        for batch in batches {
            if batch.is_parallel() {
                self._execute_parallel_batch(si, &batch.system_indices, world);
            } else if let Some(&idx) = batch.system_indices.first() {
                let systems = &mut self.stages[si].1;
                world.begin_system(systems[idx].name());
                systems[idx].run(world);
                world.end_system(systems[idx].name());
            }
        }
    }

    /// Compute parallel batches from dependencies and conflict graph.
    #[cfg(feature = "parallel")]
    fn _compute_parallel_batches(
        &self,
        n: usize,
        dependencies: &[Vec<usize>],
        conflict_graph: &ConflictGraph,
    ) -> Vec<ParallelBatch> {
        if n == 0 {
            return Vec::new();
        }

        // Compute in-degrees.
        let mut in_degree = vec![0usize; n];
        for i in 0..n {
            for &dep in &dependencies[i] {
                in_degree[dep] += 1;
            }
        }

        let mut completed: HashSet<usize> = HashSet::new();
        let mut remaining: HashSet<usize> = (0..n).collect();
        let mut batches: Vec<ParallelBatch> = Vec::new();

        while !remaining.is_empty() {
            // Find all ready systems (all dependencies completed).
            let mut ready: Vec<usize> = remaining
                .iter()
                .copied()
                .filter(|&idx| dependencies[idx].iter().all(|dep| completed.contains(dep)))
                .collect();

            if ready.is_empty() {
                // Cycle detected — break by taking any remaining system.
                if let Some(&idx) = remaining.iter().next() {
                    ready.push(idx);
                } else {
                    break;
                }
            }

            ready.sort_unstable();

            // Greedily pack non-conflicting systems into the first batch.
            let mut batch_members: Vec<usize> = Vec::new();
            let mut batch_remaining: Vec<usize> = Vec::new();

            if let Some(&first) = ready.first() {
                batch_members.push(first);
            }

            for &idx in ready.iter().skip(1) {
                let conflicts = batch_members
                    .iter()
                    .any(|&b| conflict_graph.has_conflict(b, idx));
                if conflicts {
                    batch_remaining.push(idx);
                } else {
                    batch_members.push(idx);
                }
            }

            batches.push(ParallelBatch::new(batch_members));

            // Split remaining into additional batches.
            let mut extra = self._split_ready_into_batches(batch_remaining, conflict_graph);
            batches.append(&mut extra);

            // Mark completed.
            for batch in batches.iter() {
                for &idx in &batch.system_indices {
                    completed.insert(idx);
                    remaining.remove(&idx);
                }
            }
        }

        batches
    }

    /// Split a set of ready systems into non-conflicting batches.
    #[cfg(feature = "parallel")]
    fn _split_ready_into_batches(
        &self,
        mut systems: Vec<usize>,
        conflict_graph: &ConflictGraph,
    ) -> Vec<ParallelBatch> {
        if systems.is_empty() {
            return Vec::new();
        }

        systems.sort_unstable();
        let mut batches = Vec::new();
        let mut remaining = systems;

        while !remaining.is_empty() {
            let mut batch = Vec::new();
            let mut next_remaining = Vec::new();

            for &idx in &remaining {
                let conflicts = batch.iter().any(|&b| conflict_graph.has_conflict(b, idx));
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

    /// Execute systems in a single batch using rayon's parallel iterator.
    #[cfg(feature = "parallel")]
    fn _execute_parallel_batch(&mut self, si: usize, indices: &[usize], world: &mut World) {
        // Begin all systems.
        let systems = &mut self.stages[si].1;
        for &idx in indices {
            world.begin_system(systems[idx].name());
        }

        // Extract mutable references to systems in this batch. Since each
        // system is unique and the batch has no conflicts, we can safely
        // run them in parallel.
        //
        // SAFETY: We collect raw pointers from unique &mut references,
        // encode them as usize (which is Send), pass them through
        // rayon::scope (which guarantees all tasks complete before returning),
        // and decode+dereference them exactly once.
        let systems = &mut self.stages[si].1;
        let batch_addrs: Vec<usize> = indices
            .iter()
            .map(|&idx| &mut systems[idx] as *mut Box<dyn System> as usize)
            .collect();

        let world_addr = world as *mut World as usize;

        // SAFETY:
        // 1. Each address in batch_addrs points to a unique Box<dyn System>.
        // 2. The conflict graph guarantees no conflicting component/resource
        //    access within this batch.
        // 3. rayon::scope ensures all spawned tasks complete before we continue,
        //    so the mutable borrow of self.stages remains valid.
        unsafe {
            rayon::scope(move |s| {
                for sys_addr in batch_addrs {
                    let w_addr = world_addr;
                    s.spawn(move |_| {
                        let system: &mut Box<dyn System> = &mut *(sys_addr as *mut Box<dyn System>);
                        let world: &mut World = &mut *(w_addr as *mut World);
                        system.run(world);
                    });
                }
            });
        }

        // End all systems.
        let systems = &self.stages[si].1;
        for &idx in indices {
            world.end_system(systems[idx].name());
        }
    }

    /// Run all systems with fixed-update substep loop for the `FixedUpdate` stage.
    ///
    /// Non-FixedUpdate stages run once. The FixedUpdate stage runs in a loop
    /// consuming accumulated time from [`crate::time::FixedUpdateAccumulator`].
    pub fn run_with_fixed_update(&mut self, world: &mut World, frame_delta: f32) {
        use crate::time::FixedUpdateAccumulator;

        let num_stages = self.stages.len();
        for si in 0..num_stages {
            let stage = self.stages[si].0;
            if stage == SystemStage::FixedUpdate {
                let (acc, step) = if let Some(fua) = world.resource_mut::<FixedUpdateAccumulator>()
                {
                    fua.acc += frame_delta;
                    (fua.acc, fua.step)
                } else {
                    continue;
                };

                if step <= 0.0 {
                    continue;
                }

                let mut remaining = acc;
                while remaining >= step {
                    #[cfg(feature = "parallel")]
                    {
                        if self.parallel_enabled {
                            let systems = &self.stages[si].1;
                            let has_access = systems
                                .iter()
                                .any(|s| self.system_access.contains_key(s.name()));
                            if has_access {
                                self.run_stage_parallel(si, world);
                            } else {
                                self.run_stage_sequential(si, world);
                            }
                        } else {
                            self.run_stage_sequential(si, world);
                        }
                    }
                    #[cfg(not(feature = "parallel"))]
                    {
                        self.run_stage_sequential(si, world);
                    }

                    if let Some(mut cmds) = world.remove_resource::<Commands>() {
                        cmds.apply(world);
                        world.insert_resource(cmds);
                    }
                    remaining -= step;
                }

                if let Some(fua) = world.resource_mut::<FixedUpdateAccumulator>() {
                    fua.acc = remaining;
                }
            } else {
                #[cfg(feature = "parallel")]
                {
                    if self.parallel_enabled {
                        let systems = &self.stages[si].1;
                        let has_access = systems
                            .iter()
                            .any(|s| self.system_access.contains_key(s.name()));
                        if has_access {
                            self.run_stage_parallel(si, world);
                        } else {
                            self.run_stage_sequential(si, world);
                        }
                    } else {
                        self.run_stage_sequential(si, world);
                    }
                }
                #[cfg(not(feature = "parallel"))]
                {
                    self.run_stage_sequential(si, world);
                }

                if let Some(mut cmds) = world.remove_resource::<Commands>() {
                    cmds.apply(world);
                    world.insert_resource(cmds);
                }
            }
        }
    }
}

/// Topological sort of systems within a stage based on `before` constraints.
/// Returns indices in execution order. Falls back to insertion order if no constraints apply.
fn topo_sort_systems(
    systems: &[Box<dyn System>],
    before: &HashMap<String, Vec<String>>,
) -> Vec<usize> {
    let n = systems.len();
    if n == 0 {
        return Vec::new();
    }

    // Build name → index map.
    let name_to_idx: HashMap<&str, usize> = systems
        .iter()
        .enumerate()
        .map(|(i, s)| (s.name(), i))
        .collect();

    // Build in-degree + adjacency list over indices.
    let mut in_degree = vec![0u32; n];
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];

    for (before_name, after_names) in before {
        let Some(&from) = name_to_idx.get(before_name.as_str()) else {
            continue;
        };
        for after_name in after_names {
            let Some(&to) = name_to_idx.get(after_name.as_str()) else {
                continue;
            };
            adj[from].push(to);
            in_degree[to] += 1;
        }
    }

    // Kahn's algorithm.
    let mut queue: VecDeque<usize> = VecDeque::new();
    for (i, &deg) in in_degree.iter().enumerate() {
        if deg == 0 {
            queue.push_back(i);
        }
    }

    let mut order = Vec::with_capacity(n);
    while let Some(idx) = queue.pop_front() {
        order.push(idx);
        for &next in &adj[idx] {
            in_degree[next] -= 1;
            if in_degree[next] == 0 {
                queue.push_back(next);
            }
        }
    }

    // If there's a cycle, append remaining indices in insertion order.
    if order.len() < n {
        let in_order: HashSet<usize> = order.iter().copied().collect();
        for i in 0..n {
            if !in_order.contains(&i) {
                order.push(i);
            }
        }
    }

    order
}

// ---------------------------------------------------------------------------
// SystemExecutor — compile-time SystemParam system runner
// ---------------------------------------------------------------------------

use super::system_param::{Access, SystemParam, SystemState};

/// A system that uses compile-time `SystemParam` for type-safe access.
///
/// This wraps a function that takes compile-time system parameters,
/// pre-computes the state during construction, and runs with zero per-frame
/// allocation.
///
/// # Type parameters
/// - `P` — The `SystemParam` tuple type for the system's parameters
/// - `F` — The system function type
pub struct SystemExecutor<P: SystemParam, F> {
    name: String,
    state: SystemState<P>,
    func: F,
    _marker: std::marker::PhantomData<P>,
}

impl<P, F> SystemExecutor<P, F>
where
    P: SystemParam + Send + Sync,
    F: for<'w, 's> FnMut(P::Item<'w, 's>) + Send + Sync + 'static,
{
    /// Create a new system executor with pre-computed state.
    ///
    /// # Example
    /// ```ignore
    /// use quasar_core::ecs::system_param::{Query, QueryMut, Res};
    ///
    /// let system = SystemExecutor::<(QueryMut<&mut Position>, Query<&Velocity>), _>::new(
    ///     "movement",
    ///     &mut world,
    ///     |(mut positions, velocities)| {
    ///         // system body
    ///     },
    /// );
    /// ```
    pub fn new(name: impl Into<String>, world: &mut World, func: F) -> Self {
        Self {
            name: name.into(),
            state: SystemState::new(world),
            func,
            _marker: std::marker::PhantomData,
        }
    }

    /// Get the compile-time access descriptor for this system.
    /// Used by the scheduler to build the dependency DAG.
    pub fn access(&self) -> Access {
        self.state.access()
    }
}

impl<P, F> super::System for SystemExecutor<P, F>
where
    P: SystemParam + Send + Sync,
    F: for<'w, 's> FnMut(P::Item<'w, 's>) + Send + Sync + 'static,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn run(&mut self, world: &mut World) {
        // SAFETY: The world reference is valid for the duration of this call.
        // The SystemParam's Item type encodes the borrow scope via lifetimes.
        // The scheduler ensures no conflicting borrows exist via the Access DAG.
        let params = unsafe { self.state.get_params(world) };
        (self.func)(params);
    }
}

/// Convenience function to create a system from a function with compile-time SystemParams.
///
/// # Example
/// ```ignore
/// use quasar_core::ecs::system_param::{Query, QueryMut, Res};
/// use quasar_core::ecs::system::{run_system, run_system_with};
///
/// // Using explicit type annotation
/// let system: Box<dyn System> = Box::new(run_system_with::<(Query<&Position>,), _>(
///     "read_positions",
///     &mut world,
///     |(positions,)| {
///         for (e, pos) in positions.iter() {
///             println!("{:?}: {:?}", e, pos);
///         }
///     },
/// ));
/// ```
pub fn run_system_with<P, F>(
    name: impl Into<String>,
    world: &mut World,
    func: F,
) -> SystemExecutor<P, F>
where
    P: SystemParam + Send + Sync,
    F: for<'w, 's> FnMut(P::Item<'w, 's>) + Send + Sync + 'static,
{
    SystemExecutor::new(name, world, func)
}

impl Default for Schedule {
    fn default() -> Self {
        Self::new()
    }
}
