//! System scheduling — defines how game logic runs each frame.

use std::collections::{HashMap, HashSet, VecDeque};

use super::{Commands, World};

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
pub struct Schedule {
    stages: Vec<(SystemStage, Vec<Box<dyn System>>)>,
    /// Ordering constraints: `"A" -> ["B", "C"]` means A must run before B and C.
    before: HashMap<String, Vec<String>>,
}

impl Schedule {
    pub fn new() -> Self {
        Self {
            stages: vec![
                (SystemStage::PreUpdate, Vec::new()),
                (SystemStage::FixedUpdate, Vec::new()),
                (SystemStage::Update, Vec::new()),
                (SystemStage::PostUpdate, Vec::new()),
                (SystemStage::PreRender, Vec::new()),
                (SystemStage::Render, Vec::new()),
            ],
            before: HashMap::new(),
        }
    }

    /// Add a system to a specific stage.
    pub fn add_system(&mut self, stage: SystemStage, system: Box<dyn System>) {
        for (s, systems) in &mut self.stages {
            if *s == stage {
                systems.push(system);
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
    }

    /// Run all systems in stage order, flushing Commands between stages.
    ///
    /// Within each stage, systems are topologically sorted according to
    /// the constraints registered via [`add_order`].
    pub fn run(&mut self, world: &mut World) {
        for (_stage, systems) in &mut self.stages {
            let order = topo_sort_systems(systems, &self.before);
            for idx in order {
                world.begin_system(systems[idx].name());
                systems[idx].run(world);
                world.end_system(systems[idx].name());
            }
            // Flush Commands between stages
            if let Some(mut cmds) = world.remove_resource::<Commands>() {
                cmds.apply(world);
                world.insert_resource(cmds);
            }
        }
    }

    /// Run all systems with fixed-update substep loop for the `FixedUpdate` stage.
    ///
    /// Non-FixedUpdate stages run once. The FixedUpdate stage runs in a loop
    /// consuming accumulated time from [`crate::time::FixedUpdateAccumulator`].
    pub fn run_with_fixed_update(&mut self, world: &mut World, frame_delta: f32) {
        use crate::time::FixedUpdateAccumulator;

        for (stage, systems) in &mut self.stages {
            if *stage == SystemStage::FixedUpdate {
                // Accumulate frame delta and run fixed-rate substeps.
                let (acc, step) = if let Some(fua) = world.resource_mut::<FixedUpdateAccumulator>()
                {
                    fua.acc += frame_delta;
                    (fua.acc, fua.step)
                } else {
                    continue;
                };

                if step <= 0.0 || systems.is_empty() {
                    continue;
                }

                let mut remaining = acc;
                while remaining >= step {
                    let order = topo_sort_systems(systems, &self.before);
                    for idx in &order {
                        world.begin_system(systems[*idx].name());
                        systems[*idx].run(world);
                        world.end_system(systems[*idx].name());
                    }
                    if let Some(mut cmds) = world.remove_resource::<Commands>() {
                        cmds.apply(world);
                        world.insert_resource(cmds);
                    }
                    remaining -= step;
                }

                // Write back remaining accumulator.
                if let Some(fua) = world.resource_mut::<FixedUpdateAccumulator>() {
                    fua.acc = remaining;
                }
            } else {
                let order = topo_sort_systems(systems, &self.before);
                for idx in order {
                    world.begin_system(systems[idx].name());
                    systems[idx].run(world);
                    world.end_system(systems[idx].name());
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

impl Default for Schedule {
    fn default() -> Self {
        Self::new()
    }
}
