//! Plugin system — modular extensions for the engine.

use super::app::App;

/// A plugin adds functionality to the engine by registering systems,
/// resources, and events during the build phase.
///
/// # Examples
/// ```ignore
/// struct PhysicsPlugin;
///
/// impl Plugin for PhysicsPlugin {
///     fn build(&self, app: &mut App) {
///         app.add_system("physics_step", physics_step_system);
///     }
/// }
/// ```
pub trait Plugin: Send + Sync {
    /// Human-readable name for logging.
    fn name(&self) -> &str {
        std::any::type_name::<Self>()
    }

    /// Called once when the plugin is added to the [`App`].
    fn build(&self, app: &mut App);
}
