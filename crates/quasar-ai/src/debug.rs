//! AI Debug - Visualization and Debugging Tools

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum DebugDrawColor {
    Red,
    Green,
    Blue,
    Yellow,
    Cyan,
    Magenta,
    White,
    Gray,
    Custom([f32; 4]),
}

impl DebugDrawColor {
    pub fn to_rgba(&self) -> [f32; 4] {
        match self {
            Self::Red => [1.0, 0.0, 0.0, 1.0],
            Self::Green => [0.0, 1.0, 0.0, 1.0],
            Self::Blue => [0.0, 0.0, 1.0, 1.0],
            Self::Yellow => [1.0, 1.0, 0.0, 1.0],
            Self::Cyan => [0.0, 1.0, 1.0, 1.0],
            Self::Magenta => [1.0, 0.0, 1.0, 1.0],
            Self::White => [1.0, 1.0, 1.0, 1.0],
            Self::Gray => [0.5, 0.5, 0.5, 1.0],
            Self::Custom(rgba) => *rgba,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DebugDrawCommand {
    Line {
        start: [f32; 3],
        end: [f32; 3],
        color: DebugDrawColor,
        thickness: f32,
    },
    Sphere {
        center: [f32; 3],
        radius: f32,
        color: DebugDrawColor,
        wireframe: bool,
    },
    Box {
        center: [f32; 3],
        half_extents: [f32; 3],
        color: DebugDrawColor,
        wireframe: bool,
    },
    Cylinder {
        center: [f32; 3],
        radius: f32,
        height: f32,
        color: DebugDrawColor,
    },
    Arrow {
        start: [f32; 3],
        end: [f32; 3],
        color: DebugDrawColor,
        head_size: f32,
    },
    Text {
        position: [f32; 3],
        text: String,
        color: DebugDrawColor,
        size: f32,
    },
    Path {
        points: Vec<[f32; 3]>,
        color: DebugDrawColor,
        thickness: f32,
    },
    Circle {
        center: [f32; 3],
        normal: [f32; 3],
        radius: f32,
        color: DebugDrawColor,
    },
}

pub struct DebugDraw {
    commands: Vec<DebugDrawCommand>,
    enabled: bool,
}

impl Default for DebugDraw {
    fn default() -> Self {
        Self::new()
    }
}

impl DebugDraw {
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
            enabled: true,
        }
    }

    pub fn enable(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn clear(&mut self) {
        self.commands.clear();
    }

    pub fn commands(&self) -> &[DebugDrawCommand] {
        &self.commands
    }

    pub fn draw_line(&mut self, start: [f32; 3], end: [f32; 3], color: DebugDrawColor) {
        if self.enabled {
            self.commands.push(DebugDrawCommand::Line {
                start,
                end,
                color,
                thickness: 1.0,
            });
        }
    }

    pub fn draw_line_thick(
        &mut self,
        start: [f32; 3],
        end: [f32; 3],
        color: DebugDrawColor,
        thickness: f32,
    ) {
        if self.enabled {
            self.commands.push(DebugDrawCommand::Line {
                start,
                end,
                color,
                thickness,
            });
        }
    }

    pub fn draw_sphere(&mut self, center: [f32; 3], radius: f32, color: DebugDrawColor) {
        if self.enabled {
            self.commands.push(DebugDrawCommand::Sphere {
                center,
                radius,
                color,
                wireframe: true,
            });
        }
    }

    pub fn draw_box(&mut self, center: [f32; 3], half_extents: [f32; 3], color: DebugDrawColor) {
        if self.enabled {
            self.commands.push(DebugDrawCommand::Box {
                center,
                half_extents,
                color,
                wireframe: true,
            });
        }
    }

    pub fn draw_arrow(&mut self, start: [f32; 3], end: [f32; 3], color: DebugDrawColor) {
        if self.enabled {
            let length = ((end[0] - start[0]).powi(2)
                + (end[1] - start[1]).powi(2)
                + (end[2] - start[2]).powi(2))
            .sqrt();
            let head_size = length * 0.2;
            self.commands.push(DebugDrawCommand::Arrow {
                start,
                end,
                color,
                head_size,
            });
        }
    }

    pub fn draw_path(&mut self, points: Vec<[f32; 3]>, color: DebugDrawColor) {
        if self.enabled && points.len() >= 2 {
            self.commands.push(DebugDrawCommand::Path {
                points,
                color,
                thickness: 2.0,
            });
        }
    }

    pub fn draw_text(&mut self, position: [f32; 3], text: &str, color: DebugDrawColor) {
        if self.enabled {
            self.commands.push(DebugDrawCommand::Text {
                position,
                text: text.to_string(),
                color,
                size: 12.0,
            });
        }
    }

    pub fn draw_circle(
        &mut self,
        center: [f32; 3],
        normal: [f32; 3],
        radius: f32,
        color: DebugDrawColor,
    ) {
        if self.enabled {
            self.commands.push(DebugDrawCommand::Circle {
                center,
                normal,
                radius,
                color,
            });
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiAgentDebugInfo {
    pub agent_id: u64,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub current_state: String,
    pub target: Option<[f32; 3]>,
    pub path: Vec<[f32; 3]>,
    pub blackboard_values: HashMap<String, String>,
    pub behavior_tree_status: Option<String>,
    pub goap_plan: Vec<String>,
    pub utility_scores: HashMap<String, f32>,
    pub perception_count: u32,
    pub awareness_level: String,
}

impl AiAgentDebugInfo {
    pub fn new(agent_id: u64) -> Self {
        Self {
            agent_id,
            position: [0.0; 3],
            velocity: [0.0; 3],
            current_state: String::new(),
            target: None,
            path: Vec::new(),
            blackboard_values: HashMap::new(),
            behavior_tree_status: None,
            goap_plan: Vec::new(),
            utility_scores: HashMap::new(),
            perception_count: 0,
            awareness_level: "Unaware".to_string(),
        }
    }
}

pub struct AiDebugger {
    agents: HashMap<u64, AiAgentDebugInfo>,
    global_commands: Vec<DebugDrawCommand>,
    enabled: bool,
    selected_agent: Option<u64>,
    show_paths: bool,
    show_perception: bool,
    show_blackboard: bool,
    show_planning: bool,
}

impl Default for AiDebugger {
    fn default() -> Self {
        Self::new()
    }
}

impl AiDebugger {
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
            global_commands: Vec::new(),
            enabled: false,
            selected_agent: None,
            show_paths: true,
            show_perception: true,
            show_blackboard: true,
            show_planning: true,
        }
    }

    pub fn enable(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn update_agent(&mut self, info: AiAgentDebugInfo) {
        self.agents.insert(info.agent_id, info);
    }

    pub fn remove_agent(&mut self, agent_id: u64) {
        self.agents.remove(&agent_id);
    }

    pub fn clear(&mut self) {
        self.agents.clear();
        self.global_commands.clear();
    }

    pub fn select_agent(&mut self, agent_id: Option<u64>) {
        self.selected_agent = agent_id;
    }

    pub fn get_selected_agent(&self) -> Option<&AiAgentDebugInfo> {
        self.selected_agent.and_then(|id| self.agents.get(&id))
    }

    pub fn agents(&self) -> &HashMap<u64, AiAgentDebugInfo> {
        &self.agents
    }

    pub fn set_show_paths(&mut self, show: bool) {
        self.show_paths = show;
    }

    pub fn set_show_perception(&mut self, show: bool) {
        self.show_perception = show;
    }

    pub fn set_show_blackboard(&mut self, show: bool) {
        self.show_blackboard = show;
    }

    pub fn set_show_planning(&mut self, show: bool) {
        self.show_planning = show;
    }

    pub fn generate_draw_commands(&self, draw: &mut DebugDraw) {
        if !self.enabled {
            return;
        }

        for agent in self.agents.values() {
            if self.selected_agent.is_some() && self.selected_agent != Some(agent.agent_id) {
                continue;
            }

            if self.show_paths && !agent.path.is_empty() {
                draw.draw_path(agent.path.clone(), DebugDrawColor::Cyan);
            }

            if self.show_perception {
                let color = match agent.awareness_level.as_str() {
                    "Threat" => DebugDrawColor::Red,
                    "Alert" => DebugDrawColor::Yellow,
                    "Suspicious" => DebugDrawColor::Cyan,
                    _ => DebugDrawColor::Green,
                };
                draw.draw_circle(agent.position, [0.0, 1.0, 0.0], 2.0, color);
            }

            if let Some(target) = agent.target {
                draw.draw_line(agent.position, target, DebugDrawColor::Yellow);
                draw.draw_sphere(target, 0.3, DebugDrawColor::Yellow);
            }

            if agent.velocity != [0.0; 3] {
                let vel_end = [
                    agent.position[0] + agent.velocity[0],
                    agent.position[1] + agent.velocity[1],
                    agent.position[2] + agent.velocity[2],
                ];
                draw.draw_arrow(agent.position, vel_end, DebugDrawColor::Blue);
            }
        }

        for cmd in &self.global_commands {
            draw.commands.push(cmd.clone());
        }
    }

    pub fn add_global_command(&mut self, command: DebugDrawCommand) {
        self.global_commands.push(command);
    }

    pub fn clear_global_commands(&mut self) {
        self.global_commands.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_draw_line() {
        let mut draw = DebugDraw::new();
        draw.draw_line([0.0; 3], [1.0; 3], DebugDrawColor::Red);

        assert_eq!(draw.commands().len(), 1);
    }

    #[test]
    fn debug_draw_disabled() {
        let mut draw = DebugDraw::new();
        draw.enable(false);
        draw.draw_line([0.0; 3], [1.0; 3], DebugDrawColor::Red);

        assert_eq!(draw.commands().len(), 0);
    }

    #[test]
    fn debug_draw_clear() {
        let mut draw = DebugDraw::new();
        draw.draw_line([0.0; 3], [1.0; 3], DebugDrawColor::Red);
        draw.clear();

        assert_eq!(draw.commands().len(), 0);
    }

    #[test]
    fn ai_debugger_update_agent() {
        let mut debugger = AiDebugger::new();
        let mut info = AiAgentDebugInfo::new(1);
        info.position = [1.0, 2.0, 3.0];

        debugger.update_agent(info);

        assert!(debugger.agents().contains_key(&1));
    }

    #[test]
    fn ai_debugger_select_agent() {
        let mut debugger = AiDebugger::new();
        debugger.update_agent(AiAgentDebugInfo::new(1));
        debugger.update_agent(AiAgentDebugInfo::new(2));

        debugger.select_agent(Some(1));

        assert!(debugger.get_selected_agent().is_some());
        assert_eq!(debugger.get_selected_agent().unwrap().agent_id, 1);
    }

    #[test]
    fn ai_debugger_toggle() {
        let mut debugger = AiDebugger::new();
        assert!(!debugger.is_enabled());

        debugger.toggle();
        assert!(debugger.is_enabled());

        debugger.toggle();
        assert!(!debugger.is_enabled());
    }

    #[test]
    fn debug_color_rgba() {
        assert_eq!(DebugDrawColor::Red.to_rgba(), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(DebugDrawColor::Green.to_rgba(), [0.0, 1.0, 0.0, 1.0]);
    }
}
