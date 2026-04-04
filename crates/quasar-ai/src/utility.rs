//! Utility AI Implementation
//!
//! Score-based decision making for emergent behavior.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ResponseCurveType {
    Linear,
    Quadratic,
    Logistic,
    Logit,
    Threshold,
    Sine,
    Exponential,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseCurve {
    pub curve_type: ResponseCurveType,
    pub slope: f32,
    pub exponent: f32,
    pub shift: f32,
    pub threshold: f32,
}

impl Default for ResponseCurve {
    fn default() -> Self {
        Self::linear()
    }
}

impl ResponseCurve {
    pub fn linear() -> Self {
        Self {
            curve_type: ResponseCurveType::Linear,
            slope: 1.0,
            exponent: 1.0,
            shift: 0.0,
            threshold: 0.5,
        }
    }

    pub fn quadratic() -> Self {
        Self {
            curve_type: ResponseCurveType::Quadratic,
            slope: 1.0,
            exponent: 2.0,
            shift: 0.0,
            threshold: 0.5,
        }
    }

    pub fn logistic() -> Self {
        Self {
            curve_type: ResponseCurveType::Logistic,
            slope: 1.0,
            exponent: 2.0,
            shift: 0.0,
            threshold: 0.5,
        }
    }

    pub fn threshold(threshold: f32) -> Self {
        Self {
            curve_type: ResponseCurveType::Threshold,
            slope: 1.0,
            exponent: 1.0,
            shift: 0.0,
            threshold,
        }
    }

    pub fn slope(mut self, slope: f32) -> Self {
        self.slope = slope;
        self
    }

    pub fn exponent(mut self, exponent: f32) -> Self {
        self.exponent = exponent;
        self
    }

    pub fn shift(mut self, shift: f32) -> Self {
        self.shift = shift;
        self
    }

    pub fn evaluate(&self, input: f32) -> f32 {
        let x = (input - self.shift).clamp(0.0, 1.0);

        match self.curve_type {
            ResponseCurveType::Linear => (self.slope * x).clamp(0.0, 1.0),
            ResponseCurveType::Quadratic => {
                if self.slope >= 0.0 {
                    (self.slope * x.powf(self.exponent)).clamp(0.0, 1.0)
                } else {
                    (1.0 - (-self.slope * (1.0 - x).powf(self.exponent))).clamp(0.0, 1.0)
                }
            }
            ResponseCurveType::Logistic => {
                let v = 1.0 / (1.0 + (-self.slope * (x - 0.5)).exp());
                v.clamp(0.0, 1.0)
            }
            ResponseCurveType::Logit => {
                if x <= 0.0 {
                    0.0
                } else if x >= 1.0 {
                    1.0
                } else {
                    let v = (x / (1.0 - x)).ln() * self.slope;
                    (v + 0.5).clamp(0.0, 1.0)
                }
            }
            ResponseCurveType::Threshold => {
                if x >= self.threshold {
                    1.0
                } else {
                    0.0
                }
            }
            ResponseCurveType::Sine => {
                ((x * std::f32::consts::PI / 2.0).sin() * self.slope).clamp(0.0, 1.0)
            }
            ResponseCurveType::Exponential => (self.slope * (x - 1.0).exp()).clamp(0.0, 1.0),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Consideration {
    pub name: String,
    pub weight: f32,
    pub curve: ResponseCurve,
    pub input_fn: String,
}

impl Consideration {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            weight: 1.0,
            curve: ResponseCurve::linear(),
            input_fn: name.to_string(),
        }
    }

    pub fn weight(mut self, weight: f32) -> Self {
        self.weight = weight;
        self
    }

    pub fn curve(mut self, curve: ResponseCurve) -> Self {
        self.curve = curve;
        self
    }

    pub fn input(mut self, input_fn: &str) -> Self {
        self.input_fn = input_fn.to_string();
        self
    }

    pub fn evaluate(&self, input: f32) -> f32 {
        self.curve.evaluate(input) * self.weight
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UtilityAction {
    pub name: String,
    pub considerations: Vec<Consideration>,
    pub weight: f32,
    pub cooldown: f32,
    pub last_used: f32,
}

impl UtilityAction {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            considerations: Vec::new(),
            weight: 1.0,
            cooldown: 0.0,
            last_used: -f32::MAX,
        }
    }

    pub fn consideration(mut self, consideration: Consideration) -> Self {
        self.considerations.push(consideration);
        self
    }

    pub fn weight(mut self, weight: f32) -> Self {
        self.weight = weight;
        self
    }

    pub fn cooldown(mut self, cooldown: f32) -> Self {
        self.cooldown = cooldown;
        self
    }

    pub fn calculate_score(&self, inputs: &HashMap<String, f32>, current_time: f32) -> f32 {
        if current_time - self.last_used < self.cooldown {
            return 0.0;
        }

        if self.considerations.is_empty() {
            return self.weight;
        }

        let mut score = 1.0;
        for consideration in &self.considerations {
            let input = inputs.get(&consideration.input_fn).copied().unwrap_or(0.5);
            let consideration_score = consideration.evaluate(input);
            score *= consideration_score;
        }

        score * self.weight
    }
}

pub struct UtilityBrain {
    actions: Vec<UtilityAction>,
    current_action: Option<String>,
    decision_time: f32,
}

impl Default for UtilityBrain {
    fn default() -> Self {
        Self::new()
    }
}

impl UtilityBrain {
    pub fn new() -> Self {
        Self {
            actions: Vec::new(),
            current_action: None,
            decision_time: 0.0,
        }
    }

    pub fn add_action(&mut self, action: UtilityAction) {
        self.actions.push(action);
    }

    pub fn actions(&self) -> &[UtilityAction] {
        &self.actions
    }

    pub fn decide(&mut self, inputs: &HashMap<String, f32>, current_time: f32) -> Option<&str> {
        let start = std::time::Instant::now();

        let mut best_action: Option<&UtilityAction> = None;
        let mut best_score = 0.0f32;

        for action in &self.actions {
            let score = action.calculate_score(inputs, current_time);
            if score > best_score {
                best_score = score;
                best_action = Some(action);
            }
        }

        let best_action_name = best_action.map(|a| a.name.clone());

        if let Some(name) = &best_action_name {
            self.current_action = Some(name.clone());
            if let Some(a) = self.actions.iter_mut().find(|a| &a.name == name) {
                a.last_used = current_time;
            }
        }

        self.decision_time = start.elapsed().as_secs_f32();

        self.current_action.as_deref()
    }

    pub fn get_all_scores(
        &self,
        inputs: &HashMap<String, f32>,
        current_time: f32,
    ) -> Vec<(String, f32)> {
        self.actions
            .iter()
            .map(|a| (a.name.clone(), a.calculate_score(inputs, current_time)))
            .collect()
    }

    pub fn decision_time(&self) -> f32 {
        self.decision_time
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_curve_linear() {
        let curve = ResponseCurve::linear();
        assert!((curve.evaluate(0.0) - 0.0).abs() < 0.001);
        assert!((curve.evaluate(0.5) - 0.5).abs() < 0.001);
        assert!((curve.evaluate(1.0) - 1.0).abs() < 0.001);
    }

    #[test]
    fn response_curve_quadratic() {
        let curve = ResponseCurve::quadratic();
        assert!((curve.evaluate(0.0) - 0.0).abs() < 0.001);
        assert!((curve.evaluate(0.5) - 0.25).abs() < 0.001);
        assert!((curve.evaluate(1.0) - 1.0).abs() < 0.001);
    }

    #[test]
    fn response_curve_threshold() {
        let curve = ResponseCurve::threshold(0.5);
        assert_eq!(curve.evaluate(0.4), 0.0);
        assert_eq!(curve.evaluate(0.5), 1.0);
        assert_eq!(curve.evaluate(0.6), 1.0);
    }

    #[test]
    fn consideration_evaluate() {
        let cons = Consideration::new("health")
            .weight(1.0)
            .curve(ResponseCurve::linear());
        assert!((cons.evaluate(0.5) - 0.5).abs() < 0.001);
    }

    #[test]
    fn utility_action_score() {
        let action = UtilityAction::new("flee").consideration(
            Consideration::new("health").curve(ResponseCurve::quadratic().slope(-1.0)),
        );

        let mut inputs = HashMap::new();
        inputs.insert("health".to_string(), 0.2);

        let score = action.calculate_score(&inputs, 0.0);
        assert!(score > 0.0);
    }

    #[test]
    fn utility_brain_decide() {
        let mut brain = UtilityBrain::new();

        brain.add_action(UtilityAction::new("idle").weight(0.5));
        brain.add_action(UtilityAction::new("attack").weight(0.8));

        let inputs = HashMap::new();
        let decision = brain.decide(&inputs, 0.0);

        assert_eq!(decision, Some("attack"));
    }

    #[test]
    fn utility_brain_cooldown() {
        let mut brain = UtilityBrain::new();

        brain.add_action(UtilityAction::new("attack").cooldown(1.0).weight(1.0));
        brain.add_action(UtilityAction::new("idle").weight(0.1));

        let inputs = HashMap::new();
        brain.decide(&inputs, 0.0);

        let decision = brain.decide(&inputs, 0.5);
        assert_eq!(decision, Some("idle"));

        let decision = brain.decide(&inputs, 1.5);
        assert_eq!(decision, Some("attack"));
    }

    #[test]
    fn utility_brain_scores() {
        let mut brain = UtilityBrain::new();
        brain.add_action(UtilityAction::new("a").weight(0.5));
        brain.add_action(UtilityAction::new("b").weight(0.8));

        let scores = brain.get_all_scores(&HashMap::new(), 0.0);
        assert_eq!(scores.len(), 2);

        let b_score = scores.iter().find(|(n, _)| n == "b").unwrap().1;
        let a_score = scores.iter().find(|(n, _)| n == "a").unwrap().1;
        assert!(b_score > a_score);
    }
}
