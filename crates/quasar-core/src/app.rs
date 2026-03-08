//! Application builder and main loop.

use crate::ecs::{ParallelSchedule, Schedule, SystemNode, SystemStage, World};
use crate::event::Events;
use crate::plugin::Plugin;
use crate::time::{FixedUpdateAccumulator, Time};

/// A lightweight, clonable snapshot of frame timing data.
///
/// Inserted into the [`World`] every frame so systems can access timing
/// information via `world.resource::<TimeSnapshot>()`.
pub struct TimeSnapshot {
    /// Duration of the last frame in seconds.
    pub delta_seconds: f32,
    /// Total elapsed time since engine startup in seconds.
    pub elapsed_seconds: f32,
    /// Number of frames rendered so far.
    pub frame_count: u64,
}

/// The top-level application that ties the ECS, systems, and plugins together.
///
/// # Examples
/// ```ignore
/// App::new()
///     .add_plugin(WindowPlugin)
///     .add_plugin(RenderPlugin)
///     .add_system("player_movement", move_player)
///     .run();
/// ```
pub struct App {
    /// The ECS world containing all entities and components.
    pub world: World,
    /// Event bus for inter-system communication.
    pub events: Events,
    /// Frame timing information.
    pub time: Time,
    /// System schedule (sequential).
    pub schedule: Schedule,
    /// Optional parallel schedule for systems with declared access.
    pub parallel_schedule: Option<ParallelSchedule>,
}

impl App {
    /// Create a new, empty application.
    pub fn new() -> Self {
        Self {
            world: World::new(),
            events: Events::new(),
            time: Time::new(),
            schedule: Schedule::new(),
            parallel_schedule: None,
        }
    }

    /// Register a plugin.
    pub fn add_plugin(&mut self, plugin: impl Plugin) -> &mut Self {
        log::info!("Loading plugin: {}", plugin.name());
        plugin.build(self);
        self
    }

    /// Add a closure system to the Update stage.
    pub fn add_system(
        &mut self,
        name: impl Into<String>,
        func: impl FnMut(&mut World) + Send + Sync + 'static,
    ) -> &mut Self {
        self.schedule.add_system_fn(name, func);
        self
    }

    /// Add a system to a specific stage.
    pub fn add_system_to_stage(
        &mut self,
        stage: SystemStage,
        system: Box<dyn crate::ecs::System>,
    ) -> &mut Self {
        self.schedule.add_system(stage, system);
        self
    }

    /// Enable parallel system execution.
    ///
    /// Creates a [`ParallelSchedule`] that runs systems with declared
    /// component/resource access concurrently when safe to do so.
    /// Systems added via [`add_parallel_system`] use this schedule;
    /// systems added via [`add_system`] continue to run sequentially.
    pub fn enable_parallel(&mut self) -> &mut Self {
        if self.parallel_schedule.is_none() {
            self.parallel_schedule = Some(ParallelSchedule::new());
        }
        self
    }

    /// Add a system with declared access to the parallel schedule.
    ///
    /// Call [`enable_parallel`] first. If the parallel schedule is not
    /// enabled, the system's node is silently dropped.
    pub fn add_parallel_system(&mut self, stage: SystemStage, node: SystemNode) -> &mut Self {
        if let Some(ps) = &mut self.parallel_schedule {
            ps.add_system(stage, node);
        }
        self
    }

    /// Tick the application for one frame.
    ///
    /// This is called by the windowing backend each frame. It:
    /// 1. Updates the time
    /// 2. Syncs events from App.events into World as a resource
    /// 3. Inserts the current `Time` snapshot into the World as a resource
    /// 4. Runs all scheduled systems
    /// 5. Syncs events back from World to App.events
    /// 6. Clears frame events
    pub fn tick(&mut self) {
        self.time.update();

        // Sync events to world so systems can use world.resource::<Events>()
        self.world.insert_resource(std::mem::take(&mut self.events));

        // Make time accessible to systems via `world.resource::<Time>()`.
        self.world.insert_resource(TimeSnapshot {
            delta_seconds: self.time.delta_seconds(),
            elapsed_seconds: self.time.elapsed_seconds(),
            frame_count: self.time.frame_count(),
        });

        // Ensure a FixedUpdateAccumulator exists.
        if self.world.resource::<FixedUpdateAccumulator>().is_none() {
            self.world.insert_resource(FixedUpdateAccumulator::default());
        }

        self.schedule.run_with_fixed_update(&mut self.world, self.time.delta_seconds());

        // Run parallel schedule (if enabled) after sequential schedule.
        if let Some(ps) = &mut self.parallel_schedule {
            ps.run_with_fixed_update(&mut self.world, self.time.delta_seconds());
        }

        // Sync events back from world (systems may have added events)
        if let Some(events) = self.world.resource_mut::<Events>() {
            self.events = std::mem::take(events);
        }

        self.events.clear_all();
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
