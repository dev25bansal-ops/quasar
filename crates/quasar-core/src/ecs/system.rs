//! System scheduling — defines how game logic runs each frame.

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
pub struct Schedule {
    stages: Vec<(SystemStage, Vec<Box<dyn System>>)>,
}

impl Schedule {
    pub fn new() -> Self {
        Self {
            stages: vec![
                (SystemStage::PreUpdate, Vec::new()),
                (SystemStage::Update, Vec::new()),
                (SystemStage::PostUpdate, Vec::new()),
                (SystemStage::PreRender, Vec::new()),
                (SystemStage::Render, Vec::new()),
            ],
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

    /// Run all systems in stage order, flushing Commands between stages.
    pub fn run(&mut self, world: &mut World) {
        for (_stage, systems) in &mut self.stages {
            for system in systems.iter_mut() {
                system.run(world);
            }
            // Flush Commands between stages
            if let Some(mut cmds) = world.remove_resource::<Commands>() {
                cmds.apply(world);
                world.insert_resource(cmds);
            }
        }
    }
}

impl Default for Schedule {
    fn default() -> Self {
        Self::new()
    }
}
