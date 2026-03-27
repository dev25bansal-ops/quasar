//! ECS components for rendering.

/// A texture index for per-entity texture selection.
///
/// When attached to an entity alongside `MeshShape`, the entity will be rendered
/// with the texture at this index in the renderer's texture cache.
#[derive(Debug, Clone, Copy, Default)]
pub struct TextureHandle {
    pub index: u32,
}

impl TextureHandle {
    pub fn new(index: u32) -> Self {
        Self { index }
    }
}
