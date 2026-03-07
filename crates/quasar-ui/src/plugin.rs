//! UI plugin — integrates the UI tree, layout, and interaction
//! with the ECS world.

use quasar_core::ecs::{System, World};

use crate::layout::LayoutSolver;
use crate::widget::UiTree;

/// ECS resource that holds the UI tree and layout solver.
pub struct UiResource {
    pub tree: UiTree,
    pub layout: LayoutSolver,
    pub viewport_width: f32,
    pub viewport_height: f32,
}

impl UiResource {
    pub fn new(viewport_width: f32, viewport_height: f32) -> Self {
        Self {
            tree: UiTree::new(),
            layout: LayoutSolver::new(),
            viewport_width,
            viewport_height,
        }
    }
}

impl Default for UiResource {
    fn default() -> Self {
        Self::new(1280.0, 720.0)
    }
}

/// System that runs the layout solver each frame.
pub struct UiLayoutSystem;

impl System for UiLayoutSystem {
    fn name(&self) -> &str {
        "ui_layout"
    }

    fn run(&mut self, world: &mut World) {
        if let Some(ui) = world.resource_mut::<UiResource>() {
            let vw = ui.viewport_width;
            let vh = ui.viewport_height;
            ui.layout.solve(&ui.tree, vw, vh);
        }
    }
}

/// System that handles mouse interaction with UI nodes.
pub struct UiInteractionSystem;

impl System for UiInteractionSystem {
    fn name(&self) -> &str {
        "ui_interaction"
    }

    fn run(&mut self, world: &mut World) {
        // Read cursor position from window input resource if available.
        let (cursor_x, cursor_y, mouse_pressed) = world
            .resource::<CursorState>()
            .map(|c| (c.x, c.y, c.pressed))
            .unwrap_or((0.0, 0.0, false));

        if let Some(ui) = world.resource_mut::<UiResource>() {
            // Check all laid-out rects for interaction.
            let rects: Vec<(crate::widget::WidgetId, crate::layout::LayoutRect)> =
                ui.layout.rects().to_vec();

            for (id, rect) in rects {
                if let Some(node) = ui.tree.get_mut(id) {
                    let inside = rect.contains(cursor_x, cursor_y);
                    let was_pressed = node.interaction.pressed;
                    node.interaction.hovered = inside;
                    node.interaction.pressed = inside && mouse_pressed;
                    node.interaction.clicked = was_pressed && inside && !mouse_pressed;
                }
            }
        }
    }
}

/// Simple cursor state resource — should be fed from the window system.
#[derive(Debug, Clone, Copy, Default)]
pub struct CursorState {
    pub x: f32,
    pub y: f32,
    pub pressed: bool,
}

/// Plugin registering the UI systems.
pub struct UiPlugin;

impl quasar_core::Plugin for UiPlugin {
    fn name(&self) -> &str {
        "UiPlugin"
    }

    fn build(&self, app: &mut quasar_core::App) {
        app.world.insert_resource(UiResource::default());
        app.world.insert_resource(CursorState::default());

        app.schedule.add_system(
            quasar_core::ecs::SystemStage::PostUpdate,
            Box::new(UiLayoutSystem),
        );
        app.schedule.add_system(
            quasar_core::ecs::SystemStage::PostUpdate,
            Box::new(UiInteractionSystem),
        );

        log::info!("UiPlugin loaded — retained-mode UI active");
    }
}
