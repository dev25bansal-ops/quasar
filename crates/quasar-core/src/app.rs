//! Application builder and main loop.

use crate::ecs::{Schedule, SystemStage, World};
use crate::event::Events;
use crate::plugin::Plugin;
use crate::time::Time;

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
    /// System schedule.
    pub schedule: Schedule,
}

impl App {
    /// Create a new, empty application.
    pub fn new() -> Self {
        Self {
            world: World::new(),
            events: Events::new(),
            time: Time::new(),
            schedule: Schedule::new(),
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

        self.schedule.run(&mut self.world);

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
