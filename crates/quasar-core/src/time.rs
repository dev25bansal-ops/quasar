//! Time tracking — delta time, elapsed time, and fixed timestep support.

use std::time::{Duration, Instant};

/// Tracks frame timing information.
///
/// Updated once per frame by the engine's main loop. Systems use this to
/// make movement and animation frame-rate independent.
pub struct Time {
    /// Wall-clock instant when the engine started.
    startup: Instant,
    /// Instant at the start of the current frame.
    frame_start: Instant,
    /// Duration of the last frame (raw).
    delta: Duration,
    /// Smoothed delta for consistent gameplay (capped at `max_delta`).
    delta_seconds: f32,
    /// Total elapsed time since startup.
    elapsed: Duration,
    /// Maximum allowed delta — prevents spiral of death on lag spikes.
    max_delta: Duration,
    /// Accumulated frames.
    frame_count: u64,
}

impl Time {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            startup: now,
            frame_start: now,
            delta: Duration::ZERO,
            delta_seconds: 0.0,
            elapsed: Duration::ZERO,
            max_delta: Duration::from_millis(250), // 4 FPS floor
            frame_count: 0,
        }
    }

    /// Called at the start of each frame by the main loop.
    pub fn update(&mut self) {
        let now = Instant::now();
        let raw_delta = now - self.frame_start;
        self.delta = raw_delta.min(self.max_delta);
        self.delta_seconds = self.delta.as_secs_f32();
        self.frame_start = now;
        self.elapsed = now - self.startup;
        self.frame_count += 1;
    }

    /// Delta time as seconds (`f32`). Frame-rate independent.
    #[inline]
    pub fn delta_seconds(&self) -> f32 {
        self.delta_seconds
    }

    /// Raw delta duration.
    #[inline]
    pub fn delta(&self) -> Duration {
        self.delta
    }

    /// Total elapsed time since engine startup.
    #[inline]
    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }

    /// Total elapsed seconds since engine startup.
    #[inline]
    pub fn elapsed_seconds(&self) -> f32 {
        self.elapsed.as_secs_f32()
    }

    /// Number of frames rendered so far.
    #[inline]
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }
}

impl Default for Time {
    fn default() -> Self {
        Self::new()
    }
}
