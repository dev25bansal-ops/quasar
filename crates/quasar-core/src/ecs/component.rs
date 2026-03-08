//! Component storage — marker trait for ECS components.

/// Marker trait for data that can be attached to entities.
///
/// Any `'static + Send + Sync` type automatically implements `Component`.
pub trait Component: 'static + Send + Sync {}

/// Blanket implementation: every `'static + Send + Sync` type is a component.
impl<T: 'static + Send + Sync> Component for T {}
