//! App State Machine — Game state management with transitions.
//!
//! Provides a state machine for managing game states (e.g., Menu, Playing, Paused, GameOver)
//! with automatic lifecycle hooks (OnEnter, OnUpdate, OnExit) and state transitions.
//!
//! # Examples
//! ```ignore
//! #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
//! enum GameState {
//!     #[default]
//!     Menu,
//!     Playing,
//!     Paused,
//!     GameOver,
//! }
//!
//! impl State for GameState {
//!     fn on_enter(&self, world: &mut World) {
//!         match self {
//!             GameState::Menu => log::info!("Entered Menu"),
//!             GameState::Playing => log::info!("Started Game"),
//!             GameState::Paused => log::info!("Paused"),
//!             GameState::GameOver => log::info!("Game Over"),
//!         }
//!     }
//!
//!     fn on_exit(&self, world: &mut World) {
//!         match self {
//!             GameState::Menu => log::info!("Exited Menu"),
//!             GameState::Playing => log::info!("Ended Game"),
//!             GameState::Paused => log::info!("Resumed"),
//!             GameState::GameOver => log::info!("Restarting"),
//!         }
//!     }
//! }
//!
//! // In your app setup:
//! app.add_state::<GameState>()
//!     .add_system_to_state(GameState::Playing, "update_player", update_player)
//!     .add_system_to_state(GameState::Menu, "update_menu", update_menu);
//!
//! // To transition states:
//! app.world.resource_mut::<StateManager<GameState>>().set(GameState::Playing);
//! ```

use std::marker::PhantomData;

use crate::ecs::{System, World};

/// Trait for defining game states with lifecycle hooks.
///
/// Implement this trait for your state enum to define behavior
/// when entering, updating, and exiting each state.
pub trait State:
    Default + Clone + Copy + PartialEq + Eq + std::hash::Hash + std::fmt::Debug + Send + Sync + 'static
{
    /// Called once when entering this state.
    ///
    /// Use this to initialize state-specific resources, spawn entities,
    /// or set up systems that should run only in this state.
    fn on_enter(&self, _world: &mut World) {}

    /// Called every frame while this state is active.
    ///
    /// Use this for state-specific logic that runs every frame.
    fn on_update(&self, _world: &mut World) {}

    /// Called once when exiting this state.
    ///
    /// Use this to clean up state-specific resources, despawn entities,
    /// or tear down systems that were specific to this state.
    fn on_exit(&self, _world: &mut World) {}
}

/// Event fired when a state transition occurs.
#[derive(Debug, Clone)]
pub struct StateTransition<S: State> {
    /// The state being transitioned from.
    pub from: S,
    /// The state being transitioned to.
    pub to: S,
}

/// Manages the current game state and handles transitions.
///
/// This resource is automatically inserted when you call `app.add_state::<S>()`.
pub struct StateManager<S: State> {
    /// The current active state.
    pub current: S,
    /// The previous state (before last transition).
    pub previous: Option<S>,
    /// Queue of pending state transitions.
    pub pending_transitions: Vec<S>,
    /// Whether a transition is in progress.
    transitioning: bool,
    /// Marker for state type.
    _marker: PhantomData<S>,
}

impl<S: State> StateManager<S> {
    /// Create a new state manager with the default state.
    pub fn new() -> Self {
        Self {
            current: S::default(),
            previous: None,
            pending_transitions: Vec::new(),
            transitioning: false,
            _marker: PhantomData,
        }
    }

    /// Get the current active state.
    pub fn current(&self) -> S {
        self.current
    }

    /// Get the previous state (before last transition).
    pub fn previous(&self) -> Option<S> {
        self.previous
    }

    /// Check if a specific state is currently active.
    pub fn is_active(&self, state: S) -> bool {
        self.current == state
    }

    /// Check if any of the given states are currently active.
    pub fn is_any_active(&self, states: &[S]) -> bool {
        states.contains(&self.current)
    }

    /// Request a state transition.
    ///
    /// The transition will be processed at the end of the current frame,
    /// allowing the current state's on_update to complete first.
    pub fn set(&mut self, new_state: S) {
        if new_state == self.current {
            return; // No transition needed
        }

        log::info!(
            "State transition requested: {:?} → {:?}",
            self.current,
            new_state
        );
        self.pending_transitions.push(new_state);
    }

    /// Request a state transition immediately (bypasses queue).
    ///
    /// Use with caution — this will interrupt the current state's update.
    pub fn set_immediate(&mut self, new_state: S) {
        if new_state == self.current {
            return;
        }

        log::info!(
            "Immediate state transition: {:?} → {:?}",
            self.current,
            new_state
        );
        self.pending_transitions.clear();
        self.pending_transitions.push(new_state);
        self.transitioning = true;
    }

    /// Revert to the previous state.
    pub fn revert(&mut self) {
        if let Some(prev) = self.previous {
            self.set(prev);
        }
    }

    /// Process any pending state transitions.
    ///
    /// This is called automatically by the state system each frame.
    pub fn process_transitions(&mut self, world: &mut World) {
        if self.pending_transitions.is_empty() {
            return;
        }

        let new_state = self.pending_transitions.remove(0);
        let old_state = self.current;

        // Exit current state
        self.current.on_exit(world);
        world.insert_resource(StateTransition {
            from: old_state,
            to: new_state,
        });

        // Update state
        self.previous = Some(old_state);
        self.current = new_state;

        // Enter new state
        self.current.on_enter(world);

        // Clear transition event
        world.remove_resource::<StateTransition<S>>();

        log::info!(
            "State transition complete: {:?} → {:?}",
            old_state,
            new_state
        );
    }

    /// Update the current state (called every frame).
    pub fn update(&self, world: &mut World) {
        self.current.on_update(world);
    }
}

impl<S: State> Default for StateManager<S> {
    fn default() -> Self {
        Self::new()
    }
}

/// System that runs state update hooks every frame.
pub struct StateUpdateSystem<S: State> {
    _marker: PhantomData<S>,
}

impl<S: State> StateUpdateSystem<S> {
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<S: State> System for StateUpdateSystem<S> {
    fn name(&self) -> &str {
        "state_update"
    }

    fn run(&mut self, world: &mut World) {
        // Get the current state and call on_update
        if let Some(manager) = world.resource::<StateManager<S>>() {
            let current_state = manager.current();
            // Drop the borrow before calling on_update
            drop(manager);
            current_state.on_update(world);
        }
    }
}

/// System that processes state transitions at the end of each frame.
pub struct StateTransitionSystem<S: State> {
    _marker: PhantomData<S>,
}

impl<S: State> StateTransitionSystem<S> {
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<S: State> System for StateTransitionSystem<S> {
    fn name(&self) -> &str {
        "state_transition"
    }

    fn run(&mut self, world: &mut World) {
        // Check if there are pending transitions
        let has_pending = world
            .resource::<StateManager<S>>()
            .map(|m| !m.pending_transitions.is_empty())
            .unwrap_or(false);

        if !has_pending {
            return;
        }

        // Process transitions with separate mutable borrows
        let (old_state, new_state) = {
            let manager = world
                .resource_mut::<StateManager<S>>()
                .expect("StateManager should exist");
            let new_state = manager.pending_transitions.remove(0);
            let old_state = manager.current;
            (old_state, new_state)
        };

        if old_state == new_state {
            return;
        }

        // Exit current state
        old_state.on_exit(world);

        // Insert transition event
        world.insert_resource(StateTransition {
            from: old_state,
            to: new_state,
        });

        // Update state manager
        {
            let manager = world
                .resource_mut::<StateManager<S>>()
                .expect("StateManager should exist");
            manager.previous = Some(old_state);
            manager.current = new_state;
        }

        // Enter new state
        new_state.on_enter(world);

        // Clear transition event
        world.remove_resource::<StateTransition<S>>();

        log::info!(
            "State transition complete: {:?} → {:?}",
            old_state,
            new_state
        );
    }
}

/// Extension trait for App to add state machine support.
pub trait AppExt {
    /// Add a state machine to the app.
    ///
    /// This registers the state manager resource and adds systems
    /// for state updates and transitions.
    fn add_state<S: State>(&mut self) -> &mut Self;

    /// Add a system that runs only when a specific state is active.
    fn add_system_to_state<S: State>(
        &mut self,
        state: S,
        name: impl Into<String>,
        func: impl FnMut(&mut World) + Send + Sync + 'static,
    ) -> &mut Self;

    /// Add a system that runs only when any of the given states are active.
    fn add_system_to_states<S: State>(
        &mut self,
        states: &[S],
        name: impl Into<String>,
        func: impl FnMut(&mut World) + Send + Sync + 'static,
    ) -> &mut Self;
}

/// Wrapper system that only runs when a specific state is active.
pub struct StateGuardedSystem<S: State> {
    pub state: S,
    pub inner: Box<dyn FnMut(&mut World) + Send + Sync>,
    pub name: String,
    pub _marker: PhantomData<S>,
}

impl<S: State> System for StateGuardedSystem<S> {
    fn name(&self) -> &str {
        &self.name
    }

    fn run(&mut self, world: &mut World) {
        if let Some(manager) = world.resource::<StateManager<S>>() {
            if manager.is_active(self.state) {
                (self.inner)(world);
            }
        }
    }
}

/// Wrapper system that only runs when any of the given states are active.
pub struct StatesGuardedSystem<S: State> {
    pub states: Vec<S>,
    pub inner: Box<dyn FnMut(&mut World) + Send + Sync>,
    pub name: String,
    pub _marker: PhantomData<S>,
}

impl<S: State> System for StatesGuardedSystem<S> {
    fn name(&self) -> &str {
        &self.name
    }

    fn run(&mut self, world: &mut World) {
        if let Some(manager) = world.resource::<StateManager<S>>() {
            if manager.is_any_active(&self.states) {
                (self.inner)(world);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
    enum TestState {
        #[default]
        Menu,
        Playing,
        Paused,
    }

    impl State for TestState {
        fn on_enter(&self, world: &mut World) {
            world.insert_resource(format!("Entered {:?}", self));
        }

        fn on_exit(&self, world: &mut World) {
            world.insert_resource(format!("Exited {:?}", self));
        }
    }

    #[test]
    fn test_state_manager_initial_state() {
        let manager = StateManager::<TestState>::new();
        assert_eq!(manager.current(), TestState::Menu);
        assert_eq!(manager.previous(), None);
    }

    #[test]
    fn test_state_transition() {
        let mut world = World::new();
        let mut manager = StateManager::<TestState>::new();

        // Request transition
        manager.set(TestState::Playing);

        // Process transition
        manager.process_transitions(&mut world);

        assert_eq!(manager.current(), TestState::Playing);
        assert_eq!(manager.previous(), Some(TestState::Menu));
    }

    #[test]
    fn test_state_no_transition_if_same() {
        let mut manager = StateManager::<TestState>::new();

        // Request transition to same state
        manager.set(TestState::Menu);

        assert!(manager.pending_transitions.is_empty());
        assert_eq!(manager.current(), TestState::Menu);
    }

    #[test]
    fn test_state_revert() {
        let mut world = World::new();
        let mut manager = StateManager::<TestState>::new();

        // Transition to Playing
        manager.set(TestState::Playing);
        manager.process_transitions(&mut world);

        // Transition to Paused
        manager.set(TestState::Paused);
        manager.process_transitions(&mut world);

        assert_eq!(manager.current(), TestState::Paused);
        assert_eq!(manager.previous(), Some(TestState::Playing));

        // Revert to previous
        manager.revert();
        manager.process_transitions(&mut world);

        assert_eq!(manager.current(), TestState::Playing);
    }

    #[test]
    fn test_is_active() {
        let manager = StateManager::<TestState>::new();

        assert!(manager.is_active(TestState::Menu));
        assert!(!manager.is_active(TestState::Playing));
        assert!(!manager.is_active(TestState::Paused));
    }

    #[test]
    fn test_is_any_active() {
        let manager = StateManager::<TestState>::new();

        assert!(manager.is_any_active(&[TestState::Menu, TestState::Playing]));
        assert!(!manager.is_any_active(&[TestState::Playing, TestState::Paused]));
    }

    #[test]
    fn test_state_on_enter_exit() {
        let mut world = World::new();
        let mut manager = StateManager::<TestState>::new();

        // Transition to Playing
        manager.set(TestState::Playing);
        manager.process_transitions(&mut world);

        // Check that on_enter was called
        assert!(world.resource::<String>().is_some());
        assert_eq!(world.resource::<String>().unwrap(), "Entered Playing");

        // Transition to Paused
        manager.set(TestState::Paused);
        manager.process_transitions(&mut world);

        // Check that on_exit and on_enter were called
        assert_eq!(world.resource::<String>().unwrap(), "Entered Paused");
    }

    #[test]
    fn test_state_guarded_system() {
        let mut world = World::new();
        let mut manager = StateManager::<TestState>::new();

        // Add counter resource
        world.insert_resource(0u32);

        // Create a system that increments counter
        let mut system = StateGuardedSystem {
            state: TestState::Playing,
            inner: Box::new(|world: &mut World| {
                if let Some(counter) = world.resource_mut::<u32>() {
                    *counter += 1;
                }
            }),
            name: "test_increment".to_string(),
            _marker: PhantomData,
        };

        // Run system while in Menu state — should not execute
        system.run(&mut world);
        assert_eq!(*world.resource::<u32>().unwrap(), 0);

        // Transition to Playing
        manager.set(TestState::Playing);
        manager.process_transitions(&mut world);
        world.insert_resource(manager);

        // Run system while in Playing state — should execute
        system.run(&mut world);
        assert_eq!(*world.resource::<u32>().unwrap(), 1);

        // Run again
        system.run(&mut world);
        assert_eq!(*world.resource::<u32>().unwrap(), 2);

        // Transition to Paused - need to get manager back from world
        let mut manager = world.remove_resource::<StateManager<TestState>>().unwrap();
        manager.set(TestState::Paused);
        manager.process_transitions(&mut world);
        world.insert_resource(manager);

        // Run system while in Paused state — should not execute
        system.run(&mut world);
        assert_eq!(*world.resource::<u32>().unwrap(), 2);
    }
}
