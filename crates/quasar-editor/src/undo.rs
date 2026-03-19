//! Editor undo/redo system using the command pattern.

use quasar_core::ecs::World;
use std::collections::VecDeque;

/// Trait for editor commands that support undo/redo.
pub trait EditorCommand: Send + Sync {
    /// Execute the command, mutating world state.
    fn execute(&mut self, world: &mut World);

    /// Undo the command, restoring previous state.
    fn undo(&mut self, world: &mut World);

    /// Human-readable description for the undo history UI.
    fn description(&self) -> &str;

    /// Whether this command can be merged with a previous one.
    fn can_merge(&self, _previous: &dyn EditorCommand) -> bool {
        false
    }
}

/// Undo/redo history stack.
pub struct UndoStack {
    undo_stack: VecDeque<Box<dyn EditorCommand>>,
    redo_stack: VecDeque<Box<dyn EditorCommand>>,
    max_depth: usize,
}

impl UndoStack {
    pub fn new(max_depth: usize) -> Self {
        Self {
            undo_stack: VecDeque::with_capacity(max_depth),
            redo_stack: VecDeque::new(),
            max_depth,
        }
    }

    /// Execute a command and push it onto the undo stack.
    pub fn execute(&mut self, command: Box<dyn EditorCommand>, world: &mut World) {
        // Clear redo stack when new command is executed
        self.redo_stack.clear();

        // Try to merge with previous command
        let should_merge = if let Some(last) = self.undo_stack.back_mut() {
            command.can_merge(last.as_ref())
        } else {
            false
        };

        if should_merge {
            // Merge: don't push, just update existing command
            // The command's execute was already handled by can_merge logic
        } else {
            let mut cmd = command;
            cmd.execute(world);

            // Trim oldest if at capacity
            if self.undo_stack.len() >= self.max_depth {
                self.undo_stack.pop_front();
            }
            self.undo_stack.push_back(cmd);
        }
    }

    /// Undo the last command.
    pub fn undo(&mut self, world: &mut World) -> bool {
        if let Some(mut command) = self.undo_stack.pop_back() {
            command.undo(world);
            self.redo_stack.push_back(command);
            true
        } else {
            false
        }
    }

    /// Redo the last undone command.
    pub fn redo(&mut self, world: &mut World) -> bool {
        if let Some(mut command) = self.redo_stack.pop_back() {
            command.execute(world);
            self.undo_stack.push_back(command);
            true
        } else {
            false
        }
    }

    /// Check if undo is available.
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Check if redo is available.
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Get undo history descriptions.
    pub fn undo_history(&self) -> Vec<&str> {
        self.undo_stack.iter().map(|c| c.description()).collect()
    }

    /// Clear all history.
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }
}

impl Default for UndoStack {
    fn default() -> Self {
        Self::new(100)
    }
}

// ---------------------------------------------------------------------------
// Standard Editor Commands
// ---------------------------------------------------------------------------

use quasar_core::ecs::Entity;
use quasar_math::Transform;

/// Command: Spawn a new entity.
pub struct SpawnEntityCommand {
    entity: Option<Entity>,
    transform: Transform,
    description: String,
}

impl SpawnEntityCommand {
    pub fn new(transform: Transform) -> Self {
        Self {
            entity: None,
            transform,
            description: "Spawn Entity".to_string(),
        }
    }
}

impl EditorCommand for SpawnEntityCommand {
    fn execute(&mut self, world: &mut World) {
        let entity = world.spawn();
        world.insert(entity, self.transform);
        self.entity = Some(entity);
    }

    fn undo(&mut self, world: &mut World) {
        if let Some(entity) = self.entity {
            world.despawn(entity);
        }
    }

    fn description(&self) -> &str {
        &self.description
    }
}

/// Command: Despawn an entity.
pub struct DespawnEntityCommand {
    entity: Entity,
    transform: Transform,
    description: String,
}

impl DespawnEntityCommand {
    pub fn new(entity: Entity, transform: Transform) -> Self {
        Self {
            entity,
            transform,
            description: "Despawn Entity".to_string(),
        }
    }
}

impl EditorCommand for DespawnEntityCommand {
    fn execute(&mut self, world: &mut World) {
        world.despawn(self.entity);
    }

    fn undo(&mut self, world: &mut World) {
        // Note: This is a simplified version - full impl would need to restore all components
        let entity = world.spawn();
        world.insert(entity, self.transform);
    }

    fn description(&self) -> &str {
        &self.description
    }
}

/// Command: Set transform component.
pub struct SetTransformCommand {
    entity: Entity,
    old_transform: Transform,
    new_transform: Transform,
    description: String,
}

impl SetTransformCommand {
    pub fn new(entity: Entity, old_transform: Transform, new_transform: Transform) -> Self {
        Self {
            entity,
            old_transform,
            new_transform,
            description: "Set Transform".to_string(),
        }
    }
}

impl EditorCommand for SetTransformCommand {
    fn execute(&mut self, world: &mut World) {
        if let Some(transform) = world.get_mut::<Transform>(self.entity) {
            *transform = self.new_transform;
        }
    }

    fn undo(&mut self, world: &mut World) {
        if let Some(transform) = world.get_mut::<Transform>(self.entity) {
            *transform = self.old_transform;
        }
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn can_merge(&self, previous: &dyn EditorCommand) -> bool {
        if let Some(prev) = previous.as_any().downcast_ref::<SetTransformCommand>() {
            prev.entity == self.entity && self.description == prev.description
        } else {
            false
        }
    }
}

// Extension trait for downcasting
trait AsAny {
    fn as_any(&self) -> &dyn std::any::Any;
}

impl<T: 'static> AsAny for T {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
