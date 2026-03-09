//! Editor state management — Play/Pause/Stop + Undo/Redo.
//!
//! Implements:
//! - World snapshot on Play, restore on Stop
//! - Command stack for undo/redo of all inspector edits

use quasar_math::{Quat, Vec3};
use std::collections::VecDeque;

pub const MAX_UNDO_HISTORY: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorMode {
    Stopped,
    Playing,
    Paused,
    /// Isolated prefab editing sub-mode.
    PrefabEdit,
}

#[derive(Debug, Clone)]
pub struct WorldSnapshot {
    pub transforms: Vec<(u32, TransformData)>,
}

#[derive(Debug, Clone)]
pub struct TransformData {
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

pub trait EditCommand: std::fmt::Debug {
    fn execute(&self, world: &mut quasar_core::ecs::World);
    fn undo(&self, world: &mut quasar_core::ecs::World);
    fn description(&self) -> String;
}

#[derive(Debug, Clone)]
pub struct SetPositionCommand {
    pub entity: quasar_core::ecs::Entity,
    pub old_position: Vec3,
    pub new_position: Vec3,
}

impl EditCommand for SetPositionCommand {
    fn execute(&self, world: &mut quasar_core::ecs::World) {
        if let Some(transform) = world.get_mut::<quasar_math::Transform>(self.entity) {
            transform.position = self.new_position;
        }
    }

    fn undo(&self, world: &mut quasar_core::ecs::World) {
        if let Some(transform) = world.get_mut::<quasar_math::Transform>(self.entity) {
            transform.position = self.old_position;
        }
    }

    fn description(&self) -> String {
        format!("Set position of entity {:?}", self.entity)
    }
}

#[derive(Debug, Clone)]
pub struct SetRotationCommand {
    pub entity: quasar_core::ecs::Entity,
    pub old_rotation: Quat,
    pub new_rotation: Quat,
}

impl EditCommand for SetRotationCommand {
    fn execute(&self, world: &mut quasar_core::ecs::World) {
        if let Some(transform) = world.get_mut::<quasar_math::Transform>(self.entity) {
            transform.rotation = self.new_rotation;
        }
    }

    fn undo(&self, world: &mut quasar_core::ecs::World) {
        if let Some(transform) = world.get_mut::<quasar_math::Transform>(self.entity) {
            transform.rotation = self.old_rotation;
        }
    }

    fn description(&self) -> String {
        format!("Set rotation of entity {:?}", self.entity)
    }
}

#[derive(Debug, Clone)]
pub struct SetScaleCommand {
    pub entity: quasar_core::ecs::Entity,
    pub old_scale: Vec3,
    pub new_scale: Vec3,
}

impl EditCommand for SetScaleCommand {
    fn execute(&self, world: &mut quasar_core::ecs::World) {
        if let Some(transform) = world.get_mut::<quasar_math::Transform>(self.entity) {
            transform.scale = self.new_scale;
        }
    }

    fn undo(&self, world: &mut quasar_core::ecs::World) {
        if let Some(transform) = world.get_mut::<quasar_math::Transform>(self.entity) {
            transform.scale = self.old_scale;
        }
    }

    fn description(&self) -> String {
        format!("Set scale of entity {:?}", self.entity)
    }
}

#[derive(Debug, Clone)]
pub struct SetMaterialCommand {
    pub entity: quasar_core::ecs::Entity,
    pub old_base_color: [f32; 3],
    pub new_base_color: [f32; 3],
    pub old_roughness: f32,
    pub new_roughness: f32,
    pub old_metallic: f32,
    pub new_metallic: f32,
}

impl EditCommand for SetMaterialCommand {
    fn execute(&self, world: &mut quasar_core::ecs::World) {
        if let Some(material) = world.get_mut::<quasar_render::MaterialOverride>(self.entity) {
            material.base_color = self.new_base_color;
            material.roughness = self.new_roughness;
            material.metallic = self.new_metallic;
        }
    }

    fn undo(&self, world: &mut quasar_core::ecs::World) {
        if let Some(material) = world.get_mut::<quasar_render::MaterialOverride>(self.entity) {
            material.base_color = self.old_base_color;
            material.roughness = self.old_roughness;
            material.metallic = self.old_metallic;
        }
    }

    fn description(&self) -> String {
        format!("Set material of entity {:?}", self.entity)
    }
}

pub struct UndoStack {
    undo_stack: VecDeque<Box<dyn EditCommand>>,
    redo_stack: VecDeque<Box<dyn EditCommand>>,
}

impl UndoStack {
    pub fn new() -> Self {
        Self {
            undo_stack: VecDeque::with_capacity(MAX_UNDO_HISTORY),
            redo_stack: VecDeque::with_capacity(MAX_UNDO_HISTORY),
        }
    }

    pub fn push(&mut self, command: Box<dyn EditCommand>) {
        if self.undo_stack.len() >= MAX_UNDO_HISTORY {
            self.undo_stack.pop_front();
        }
        self.undo_stack.push_back(command);
        self.redo_stack.clear();
    }

    pub fn undo(&mut self, world: &mut quasar_core::ecs::World) -> Option<String> {
        if let Some(command) = self.undo_stack.pop_back() {
            let description = command.description();
            command.undo(world);
            self.redo_stack.push_back(command);
            Some(description)
        } else {
            None
        }
    }

    pub fn redo(&mut self, world: &mut quasar_core::ecs::World) -> Option<String> {
        if let Some(command) = self.redo_stack.pop_back() {
            let description = command.description();
            command.execute(world);
            self.undo_stack.push_back(command);
            Some(description)
        } else {
            None
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }
}

impl Default for UndoStack {
    fn default() -> Self {
        Self::new()
    }
}

pub struct EditorState {
    pub mode: EditorMode,
    pub snapshot: Option<WorldSnapshot>,
    pub undo_stack: UndoStack,
    /// When `true`, advance exactly one frame then set back to Paused.
    pub step_requested: bool,
    /// Multi-selection: currently selected entities.
    pub selected_entities: Vec<quasar_core::ecs::Entity>,
    /// Clipboard of copied entities for paste.
    pub clipboard_entities: Vec<quasar_core::ecs::Entity>,
    /// True while a prefab is being edited in isolation.
    pub prefab_isolation: bool,
    /// Optional PIE viewport camera override (position, target).
    pub pie_camera_override: Option<([f32; 3], [f32; 3])>,
}

impl EditorState {
    pub fn new() -> Self {
        Self {
            mode: EditorMode::Stopped,
            snapshot: None,
            undo_stack: UndoStack::new(),
            step_requested: false,
            selected_entities: Vec::new(),
            clipboard_entities: Vec::new(),
            prefab_isolation: false,
            pie_camera_override: None,
        }
    }

    pub fn play(&mut self, world: &quasar_core::ecs::World) {
        if self.mode == EditorMode::Stopped {
            self.snapshot = Some(self.take_snapshot(world));
            self.undo_stack.clear();
        }
        self.mode = EditorMode::Playing;
    }

    pub fn pause(&mut self) {
        if self.mode == EditorMode::Playing {
            self.mode = EditorMode::Paused;
        }
    }

    pub fn stop(&mut self, world: &mut quasar_core::ecs::World) {
        if let Some(snapshot) = self.snapshot.take() {
            self.restore_snapshot(world, snapshot);
        }
        self.mode = EditorMode::Stopped;
    }

    pub fn toggle_play(&mut self, world: &mut quasar_core::ecs::World) {
        match self.mode {
            EditorMode::Stopped => self.play(world),
            EditorMode::Playing => self.pause(),
            EditorMode::Paused => self.mode = EditorMode::Playing,
            EditorMode::PrefabEdit => {} // no-op while editing a prefab
        }
    }

    pub fn stop_and_restore(&mut self, world: &mut quasar_core::ecs::World) {
        self.stop(world);
    }

    /// Request a single-frame step while paused.
    pub fn step_frame(&mut self) {
        if self.mode == EditorMode::Paused {
            self.step_requested = true;
        }
    }

    /// Returns `true` when the simulation should tick this frame.
    /// Clears `step_requested` after one tick.
    pub fn should_tick(&mut self) -> bool {
        match self.mode {
            EditorMode::Playing => true,
            EditorMode::Paused if self.step_requested => {
                self.step_requested = false;
                true
            }
            _ => false,
        }
    }

    fn take_snapshot(&self, world: &quasar_core::ecs::World) -> WorldSnapshot {
        let transforms: Vec<(u32, TransformData)> = world
            .query::<quasar_math::Transform>()
            .into_iter()
            .map(|(e, t)| {
                (
                    e.index(),
                    TransformData {
                        position: [t.position.x, t.position.y, t.position.z],
                        rotation: [t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w],
                        scale: [t.scale.x, t.scale.y, t.scale.z],
                    },
                )
            })
            .collect();

        WorldSnapshot { transforms }
    }

    fn restore_snapshot(&self, world: &mut quasar_core::ecs::World, snapshot: WorldSnapshot) {
        for (entity_index, data) in snapshot.transforms {
            let entities: Vec<quasar_core::ecs::Entity> = world
                .query::<quasar_math::Transform>()
                .into_iter()
                .filter(|(e, _)| e.index() == entity_index)
                .map(|(e, _)| e)
                .collect();

            for e in entities {
                if let Some(transform) = world.get_mut::<quasar_math::Transform>(e) {
                    transform.position =
                        Vec3::new(data.position[0], data.position[1], data.position[2]);
                    transform.rotation = Quat::from_xyzw(
                        data.rotation[0],
                        data.rotation[1],
                        data.rotation[2],
                        data.rotation[3],
                    );
                    transform.scale = Vec3::new(data.scale[0], data.scale[1], data.scale[2]);
                }
            }
        }
    }

    pub fn execute_command(
        &mut self,
        command: Box<dyn EditCommand>,
        world: &mut quasar_core::ecs::World,
    ) {
        command.execute(world);
        self.undo_stack.push(command);
    }

    // ── Multi-selection ─────────────────────────────────────────

    /// Select a single entity, replacing any previous selection.
    pub fn select(&mut self, entity: quasar_core::ecs::Entity) {
        self.selected_entities.clear();
        self.selected_entities.push(entity);
    }

    /// Add an entity to the current selection (Ctrl+Click).
    pub fn select_add(&mut self, entity: quasar_core::ecs::Entity) {
        if !self.selected_entities.contains(&entity) {
            self.selected_entities.push(entity);
        }
    }

    /// Toggle an entity in the selection.
    pub fn select_toggle(&mut self, entity: quasar_core::ecs::Entity) {
        if let Some(pos) = self.selected_entities.iter().position(|&e| e == entity) {
            self.selected_entities.remove(pos);
        } else {
            self.selected_entities.push(entity);
        }
    }

    /// Clear the selection.
    pub fn deselect_all(&mut self) {
        self.selected_entities.clear();
    }

    /// Copy selected entities to internal clipboard.
    pub fn copy_selection(&mut self) {
        self.clipboard_entities = self.selected_entities.clone();
    }

    /// Paste entities from clipboard by cloning them.
    pub fn paste(&mut self, world: &mut quasar_core::ecs::World) {
        let sources: Vec<quasar_core::ecs::Entity> = self.clipboard_entities.clone();
        let mut new_selection = Vec::new();
        for src in sources {
            if let Some(cloned) = world.clone_entity(src) {
                new_selection.push(cloned);
            }
        }
        self.selected_entities = new_selection;
    }

    // ── Prefab isolation ────────────────────────────────────────

    /// Enter prefab editing mode, isolating the given entities.
    pub fn enter_prefab_edit(&mut self) {
        self.prefab_isolation = true;
        self.mode = EditorMode::PrefabEdit;
    }

    /// Exit prefab editing mode and return to the main scene.
    pub fn exit_prefab_edit(&mut self) {
        self.prefab_isolation = false;
        self.mode = EditorMode::Stopped;
    }
}

impl Default for EditorState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_stack_push_pop() {
        let mut stack = UndoStack::new();
        let entity = {
            let mut world = quasar_core::ecs::World::new();
            let e = world.spawn();
            world.insert(e, quasar_math::Transform::IDENTITY);
            e
        };
        let cmd = Box::new(SetPositionCommand {
            entity,
            old_position: Vec3::ZERO,
            new_position: Vec3::new(1.0, 2.0, 3.0),
        });

        stack.push(cmd);
        assert!(stack.can_undo());
        assert!(!stack.can_redo());
    }

    #[test]
    fn editor_state_mode_transitions() {
        let mut state = EditorState::new();
        assert_eq!(state.mode, EditorMode::Stopped);

        state.mode = EditorMode::Playing;
        assert_eq!(state.mode, EditorMode::Playing);

        state.pause();
        assert_eq!(state.mode, EditorMode::Paused);
    }
}
