//! ActionMap plugin — processes input and fires action events.
//!
//! Wire up the ActionMap system to convert raw input into named action events.

use quasar_core::ecs::{System, World};
use quasar_core::Events;

use crate::{ActionMap, Input};

/// An action that was activated this frame.
#[derive(Debug, Clone)]
pub struct ActionEvent {
    /// The action name.
    pub action: String,
    /// Whether it was just pressed, held, or just released.
    pub state: ActionState,
}

/// The state of an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionState {
    /// Action was pressed this frame.
    Pressed,
    /// Action is being held.
    Held,
    /// Action was released this frame.
    Released,
}

/// System that reads Input and ActionMap, then fires ActionEvent.
pub struct ActionMapSystem;

impl System for ActionMapSystem {
    fn name(&self) -> &str {
        "action_map"
    }

    fn run(&mut self, world: &mut World) {
        // Collect all actions with their bindings first
        let actions_to_check: Vec<(String, Vec<crate::InputBinding>)> = {
            let Some(action_map) = world.resource::<ActionMap>() else {
                return;
            };
            let Some(_input) = world.resource::<Input>() else {
                return;
            };
            action_map
                .bindings
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        };

        // Early return if no actions
        if actions_to_check.is_empty() {
            return;
        }

        let Some(input) = world.resource::<Input>() else {
            return;
        };

        // Collect events to send
        let mut events_to_send: Vec<ActionEvent> = Vec::new();

        for (action, bindings) in actions_to_check {
            for binding in bindings {
                let just_pressed = match binding {
                    crate::InputBinding::Key(k) => input.just_pressed(k),
                    crate::InputBinding::Mouse(m) => input.mouse_just_pressed(m),
                };

                let just_released = match binding {
                    crate::InputBinding::Key(k) => input.just_released(k),
                    crate::InputBinding::Mouse(m) => input.mouse_just_released(m),
                };

                if just_pressed {
                    events_to_send.push(ActionEvent {
                        action: action.clone(),
                        state: ActionState::Pressed,
                    });
                }
                if just_released {
                    events_to_send.push(ActionEvent {
                        action: action.clone(),
                        state: ActionState::Released,
                    });
                }
            }
        }

        // Send events
        if !events_to_send.is_empty() {
            if let Some(events) = world.resource_mut::<Events>() {
                for event in events_to_send {
                    events.send(event);
                }
            }
        }
    }
}

/// Plugin that registers ActionMap processing.
pub struct ActionMapPlugin;

impl ActionMapPlugin {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ActionMapPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl quasar_core::Plugin for ActionMapPlugin {
    fn name(&self) -> &str {
        "ActionMapPlugin"
    }

    fn build(&self, app: &mut quasar_core::App) {
        // Insert default ActionMap if not present
        if !app.world.has_resource::<ActionMap>() {
            app.world.insert_resource(ActionMap::new());
        }

        // Add system in PreUpdate to process input before game logic
        app.schedule.add_system(
            quasar_core::ecs::SystemStage::PreUpdate,
            Box::new(ActionMapSystem),
        );

        log::info!("ActionMapPlugin loaded — action input processing active");
    }
}
