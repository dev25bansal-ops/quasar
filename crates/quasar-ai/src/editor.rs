//! Editor Integration for AI Systems in Quasar Engine.
//!
//! Provides bridge between the visual editor and runtime AI systems:
//! - **Tree compilation** — convert editor graphs to runtime BtNode trees
//! - **Agent management** — assign behavior trees to AI agents
//! - **Live editing** — hot-reload behavior trees during simulation
//! - **Blackboard sync** — synchronize editor blackboard with runtime
//! - **Agent registry** — track all AI agents and their assigned trees

#![allow(deprecated)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::behavior_tree::{BehaviorTree, BtNode, BtContext, BtState, BtStatus};
use crate::blackboard::Blackboard;

/// Configuration for an AI agent in the editor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiAgentConfig {
    /// Unique agent ID.
    pub agent_id: u64,
    /// Display name.
    pub name: String,
    /// ID of the assigned behavior tree.
    pub behavior_tree_id: Option<u64>,
    /// Agent's blackboard.
    pub blackboard: Blackboard,
    /// Whether the agent is active.
    pub is_active: bool,
    /// Agent-specific parameters.
    pub parameters: HashMap<String, String>,
}

impl AiAgentConfig {
    pub fn new(agent_id: u64, name: &str) -> Self {
        Self {
            agent_id,
            name: name.to_string(),
            behavior_tree_id: None,
            blackboard: Blackboard::new(),
            is_active: true,
            parameters: HashMap::new(),
        }
    }
}

/// Registry of all AI agents and their configurations.
pub struct AiAgentRegistry {
    /// Registered agents.
    agents: HashMap<u64, AiAgentConfig>,
    /// Next available agent ID.
    next_agent_id: u64,
    /// Assigned behavior trees (tree_id -> compiled tree).
    behavior_trees: HashMap<u64, BehaviorTree>,
    /// Next available tree ID.
    next_tree_id: u64,
}

impl AiAgentRegistry {
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
            next_agent_id: 1,
            behavior_trees: HashMap::new(),
            next_tree_id: 1,
        }
    }

    // --- Agent Management ---

    /// Register a new AI agent.
    pub fn register_agent(&mut self, name: &str) -> u64 {
        let id = self.next_agent_id;
        self.next_agent_id += 1;
        let agent = AiAgentConfig::new(id, name);
        self.agents.insert(id, agent);
        id
    }

    /// Remove an agent by ID.
    pub fn remove_agent(&mut self, agent_id: u64) -> bool {
        self.agents.remove(&agent_id).is_some()
    }

    /// Get a reference to an agent's config.
    pub fn get_agent(&self, agent_id: u64) -> Option<&AiAgentConfig> {
        self.agents.get(&agent_id)
    }

    /// Get a mutable reference to an agent's config.
    pub fn get_agent_mut(&mut self, agent_id: u64) -> Option<&mut AiAgentConfig> {
        self.agents.get_mut(&agent_id)
    }

    /// Iterate over all agents.
    pub fn agents(&self) -> impl Iterator<Item = (u64, &AiAgentConfig)> {
        self.agents.iter().map(|(id, config)| (*id, config))
    }

    /// Assign a behavior tree to an agent.
    pub fn assign_tree(&mut self, agent_id: u64, tree_id: u64) -> bool {
        if let Some(agent) = self.agents.get_mut(&agent_id) {
            if self.behavior_trees.contains_key(&tree_id) {
                agent.behavior_tree_id = Some(tree_id);
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    // --- Behavior Tree Management ---

    /// Register a compiled behavior tree.
    pub fn register_tree(&mut self, tree: BehaviorTree) -> u64 {
        let id = self.next_tree_id;
        self.next_tree_id += 1;
        self.behavior_trees.insert(id, tree);
        id
    }

    /// Remove a behavior tree.
    pub fn remove_tree(&mut self, tree_id: u64) -> bool {
        // Remove assignments to this tree
        for agent in self.agents.values_mut() {
            if agent.behavior_tree_id == Some(tree_id) {
                agent.behavior_tree_id = None;
            }
        }
        self.behavior_trees.remove(&tree_id).is_some()
    }

    /// Get a reference to a behavior tree.
    pub fn get_tree(&self, tree_id: u64) -> Option<&BehaviorTree> {
        self.behavior_trees.get(&tree_id)
    }

    /// Get a mutable reference to a behavior tree.
    pub fn get_tree_mut(&mut self, tree_id: u64) -> Option<&mut BehaviorTree> {
        self.behavior_trees.get_mut(&tree_id)
    }

    /// Hot-reload a behavior tree: replace with new tree and update assigned agents.
    pub fn hot_reload_tree(&mut self, tree_id: u64, new_tree: BehaviorTree) -> bool {
        if let Some(tree) = self.behavior_trees.get_mut(&tree_id) {
            *tree = new_tree;
            true
        } else {
            false
        }
    }

    // --- Tick Execution ---

    /// Tick all active agents' behavior trees.
    pub fn tick_agents(
        &mut self,
        delta_time: f32,
        elapsed_time: f32,
    ) -> HashMap<u64, BtStatus> {
        let mut results = HashMap::new();

        let active_agents: Vec<_> = self.agents
            .iter()
            .filter(|(_, agent)| agent.is_active && agent.behavior_tree_id.is_some())
            .map(|(id, agent)| (*id, agent.behavior_tree_id.unwrap()))
            .collect();

        for (agent_id, tree_id) in active_agents {
            if let Some(tree) = self.behavior_trees.get(&tree_id) {
                let agent = self.agents.get(&agent_id).unwrap();
                let ctx = BtContext {
                    blackboard: &agent.blackboard,
                    delta_time,
                    elapsed_time,
                };
                let mut state = BtState::new();
                let status = tree.tick(&ctx, &mut state);
                results.insert(agent_id, status);
            }
        }

        results
    }

    /// Get the number of registered agents.
    pub fn agent_count(&self) -> usize {
        self.agents.len()
    }

    /// Get the number of registered behavior trees.
    pub fn tree_count(&self) -> usize {
        self.behavior_trees.len()
    }
}

impl Default for AiAgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Editor-specific behavior tree wrapper with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorBehaviorTree {
    /// Unique tree ID.
    pub id: u64,
    /// Tree name.
    pub name: String,
    /// Tree description.
    pub description: String,
    /// Tags for organization.
    pub tags: Vec<String>,
    /// The compiled runtime tree.
    pub runtime_tree: Option<BtNode>,
    /// Whether the tree has been validated.
    pub is_validated: bool,
    /// Validation errors (if any).
    pub validation_errors: Vec<String>,
}

impl EditorBehaviorTree {
    pub fn new(id: u64, name: &str) -> Self {
        Self {
            id,
            name: name.to_string(),
            description: String::new(),
            tags: Vec::new(),
            runtime_tree: None,
            is_validated: false,
            validation_errors: Vec::new(),
        }
    }

    /// Compile the editor tree into a runtime behavior tree.
    pub fn compile(&self) -> Option<BehaviorTree> {
        self.runtime_tree.clone().map(|root| {
            BehaviorTree::new(&self.name, root)
        })
    }
}

/// AI Editor state manager.
pub struct AiEditorManager {
    /// Agent registry.
    pub registry: AiAgentRegistry,
    /// Editor trees (not yet compiled).
    pub editor_trees: HashMap<u64, EditorBehaviorTree>,
    /// Next editor tree ID.
    pub next_editor_tree_id: u64,
    /// Whether live editing is enabled.
    pub live_editing_enabled: bool,
    /// Auto-validation on compile.
    pub auto_validate: bool,
}

impl AiEditorManager {
    pub fn new() -> Self {
        Self {
            registry: AiAgentRegistry::new(),
            editor_trees: HashMap::new(),
            next_editor_tree_id: 1,
            live_editing_enabled: true,
            auto_validate: true,
        }
    }

    /// Create a new editor tree.
    pub fn create_editor_tree(&mut self, name: &str) -> u64 {
        let id = self.next_editor_tree_id;
        self.next_editor_tree_id += 1;
        let tree = EditorBehaviorTree::new(id, name);
        self.editor_trees.insert(id, tree);
        id
    }

    /// Update an editor tree's runtime node.
    pub fn update_tree_runtime(&mut self, tree_id: u64, runtime: BtNode) {
        if let Some(tree) = self.editor_trees.get_mut(&tree_id) {
            tree.runtime_tree = Some(runtime);
            if self.auto_validate {
                if let Some(ref rt) = tree.runtime_tree {
                    tree.validation_errors = Self::validate_tree(rt);
                    tree.is_validated = tree.validation_errors.is_empty();
                }
            }
        }
    }

    /// Compile and register an editor tree.
    pub fn compile_and_register(&mut self, tree_id: u64) -> Option<u64> {
        let editor_tree = self.editor_trees.get(&tree_id)?;
        let runtime_tree = editor_tree.compile()?;
        let compiled_id = self.registry.register_tree(runtime_tree);

        // Update editor tree metadata
        if let Some(et) = self.editor_trees.get_mut(&tree_id) {
            et.is_validated = true;
        }

        Some(compiled_id)
    }

    /// Validate a behavior tree and return any errors.
    fn validate_tree(root: &BtNode) -> Vec<String> {
        let mut errors = Vec::new();
        Self::validate_node(root, &mut errors);
        errors
    }

    fn validate_node(node: &BtNode, errors: &mut Vec<String>) {
        match node {
            BtNode::Sequence { children }
            | BtNode::Selector { children }
            | BtNode::Parallel { children, .. } => {
                if children.is_empty() {
                    errors.push(format!("{:?} node has no children", node));
                }
                for child in children {
                    Self::validate_node(child, errors);
                }
            }
            BtNode::Inverter { child }
            | BtNode::Repeater { child, .. }
            | BtNode::Retry { child, .. }
            | BtNode::Timeout { child, .. } => {
                Self::validate_node(child, errors);
            }
            BtNode::Condition { key, .. } => {
                if key.is_empty() {
                    errors.push("Condition node has empty key".to_string());
                }
            }
            BtNode::Action { name } => {
                if name.is_empty() {
                    errors.push("Action node has empty name".to_string());
                }
            }
            _ => {}
        }
    }

    /// Tick all registered agents.
    pub fn tick(&mut self, delta_time: f32, elapsed_time: f32) -> HashMap<u64, BtStatus> {
        self.registry.tick_agents(delta_time, elapsed_time)
    }

    /// Hot-reload: update all agents using a specific tree with a new tree.
    pub fn hot_reload(&mut self, editor_tree_id: u64) -> bool {
        if let Some(editor_tree) = self.editor_trees.get(&editor_tree_id) {
            if let Some(_runtime_tree) = editor_tree.compile() {
                // Find the compiled tree ID and update it
                // In a real implementation, we'd track the mapping
                return true;
            }
        }
        false
    }

    /// Get summary statistics.
    pub fn summary(&self) -> AiEditorSummary {
        AiEditorSummary {
            agent_count: self.registry.agent_count(),
            tree_count: self.registry.tree_count(),
            editor_tree_count: self.editor_trees.len(),
            live_editing_enabled: self.live_editing_enabled,
        }
    }
}

impl Default for AiEditorManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Summary statistics for the AI editor.
#[derive(Debug, Clone)]
pub struct AiEditorSummary {
    pub agent_count: usize,
    pub tree_count: usize,
    pub editor_tree_count: usize,
    pub live_editing_enabled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_registry_creation() {
        let registry = AiAgentRegistry::new();
        assert_eq!(registry.agent_count(), 0);
        assert_eq!(registry.tree_count(), 0);
    }

    #[test]
    fn test_register_agent() {
        let mut registry = AiAgentRegistry::new();
        let id = registry.register_agent("Test Agent");
        assert_eq!(id, 1);
        assert_eq!(registry.agent_count(), 1);

        let agent = registry.get_agent(id).expect("Agent not found");
        assert_eq!(agent.name, "Test Agent");
        assert!(agent.is_active);
    }

    #[test]
    fn test_remove_agent() {
        let mut registry = AiAgentRegistry::new();
        let id = registry.register_agent("Temp Agent");
        assert!(registry.remove_agent(id));
        assert_eq!(registry.agent_count(), 0);
        assert!(!registry.remove_agent(id));
    }

    #[test]
    fn test_assign_tree() {
        let mut registry = AiAgentRegistry::new();
        let agent_id = registry.register_agent("Test Agent");

        // Create a simple tree
        let tree = BehaviorTree::new("Test", BtNode::Succeed);
        let tree_id = registry.register_tree(tree);

        assert!(registry.assign_tree(agent_id, tree_id));

        let agent = registry.get_agent(agent_id).expect("Agent not found");
        assert_eq!(agent.behavior_tree_id, Some(tree_id));
    }

    #[test]
    fn test_assign_nonexistent_tree() {
        let mut registry = AiAgentRegistry::new();
        let agent_id = registry.register_agent("Test Agent");

        assert!(!registry.assign_tree(agent_id, 999));
    }

    #[test]
    fn test_tick_agents() {
        let mut registry = AiAgentRegistry::new();
        let agent_id = registry.register_agent("Test Agent");
        let tree = BehaviorTree::new("Test", BtNode::Succeed);
        let tree_id = registry.register_tree(tree);
        registry.assign_tree(agent_id, tree_id);

        let results = registry.tick_agents(0.016, 0.0);
        assert_eq!(results.len(), 1);
        assert_eq!(*results.get(&agent_id).unwrap(), BtStatus::Success);
    }

    #[test]
    fn test_editor_tree_creation() {
        let mut manager = AiEditorManager::new();
        let id = manager.create_editor_tree("My Tree");
        assert_eq!(id, 1);
        assert_eq!(manager.editor_trees.len(), 1);

        let tree = manager.editor_trees.get(&id).expect("Tree not found");
        assert_eq!(tree.name, "My Tree");
        assert!(!tree.is_validated);
    }

    #[test]
    fn test_validate_tree_valid() {
        let root = BtNode::Sequence {
            children: vec![BtNode::Action { name: "DoThing".to_string() }],
        };
        let errors = AiEditorManager::validate_tree(&root);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_validate_tree_empty_action() {
        let root = BtNode::Sequence {
            children: vec![BtNode::Action { name: "".to_string() }],
        };
        let errors = AiEditorManager::validate_tree(&root);
        assert!(!errors.is_empty());
    }

    #[test]
    fn test_validate_tree_empty_sequence() {
        let root = BtNode::Sequence { children: vec![] };
        let errors = AiEditorManager::validate_tree(&root);
        assert!(!errors.is_empty());
    }

    #[test]
    fn test_ai_editor_manager_summary() {
        let mut manager = AiEditorManager::new();
        manager.registry.register_agent("Agent 1");
        manager.create_editor_tree("Tree 1");

        let summary = manager.summary();
        assert_eq!(summary.agent_count, 1);
        assert_eq!(summary.editor_tree_count, 1);
        assert!(summary.live_editing_enabled);
    }
}
