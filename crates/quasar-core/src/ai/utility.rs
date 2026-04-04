//! Utility AI System for emergent behavior.
//!
//! Provides:
//! - Score-based action selection
//! - Multiple consideration types
//! - Curve-based scoring
//! - Context-sensitive decisions

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Utility score (0.0 to 1.0).
pub type UtilityScore = f32;

/// Utility action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UtilityAction {
    /// Action ID.
    pub id: String,
    /// Action name.
    pub name: String,
    /// Considerations (all must score).
    pub considerations: Vec<Consideration>,
    /// Weight multiplier.
    pub weight: f32,
    /// Cooldown in seconds.
    pub cooldown: f32,
    /// Minimum score to execute.
    pub min_score: f32,
    /// Action type for execution.
    pub action_type: UtilityActionType,
}

impl UtilityAction {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: String::new(),
            considerations: Vec::new(),
            weight: 1.0,
            cooldown: 0.0,
            min_score: 0.0,
            action_type: UtilityActionType::Custom { id: String::new() },
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    pub fn add_consideration(mut self, consideration: Consideration) -> Self {
        self.considerations.push(consideration);
        self
    }

    pub fn with_weight(mut self, weight: f32) -> Self {
        self.weight = weight;
        self
    }

    pub fn with_cooldown(mut self, cooldown: f32) -> Self {
        self.cooldown = cooldown;
        self
    }

    pub fn with_min_score(mut self, min_score: f32) -> Self {
        self.min_score = min_score;
        self
    }

    /// Calculate utility score for this action.
    pub fn calculate_score(&self, context: &UtilityContext) -> f32 {
        if self.considerations.is_empty() {
            return self.weight;
        }

        // Multiply all consideration scores
        let mut score = 1.0f32;
        for consideration in &self.considerations {
            score *= consideration.calculate_score(context);
            if score <= 0.0 {
                break;
            }
        }

        score * self.weight
    }
}

/// Action type for execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UtilityActionType {
    MoveTo { target_key: String },
    Attack { target_key: String },
    Flee { safe_distance: f32 },
    UseAbility { ability_id: String },
    Interact { object_key: String },
    Wait { duration: f32 },
    Patrol { patrol_id: String },
    Custom { id: String },
}

/// Consideration for scoring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Consideration {
    /// Consideration type.
    pub consideration_type: ConsiderationType,
    /// Response curve.
    pub curve: ResponseCurve,
    /// Invert the score.
    pub invert: bool,
    /// Bonus/malus to add.
    pub bonus: f32,
}

impl Consideration {
    pub fn new(consideration_type: ConsiderationType) -> Self {
        Self {
            consideration_type,
            curve: ResponseCurve::Linear,
            invert: false,
            bonus: 0.0,
        }
    }

    pub fn with_curve(mut self, curve: ResponseCurve) -> Self {
        self.curve = curve;
        self
    }

    pub fn inverted(mut self) -> Self {
        self.invert = true;
        self
    }

    pub fn with_bonus(mut self, bonus: f32) -> Self {
        self.bonus = bonus;
        self
    }

    /// Calculate score for this consideration.
    pub fn calculate_score(&self, context: &UtilityContext) -> f32 {
        let raw_score = match &self.consideration_type {
            ConsiderationType::Health => context.health,
            ConsiderationType::HealthPercent => context.health_percent,
            ConsiderationType::DistanceToTarget => context
                .distance_to_target
                .map(|d| 1.0 / (d + 1.0))
                .unwrap_or(0.0),
            ConsiderationType::DistanceToTargetNormalized { max_distance } => context
                .distance_to_target
                .map(|d| 1.0 - (d / max_distance).min(1.0))
                .unwrap_or(0.0),
            ConsiderationType::HasTarget => {
                if context.has_target {
                    1.0
                } else {
                    0.0
                }
            }
            ConsiderationType::TargetHealth => context.target_health.unwrap_or(0.0),
            ConsiderationType::TargetDistance => context
                .target_distance
                .map(|d| 1.0 / (d + 1.0))
                .unwrap_or(0.0),
            ConsiderationType::Ammo => context.ammo as f32 / 100.0,
            ConsiderationType::AmmoPercent => context.ammo_percent,
            ConsiderationType::Cooldown { id } => context
                .cooldowns
                .get(id)
                .map(|&t| if t <= 0.0 { 1.0 } else { 0.0 })
                .unwrap_or(1.0),
            ConsiderationType::ThreatLevel => context.threat_level,
            ConsiderationType::AllyCount => (context.ally_count as f32 / 10.0).min(1.0),
            ConsiderationType::EnemyCount => (context.enemy_count as f32 / 10.0).min(1.0),
            ConsiderationType::DistanceToHome => context
                .distance_to_home
                .map(|d| 1.0 / (d + 1.0))
                .unwrap_or(0.0),
            ConsiderationType::HasLineOfSight => {
                if context.has_line_of_sight {
                    1.0
                } else {
                    0.0
                }
            }
            ConsiderationType::IsFlanked => {
                if context.is_flanked {
                    1.0
                } else {
                    0.0
                }
            }
            ConsiderationType::TimeSinceLastAttack { max_time } => {
                (context.time_since_last_attack / max_time).min(1.0)
            }
            ConsiderationType::Random => rand::random(),
            ConsiderationType::Constant { value } => *value,
            ConsiderationType::Custom { id } => {
                context.custom_values.get(id).copied().unwrap_or(0.5)
            }
        };

        // Apply curve
        let mut score = self.curve.apply(raw_score);

        // Invert if needed
        if self.invert {
            score = 1.0 - score;
        }

        // Add bonus
        score = (score + self.bonus).clamp(0.0, 1.0);

        score
    }
}

/// Consideration type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConsiderationType {
    Health,
    HealthPercent,
    DistanceToTarget,
    DistanceToTargetNormalized { max_distance: f32 },
    HasTarget,
    TargetHealth,
    TargetDistance,
    Ammo,
    AmmoPercent,
    Cooldown { id: String },
    ThreatLevel,
    AllyCount,
    EnemyCount,
    DistanceToHome,
    HasLineOfSight,
    IsFlanked,
    TimeSinceLastAttack { max_time: f32 },
    Random,
    Constant { value: f32 },
    Custom { id: String },
}

/// Response curve for scoring.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ResponseCurve {
    /// Linear (identity).
    Linear,
    /// Quadratic curve.
    Quadratic { exponent: f32 },
    /// Sigmoid curve.
    Sigmoid { steepness: f32, midpoint: f32 },
    /// Logit curve.
    Logit { steepness: f32 },
    /// Threshold (step function).
    Threshold { value: f32 },
    /// Piecewise linear.
    PiecewiseLinear { points: [(f32, f32); 4] },
}

impl ResponseCurve {
    pub fn apply(&self, input: f32) -> f32 {
        let input = input.clamp(0.0, 1.0);

        match self {
            Self::Linear => input,
            Self::Quadratic { exponent } => input.powf(*exponent),
            Self::Sigmoid {
                steepness,
                midpoint,
            } => 1.0 / (1.0 + (-steepness * (input - midpoint)).exp()),
            Self::Logit { steepness } => {
                if input <= 0.0 {
                    0.0
                } else if input >= 1.0 {
                    1.0
                } else {
                    (1.0 / (1.0 + (-input / (1.0 - input)).ln() / steepness)).clamp(0.0, 1.0)
                }
            }
            Self::Threshold { value } => {
                if input >= *value {
                    1.0
                } else {
                    0.0
                }
            }
            Self::PiecewiseLinear { points } => {
                let points = points.as_slice();
                for i in 0..points.len() - 1 {
                    if input >= points[i].0 && input <= points[i + 1].0 {
                        let t = (input - points[i].0) / (points[i + 1].0 - points[i].0);
                        return points[i].1 + t * (points[i + 1].1 - points[i].1);
                    }
                }
                if input < points[0].0 {
                    points[0].1
                } else {
                    points[points.len() - 1].1
                }
            }
        }
    }
}

/// Context for utility calculations.
#[derive(Debug, Clone, Default)]
pub struct UtilityContext {
    // Agent state
    pub health: f32,
    pub health_percent: f32,
    pub ammo: u32,
    pub ammo_percent: f32,
    pub threat_level: f32,
    pub time_since_last_attack: f32,
    pub has_target: bool,
    pub has_line_of_sight: bool,
    pub is_flanked: bool,

    // Target info
    pub target_health: Option<f32>,
    pub target_distance: Option<f32>,
    pub distance_to_target: Option<f32>,

    // Environment
    pub ally_count: u32,
    pub enemy_count: u32,
    pub distance_to_home: Option<f32>,

    // Cooldowns
    pub cooldowns: HashMap<String, f32>,

    // Custom values
    pub custom_values: HashMap<String, f32>,
}

impl UtilityContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_health(&mut self, current: f32, max: f32) {
        self.health = current;
        self.health_percent = current / max.max(1.0);
    }

    pub fn set_ammo(&mut self, current: u32, max: u32) {
        self.ammo = current;
        self.ammo_percent = current as f32 / max.max(1) as f32;
    }

    pub fn set_custom(&mut self, key: impl Into<String>, value: f32) {
        self.custom_values.insert(key.into(), value);
    }
}

/// Utility AI decision maker.
#[derive(Debug, Clone)]
pub struct UtilityBrain {
    /// Available actions.
    pub actions: HashMap<String, UtilityAction>,
    /// Current best action.
    pub current_action: Option<String>,
    /// Current action score.
    pub current_score: f32,
    /// Action cooldowns.
    pub cooldowns: HashMap<String, f32>,
    /// Decision history.
    pub history: Vec<(String, f32)>,
    /// Max history size.
    pub max_history: usize,
}

impl UtilityBrain {
    pub fn new() -> Self {
        Self {
            actions: HashMap::new(),
            current_action: None,
            current_score: 0.0,
            cooldowns: HashMap::new(),
            history: Vec::new(),
            max_history: 100,
        }
    }

    pub fn add_action(&mut self, action: UtilityAction) {
        self.actions.insert(action.id.clone(), action);
    }

    /// Select best action based on context.
    pub fn decide(&mut self, context: &UtilityContext) -> Option<&UtilityAction> {
        let mut best_action: Option<&UtilityAction> = None;
        let mut best_score = 0.0f32;

        for action in self.actions.values() {
            // Check cooldown
            if let Some(cooldown) = self.cooldowns.get(&action.id) {
                if *cooldown > 0.0 {
                    continue;
                }
            }

            // Calculate score
            let score = action.calculate_score(context);

            // Check minimum threshold
            if score < action.min_score {
                continue;
            }

            // Add some noise for variety
            let noise = rand::random::<f32>() * 0.05;
            let final_score = score + noise;

            if final_score > best_score {
                best_score = final_score;
                best_action = Some(action);
            }
        }

        // Update state
        if let Some(action) = best_action {
            self.current_action = Some(action.id.clone());
            self.current_score = best_score;

            // Record history
            self.history.push((action.id.clone(), best_score));
            if self.history.len() > self.max_history {
                self.history.remove(0);
            }

            // Start cooldown
            self.cooldowns.insert(action.id.clone(), action.cooldown);
        }

        best_action
    }

    /// Update cooldowns.
    pub fn update(&mut self, dt: f32) {
        for cooldown in self.cooldowns.values_mut() {
            *cooldown = (*cooldown - dt).max(0.0);
        }
    }

    /// Get action by ID.
    pub fn get_action(&self, id: &str) -> Option<&UtilityAction> {
        self.actions.get(id)
    }
}

impl Default for UtilityBrain {
    fn default() -> Self {
        Self::new()
    }
}

/// Utility AI System.
pub struct UtilityAISystem {
    /// All agent brains.
    pub brains: HashMap<u64, UtilityBrain>,
    /// Debug mode.
    pub debug: bool,
}

impl UtilityAISystem {
    pub fn new() -> Self {
        Self {
            brains: HashMap::new(),
            debug: false,
        }
    }

    pub fn add_agent(&mut self, id: u64, brain: UtilityBrain) {
        self.brains.insert(id, brain);
    }

    pub fn update(&mut self, contexts: &HashMap<u64, UtilityContext>, dt: f32) {
        for (id, brain) in &mut self.brains {
            if let Some(context) = contexts.get(id) {
                brain.decide(context);
            }
            brain.update(dt);
        }
    }

    pub fn get_decision(&self, agent_id: u64) -> Option<(String, f32)> {
        self.brains
            .get(&agent_id)
            .and_then(|b| b.current_action.clone().map(|a| (a, b.current_score)))
    }
}

impl Default for UtilityAISystem {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consideration_health() {
        let context = UtilityContext {
            health_percent: 0.5,
            ..Default::default()
        };

        let consideration = Consideration::new(ConsiderationType::HealthPercent);
        let score = consideration.calculate_score(&context);

        assert!((score - 0.5).abs() < 0.01);
    }

    #[test]
    fn consideration_inverted() {
        let context = UtilityContext {
            health_percent: 0.3,
            ..Default::default()
        };

        let consideration = Consideration::new(ConsiderationType::HealthPercent).inverted();
        let score = consideration.calculate_score(&context);

        assert!((score - 0.7).abs() < 0.01);
    }

    #[test]
    fn utility_action_score() {
        let action = UtilityAction::new("test")
            .add_consideration(Consideration::new(ConsiderationType::Constant {
                value: 0.5,
            }))
            .add_consideration(Consideration::new(ConsiderationType::Constant {
                value: 0.8,
            }))
            .with_weight(2.0);

        let context = UtilityContext::default();
        let score = action.calculate_score(&context);

        assert!((score - 0.8).abs() < 0.01); // 0.5 * 0.8 * 2.0
    }

    #[test]
    fn brain_decision() {
        let mut brain = UtilityBrain::new();

        brain.add_action(
            UtilityAction::new("low").add_consideration(Consideration::new(
                ConsiderationType::Constant { value: 0.3 },
            )),
        );

        brain.add_action(
            UtilityAction::new("high").add_consideration(Consideration::new(
                ConsiderationType::Constant { value: 0.9 },
            )),
        );

        let context = UtilityContext::default();
        let decision = brain.decide(&context);

        assert!(decision.is_some());
        assert_eq!(decision.unwrap().id, "high");
    }

    #[test]
    fn response_curves() {
        let linear = ResponseCurve::Linear;
        assert_eq!(linear.apply(0.5), 0.5);

        let quad = ResponseCurve::Quadratic { exponent: 2.0 };
        assert!((quad.apply(0.5) - 0.25).abs() < 0.01);

        let threshold = ResponseCurve::Threshold { value: 0.5 };
        assert_eq!(threshold.apply(0.4), 0.0);
        assert_eq!(threshold.apply(0.6), 1.0);
    }
}
