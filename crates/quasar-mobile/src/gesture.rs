// crates/quasar-mobile/src/gesture.rs
//! Gesture recognition built on top of [`crate::touch::TouchInput`].

use glam::Vec2;

use crate::touch::{TouchInput, TouchPhase};

/// Cardinal swipe direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwipeDirection {
    Left,
    Right,
    Up,
    Down,
}

/// High-level gesture events emitted each frame.
#[derive(Debug, Clone)]
pub enum Gesture {
    /// A quick tap completed at this position.
    Tap(Vec2),
    /// A drag / swipe that exceeded the dead-zone.
    Swipe {
        direction: SwipeDirection,
        delta: Vec2,
        velocity: Vec2,
    },
    /// Two-finger pinch. `scale` is the ratio of current distance to
    /// initial distance (>1 = zoom in, <1 = zoom out).
    Pinch { center: Vec2, scale: f32 },
    /// Two-finger rotation in radians (positive = counter-clockwise).
    Rotate { center: Vec2, angle_rad: f32 },
}

/// Configuration thresholds for the recognizer.
#[derive(Debug, Clone)]
pub struct GestureConfig {
    /// Minimum drag distance (dp) before a tap becomes a swipe.
    pub swipe_threshold: f32,
    /// Maximum duration (seconds) for a touch to count as a "tap".
    pub tap_max_duration: f32,
}

impl Default for GestureConfig {
    fn default() -> Self {
        Self {
            swipe_threshold: 20.0,
            tap_max_duration: 0.3,
        }
    }
}

/// Stateful gesture recognizer. Feed it a [`TouchInput`] every frame.
#[derive(Debug, Clone, Default)]
pub struct GestureRecognizer {
    pub config: GestureConfig,
    // internal bookkeeping
    prev_pinch_dist: Option<f32>,
    prev_pinch_angle: Option<f32>,
    tap_start_time: Option<f32>,
}

impl GestureRecognizer {
    pub fn new(config: GestureConfig) -> Self {
        Self {
            config,
            ..Default::default()
        }
    }

    /// Analyse the current [`TouchInput`] and return any detected gestures.
    /// `dt` is the frame delta in seconds, `elapsed` is time since app start.
    pub fn update(&mut self, input: &TouchInput, dt: f32, elapsed: f32) -> Vec<Gesture> {
        let _ = dt;
        let mut out = Vec::new();
        let ptrs = input.pointers();

        // --- single-finger ---
        if ptrs.len() == 1 {
            self.prev_pinch_dist = None;
            self.prev_pinch_angle = None;

            let p = &ptrs[0];
            match p.phase {
                TouchPhase::Started => {
                    self.tap_start_time = Some(elapsed);
                }
                TouchPhase::Ended => {
                    let dist = p.position.distance(p.start_position);
                    let duration = self.tap_start_time.map(|t| elapsed - t).unwrap_or(f32::MAX);

                    if dist < self.config.swipe_threshold && duration < self.config.tap_max_duration
                    {
                        out.push(Gesture::Tap(p.position));
                    } else if dist >= self.config.swipe_threshold {
                        let delta = p.position - p.start_position;
                        let dir = if delta.x.abs() > delta.y.abs() {
                            if delta.x > 0.0 {
                                SwipeDirection::Right
                            } else {
                                SwipeDirection::Left
                            }
                        } else if delta.y > 0.0 {
                            SwipeDirection::Down
                        } else {
                            SwipeDirection::Up
                        };
                        let velocity = if duration > 0.0 {
                            delta / duration
                        } else {
                            Vec2::ZERO
                        };
                        out.push(Gesture::Swipe {
                            direction: dir,
                            delta,
                            velocity,
                        });
                    }
                    self.tap_start_time = None;
                }
                _ => {}
            }
        }

        // --- two-finger (pinch / rotate) ---
        if ptrs.len() >= 2 {
            self.tap_start_time = None;
            let a = ptrs[0].position;
            let b = ptrs[1].position;
            let dist = a.distance(b);
            let center = (a + b) * 0.5;
            let angle = (b.y - a.y).atan2(b.x - a.x);

            if let Some(prev_dist) = self.prev_pinch_dist {
                if prev_dist > 0.001 {
                    let scale = dist / prev_dist;
                    if (scale - 1.0).abs() > 0.005 {
                        out.push(Gesture::Pinch { center, scale });
                    }
                }
            }
            if let Some(prev_angle) = self.prev_pinch_angle {
                let mut delta_angle = angle - prev_angle;
                // wrap to [-PI, PI]
                if delta_angle > std::f32::consts::PI {
                    delta_angle -= 2.0 * std::f32::consts::PI;
                }
                if delta_angle < -std::f32::consts::PI {
                    delta_angle += 2.0 * std::f32::consts::PI;
                }
                if delta_angle.abs() > 0.002 {
                    out.push(Gesture::Rotate {
                        center,
                        angle_rad: delta_angle,
                    });
                }
            }

            self.prev_pinch_dist = Some(dist);
            self.prev_pinch_angle = Some(angle);
        }

        out
    }
}

// ---------------------------------------------------------------------------
// Gesture → Action Mapping
// ---------------------------------------------------------------------------

/// A named action that can be triggered by a gesture.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ActionName(pub String);

impl ActionName {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }
}

impl std::fmt::Display for ActionName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Mapping from gestures to named actions.
#[derive(Debug, Clone, Default)]
pub struct GestureActionMap {
    /// Single tap → action
    pub tap_action: Option<ActionName>,
    /// Swipe directions → actions
    pub swipe_left: Option<ActionName>,
    pub swipe_right: Option<ActionName>,
    pub swipe_up: Option<ActionName>,
    pub swipe_down: Option<ActionName>,
    /// Pinch → actions
    pub pinch_in: Option<ActionName>,
    pub pinch_out: Option<ActionName>,
    /// Rotate → action
    pub rotate: Option<ActionName>,
}

impl GestureActionMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the action for single taps.
    pub fn on_tap(mut self, action: impl Into<String>) -> Self {
        self.tap_action = Some(ActionName(action.into()));
        self
    }

    /// Set the action for swipe left.
    pub fn on_swipe_left(mut self, action: impl Into<String>) -> Self {
        self.swipe_left = Some(ActionName(action.into()));
        self
    }

    /// Set the action for swipe right.
    pub fn on_swipe_right(mut self, action: impl Into<String>) -> Self {
        self.swipe_right = Some(ActionName(action.into()));
        self
    }

    /// Set the action for swipe up.
    pub fn on_swipe_up(mut self, action: impl Into<String>) -> Self {
        self.swipe_up = Some(ActionName(action.into()));
        self
    }

    /// Set the action for swipe down.
    pub fn on_swipe_down(mut self, action: impl Into<String>) -> Self {
        self.swipe_down = Some(ActionName(action.into()));
        self
    }

    /// Set the action for pinch in (zoom out).
    pub fn on_pinch_in(mut self, action: impl Into<String>) -> Self {
        self.pinch_in = Some(ActionName(action.into()));
        self
    }

    /// Set the action for pinch out (zoom in).
    pub fn on_pinch_out(mut self, action: impl Into<String>) -> Self {
        self.pinch_out = Some(ActionName(action.into()));
        self
    }

    /// Set the action for rotation.
    pub fn on_rotate(mut self, action: impl Into<String>) -> Self {
        self.rotate = Some(ActionName(action.into()));
        self
    }

    /// Convert detected gestures to actions.
    pub fn map(&self, gestures: &[Gesture]) -> Vec<(ActionName, f32)> {
        let mut actions = Vec::new();

        for gesture in gestures {
            match gesture {
                Gesture::Tap(_) => {
                    if let Some(ref action) = self.tap_action {
                        actions.push((action.clone(), 1.0));
                    }
                }
                Gesture::Swipe { direction, .. } => {
                    let action = match direction {
                        SwipeDirection::Left => &self.swipe_left,
                        SwipeDirection::Right => &self.swipe_right,
                        SwipeDirection::Up => &self.swipe_up,
                        SwipeDirection::Down => &self.swipe_down,
                    };
                    if let Some(ref action) = action {
                        actions.push((action.clone(), 1.0));
                    }
                }
                Gesture::Pinch { scale, .. } => {
                    if *scale < 1.0 {
                        if let Some(ref action) = self.pinch_in {
                            actions.push((action.clone(), 1.0 - scale));
                        }
                    } else if let Some(ref action) = self.pinch_out {
                        actions.push((action.clone(), scale - 1.0));
                    }
                }
                Gesture::Rotate { angle_rad, .. } => {
                    if let Some(ref action) = self.rotate {
                        actions.push((action.clone(), angle_rad.abs()));
                    }
                }
            }
        }

        actions
    }
}

/// Bridge that connects gesture recognition to the engine's action system.
///
/// This struct holds a `GestureRecognizer` and a `GestureActionMap`,
/// providing a one-stop interface for mobile games to process touch input
/// and receive named actions each frame.
#[derive(Debug, Clone)]
pub struct GestureActionBridge {
    pub recognizer: GestureRecognizer,
    pub action_map: GestureActionMap,
}

impl GestureActionBridge {
    /// Create a new bridge with default configuration.
    pub fn new() -> Self {
        Self {
            recognizer: GestureRecognizer::new(GestureConfig::default()),
            action_map: GestureActionMap::new(),
        }
    }

    /// Create a bridge with custom configuration.
    pub fn with_config(config: GestureConfig) -> Self {
        Self {
            recognizer: GestureRecognizer::new(config),
            action_map: GestureActionMap::new(),
        }
    }

    /// Set the action map.
    pub fn with_action_map(mut self, map: GestureActionMap) -> Self {
        self.action_map = map;
        self
    }

    /// Process touch input for this frame and return triggered actions.
    ///
    /// # Arguments
    /// * `input` - The current touch input state
    /// * `dt` - Delta time in seconds
    /// * `elapsed` - Total elapsed time since app start
    ///
    /// # Returns
    /// A list of `(action_name, magnitude)` pairs. Magnitude depends on
    /// the gesture type (e.g., pinch magnitude = scale delta, rotate = angle).
    pub fn update(&mut self, input: &TouchInput, dt: f32, elapsed: f32) -> Vec<(ActionName, f32)> {
        let gestures = self.recognizer.update(input, dt, elapsed);
        self.action_map.map(&gestures)
    }
}

impl Default for GestureActionBridge {
    fn default() -> Self {
        Self::new()
    }
}
