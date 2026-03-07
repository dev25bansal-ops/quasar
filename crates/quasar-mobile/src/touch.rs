// crates/quasar-mobile/src/touch.rs
//! Multi-touch input tracking.

use glam::Vec2;

/// Maximum simultaneous touch pointers tracked.
pub const MAX_TOUCH_POINTERS: usize = 10;

/// Mirrors the lifecycle of a single finger contact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchPhase {
    Started,
    Moved,
    Ended,
    Cancelled,
}

/// A single finger / stylus pointer.
#[derive(Debug, Clone, Copy)]
pub struct TouchPointer {
    /// OS-provided touch identifier.
    pub id: u64,
    pub phase: TouchPhase,
    /// Current position in logical (dp) coordinates.
    pub position: Vec2,
    /// Position at the moment of `Started`.
    pub start_position: Vec2,
    /// Pressure (0.0–1.0) if reported; defaults to 1.0.
    pub pressure: f32,
}

impl Default for TouchPointer {
    fn default() -> Self {
        Self {
            id: u64::MAX,
            phase: TouchPhase::Ended,
            position: Vec2::ZERO,
            start_position: Vec2::ZERO,
            pressure: 1.0,
        }
    }
}

/// Aggregated touch state, updated once per frame.
#[derive(Debug, Clone)]
pub struct TouchInput {
    pointers: Vec<TouchPointer>,
}

impl Default for TouchInput {
    fn default() -> Self {
        Self {
            pointers: Vec::new(),
        }
    }
}

impl TouchInput {
    pub fn new() -> Self {
        Self::default()
    }

    /// Call at the start of each frame to promote ended / cancelled
    /// pointers out of the active set.
    pub fn begin_frame(&mut self) {
        self.pointers
            .retain(|p| p.phase != TouchPhase::Ended && p.phase != TouchPhase::Cancelled);
    }

    /// Feed a raw winit `Touch` event.
    pub fn handle_touch(&mut self, id: u64, phase: TouchPhase, x: f32, y: f32, pressure: f32) {
        if let Some(ptr) = self.pointers.iter_mut().find(|p| p.id == id) {
            ptr.phase = phase;
            ptr.position = Vec2::new(x, y);
            ptr.pressure = pressure;
        } else if phase == TouchPhase::Started && self.pointers.len() < MAX_TOUCH_POINTERS {
            let pos = Vec2::new(x, y);
            self.pointers.push(TouchPointer {
                id,
                phase,
                position: pos,
                start_position: pos,
                pressure,
            });
        }
    }

    /// Currently active pointer count.
    pub fn active_count(&self) -> usize {
        self.pointers
            .iter()
            .filter(|p| p.phase == TouchPhase::Started || p.phase == TouchPhase::Moved)
            .count()
    }

    /// Iterator over all tracked pointers (including just-ended).
    pub fn pointers(&self) -> &[TouchPointer] {
        &self.pointers
    }

    /// First active pointer, if any (convenience for single-touch use).
    pub fn primary(&self) -> Option<&TouchPointer> {
        self.pointers.first()
    }
}
