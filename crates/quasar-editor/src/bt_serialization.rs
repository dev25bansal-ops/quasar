//! Behavior Tree Serialization for Quasar Editor.
//!
//! Provides:
//! - **JSON export** — serialize graph to JSON string
//! - **JSON import** — deserialize JSON string to graph
//! - **Runtime conversion** — convert graph to runtime BtNode
//! - **Validation on load** — ensure imported trees are valid
//! - **Version migration** — handle format version changes

#![allow(deprecated)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::behavior_tree_graph::{
    BtEditorConnection, BtEditorNode, BtEditorNodeType, BtGraphState, GraphConnectionId,
    GraphNodeId,
};

/// Version of the serialization format.
pub const FORMAT_VERSION: u32 = 1;

/// Serialized representation of a behavior tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BtSerialized {
    /// Format version.
    pub version: u32,
    /// Tree metadata.
    pub metadata: BtMetadata,
    /// Serialized nodes.
    pub nodes: Vec<SerializedNode>,
    /// Serialized connections.
    pub connections: Vec<SerializedConnection>,
    /// Root node ID.
    pub root_node_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BtMetadata {
    /// Tree name.
    pub name: String,
    /// Optional description.
    pub description: String,
    /// Tags for organization.
    pub tags: Vec<String>,
    /// Timestamp when the tree was created.
    pub created_at: f64,
    /// Timestamp when the tree was last modified.
    pub modified_at: f64,
    /// Editor version that last saved this tree.
    pub editor_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedNode {
    /// Unique node ID.
    pub id: u64,
    /// Node type name.
    pub node_type: String,
    /// Display name.
    pub name: String,
    /// Position [x, y].
    pub position: [f32; 2],
    /// Node properties as key-value pairs.
    pub properties: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedConnection {
    /// Unique connection ID.
    pub id: u64,
    /// Source node ID.
    pub from: u64,
    /// Target node ID.
    pub to: u64,
    /// Order among siblings.
    pub order: u32,
}

/// Serializer for behavior tree graphs.
pub struct BtSerializer;

impl BtSerializer {
    /// Serialize a graph state to a JSON string.
    pub fn serialize_tree(graph: &BtGraphState) -> Result<String, String> {
        let serialized = BtSerialized {
            version: FORMAT_VERSION,
            metadata: BtMetadata {
                name: graph.name.clone(),
                description: String::new(),
                tags: Vec::new(),
                created_at: 0.0,
                modified_at: 0.0,
                editor_version: env!("CARGO_PKG_VERSION").to_string(),
            },
            nodes: graph
                .nodes
                .values()
                .map(|n| SerializedNode {
                    id: n.id.0,
                    node_type: format!("{:?}", n.node_type),
                    name: n.name.clone(),
                    position: n.position,
                    properties: n.properties.clone(),
                })
                .collect(),
            connections: graph
                .connections
                .values()
                .map(|c| SerializedConnection {
                    id: c.id.0,
                    from: c.from.0,
                    to: c.to.0,
                    order: c.order,
                })
                .collect(),
            root_node_id: graph.root_node.map(|id| id.0),
        };

        serde_json::to_string_pretty(&serialized).map_err(|e| format!("Serialization error: {}", e))
    }

    /// Serialize with metadata.
    pub fn serialize_tree_with_metadata(
        graph: &BtGraphState,
        description: &str,
        tags: &[String],
    ) -> Result<String, String> {
        let serialized = BtSerialized {
            version: FORMAT_VERSION,
            metadata: BtMetadata {
                name: graph.name.clone(),
                description: description.to_string(),
                tags: tags.to_vec(),
                created_at: 0.0,
                modified_at: 0.0,
                editor_version: env!("CARGO_PKG_VERSION").to_string(),
            },
            nodes: graph
                .nodes
                .values()
                .map(|n| SerializedNode {
                    id: n.id.0,
                    node_type: format!("{:?}", n.node_type),
                    name: n.name.clone(),
                    position: n.position,
                    properties: n.properties.clone(),
                })
                .collect(),
            connections: graph
                .connections
                .values()
                .map(|c| SerializedConnection {
                    id: c.id.0,
                    from: c.from.0,
                    to: c.to.0,
                    order: c.order,
                })
                .collect(),
            root_node_id: graph.root_node.map(|id| id.0),
        };

        serde_json::to_string_pretty(&serialized).map_err(|e| format!("Serialization error: {}", e))
    }
}

/// Deserializer for behavior tree graphs.
pub struct BtDeserializer;

impl BtDeserializer {
    /// Deserialize a JSON string to a graph state.
    pub fn deserialize_tree(json: &str) -> Result<BtGraphState, String> {
        let serialized: BtSerialized =
            serde_json::from_str(json).map_err(|e| format!("Deserialization error: {}", e))?;

        // Check version compatibility
        if serialized.version != FORMAT_VERSION {
            // Could implement version migration here
            return Err(format!(
                "Unsupported format version: {} (expected {})",
                serialized.version, FORMAT_VERSION
            ));
        }

        // Build the graph
        let mut graph = BtGraphState::new(&serialized.metadata.name);
        graph.root_node = serialized.root_node_id.map(GraphNodeId);

        // Track max IDs
        let max_node_id = serialized.nodes.iter().map(|n| n.id).max().unwrap_or(0);
        let max_conn_id = serialized
            .connections
            .iter()
            .map(|c| c.id)
            .max()
            .unwrap_or(0);
        graph.next_node_id = max_node_id + 1;
        graph.next_connection_id = max_conn_id + 1;

        // Deserialize nodes
        for sn in &serialized.nodes {
            let node_type = Self::parse_node_type(&sn.node_type)
                .ok_or_else(|| format!("Unknown node type: {}", sn.node_type))?;

            let node = BtEditorNode {
                id: GraphNodeId(sn.id),
                node_type,
                name: sn.name.clone(),
                position: sn.position,
                properties: sn.properties.clone(),
                sim_status: None,
            };

            graph.nodes.insert(node.id, node);
        }

        // Deserialize connections
        for sc in &serialized.connections {
            let conn = BtEditorConnection {
                id: GraphConnectionId(sc.id),
                from: GraphNodeId(sc.from),
                to: GraphNodeId(sc.to),
                order: sc.order,
            };
            graph.connections.insert(conn.id, conn);
        }

        // Validate the tree
        let errors = graph.validate();
        if !errors.is_empty() {
            return Err(format!("Validation errors: {}", errors.join(", ")));
        }

        Ok(graph)
    }

    /// Parse a node type from its string representation.
    fn parse_node_type(s: &str) -> Option<BtEditorNodeType> {
        match s {
            "Selector" => Some(BtEditorNodeType::Selector),
            "Sequence" => Some(BtEditorNodeType::Sequence),
            "Parallel" => Some(BtEditorNodeType::Parallel),
            "RandomSelector" => Some(BtEditorNodeType::RandomSelector),
            "RandomSequence" => Some(BtEditorNodeType::RandomSequence),
            "Inverter" => Some(BtEditorNodeType::Inverter),
            "Repeater" => Some(BtEditorNodeType::Repeater),
            "Succeeder" => Some(BtEditorNodeType::Succeeder),
            "Failer" => Some(BtEditorNodeType::Failer),
            "Timeout" => Some(BtEditorNodeType::Timeout),
            "Cooldown" => Some(BtEditorNodeType::Cooldown),
            "Retry" => Some(BtEditorNodeType::Retry),
            "AlwaysRunning" => Some(BtEditorNodeType::AlwaysRunning),
            "Action" => Some(BtEditorNodeType::Action),
            "Condition" => Some(BtEditorNodeType::Condition),
            "Wait" => Some(BtEditorNodeType::Wait),
            "SetBlackboard" => Some(BtEditorNodeType::SetBlackboard),
            "Log" => Some(BtEditorNodeType::Log),
            "Comment" => Some(BtEditorNodeType::Comment),
            _ => None,
        }
    }

    /// Try to migrate an older format version to the current version.
    pub fn migrate_from(json: &str, from_version: u32) -> Result<String, String> {
        match from_version {
            0 => Self::migrate_v0_to_v1(json),
            v if v == FORMAT_VERSION => Ok(json.to_string()),
            v => Err(format!("Cannot migrate from version {}", v)),
        }
    }

    /// Migrate from v0 (no version field) to v1.
    fn migrate_v0_to_v1(json: &str) -> Result<String, String> {
        // Try to parse as the old format (just nodes and connections)
        #[derive(Debug, Deserialize)]
        struct OldFormat {
            name: String,
            nodes: Vec<SerializedNode>,
            connections: Vec<SerializedConnection>,
            root_node_id: Option<u64>,
        }

        let old: OldFormat =
            serde_json::from_str(json).map_err(|e| format!("Failed to parse old format: {}", e))?;

        let new = BtSerialized {
            version: FORMAT_VERSION,
            metadata: BtMetadata {
                name: old.name,
                description: String::new(),
                tags: Vec::new(),
                created_at: 0.0,
                modified_at: 0.0,
                editor_version: env!("CARGO_PKG_VERSION").to_string(),
            },
            nodes: old.nodes,
            connections: old.connections,
            root_node_id: old.root_node_id,
        };

        serde_json::to_string_pretty(&new)
            .map_err(|e| format!("Failed to serialize migrated format: {}", e))
    }
}

/// Converter from editor graph to runtime behavior tree.
pub struct BtRuntimeConverter;

impl BtRuntimeConverter {
    /// Convert a graph state to a runtime BtNode tree.
    pub fn to_runtime(graph: &BtGraphState) -> Option<quasar_ai::behavior_tree::BtNode> {
        let root_id = graph.root_node?;
        Self::convert_node(graph, root_id)
    }

    fn convert_node(
        graph: &BtGraphState,
        node_id: GraphNodeId,
    ) -> Option<quasar_ai::behavior_tree::BtNode> {
        use quasar_ai::behavior_tree::BtNode as RuntimeNode;
        use quasar_ai::behavior_tree::ParallelPolicy;
        use quasar_ai::BlackboardValue;

        let node = graph.nodes.get(&node_id)?;
        let children: Vec<_> = graph
            .children_of(node_id)
            .iter()
            .filter_map(|child| Self::convert_node(graph, child.id))
            .collect();

        let result = match node.node_type {
            BtEditorNodeType::Selector => RuntimeNode::Selector { children },
            BtEditorNodeType::Sequence => RuntimeNode::Sequence { children },
            BtEditorNodeType::Parallel => {
                let policy =
                    if node.properties.get("policy").map(|s| s.as_str()) == Some("RequireOne") {
                        ParallelPolicy::RequireOne
                    } else {
                        ParallelPolicy::RequireAll
                    };
                RuntimeNode::Parallel { children, policy }
            }
            BtEditorNodeType::RandomSelector => RuntimeNode::Selector { children },
            BtEditorNodeType::RandomSequence => RuntimeNode::Sequence { children },

            BtEditorNodeType::Inverter => {
                if let Some(child) = children.into_iter().next() {
                    RuntimeNode::Inverter {
                        child: Box::new(child),
                    }
                } else {
                    RuntimeNode::Succeed
                }
            }
            BtEditorNodeType::Repeater => {
                if let Some(child) = children.into_iter().next() {
                    let count = node
                        .properties
                        .get("count")
                        .and_then(|s| s.parse::<u32>().ok());
                    RuntimeNode::Repeater {
                        child: Box::new(child),
                        count,
                    }
                } else {
                    RuntimeNode::Succeed
                }
            }
            BtEditorNodeType::Succeeder => RuntimeNode::Succeed,
            BtEditorNodeType::Failer => RuntimeNode::Fail,
            BtEditorNodeType::Timeout => {
                if let Some(child) = children.into_iter().next() {
                    let duration = node
                        .properties
                        .get("timeout")
                        .and_then(|s| s.parse::<f32>().ok())
                        .unwrap_or(5.0);
                    RuntimeNode::Timeout {
                        child: Box::new(child),
                        duration_secs: duration,
                    }
                } else {
                    RuntimeNode::Succeed
                }
            }
            BtEditorNodeType::Cooldown => RuntimeNode::Succeed,
            BtEditorNodeType::Retry => {
                if let Some(child) = children.into_iter().next() {
                    let max_tries = node
                        .properties
                        .get("max_retries")
                        .and_then(|s| s.parse::<u32>().ok())
                        .unwrap_or(3);
                    RuntimeNode::Retry {
                        child: Box::new(child),
                        max_tries,
                    }
                } else {
                    RuntimeNode::Succeed
                }
            }
            BtEditorNodeType::AlwaysRunning => RuntimeNode::Running,

            BtEditorNodeType::Action => {
                let action_name = node
                    .properties
                    .get("action_name")
                    .cloned()
                    .unwrap_or_else(|| node.name.clone());
                RuntimeNode::Action { name: action_name }
            }
            BtEditorNodeType::Condition => {
                let key = node.properties.get("key").cloned().unwrap_or_default();
                let expected = Self::parse_blackboard_value(
                    node.properties
                        .get("expected")
                        .cloned()
                        .unwrap_or_else(|| "true".to_string()),
                );
                RuntimeNode::Condition { key, expected }
            }
            BtEditorNodeType::Wait => {
                let duration = node
                    .properties
                    .get("duration")
                    .and_then(|s| s.parse::<f32>().ok())
                    .unwrap_or(1.0);
                RuntimeNode::Wait {
                    duration_secs: duration,
                }
            }
            BtEditorNodeType::SetBlackboard => {
                let key = node.properties.get("key").cloned().unwrap_or_default();
                RuntimeNode::Action {
                    name: format!("SetBB({})", key),
                }
            }
            BtEditorNodeType::Log => {
                let msg = node.properties.get("message").cloned().unwrap_or_default();
                RuntimeNode::Action {
                    name: format!("Log({})", msg),
                }
            }
            BtEditorNodeType::Comment => RuntimeNode::Succeed,
        };

        Some(result)
    }

    /// Parse a string into a BlackboardValue.
    fn parse_blackboard_value(s: String) -> quasar_ai::BlackboardValue {
        use quasar_ai::BlackboardValue;

        if s == "true" {
            BlackboardValue::Bool(true)
        } else if s == "false" {
            BlackboardValue::Bool(false)
        } else if let Ok(i) = s.parse::<i64>() {
            BlackboardValue::Int(i)
        } else if let Ok(f) = s.parse::<f32>() {
            BlackboardValue::Float(f)
        } else {
            BlackboardValue::String(s)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Pos2;

    fn create_test_graph() -> BtGraphState {
        let mut graph = BtGraphState::new("Test Tree");
        let root = graph.add_node(BtEditorNodeType::Selector, "Root", Pos2::new(100.0, 50.0));
        let seq = graph.add_node(BtEditorNodeType::Sequence, "Patrol", Pos2::new(50.0, 150.0));
        let action = graph.add_node(BtEditorNodeType::Action, "MoveToWP", Pos2::new(50.0, 250.0));
        let cond = graph.add_node(
            BtEditorNodeType::Condition,
            "HasTarget?",
            Pos2::new(200.0, 150.0),
        );

        graph.add_connection(root, seq);
        graph.add_connection(root, cond);
        graph.add_connection(seq, action);

        graph
    }

    #[test]
    fn test_serialize_deserialize_roundtrip() {
        let graph = create_test_graph();
        let json = BtSerializer::serialize_tree(&graph).expect("Failed to serialize");
        let restored = BtDeserializer::deserialize_tree(&json).expect("Failed to deserialize");

        assert_eq!(restored.name, graph.name);
        assert_eq!(restored.nodes.len(), graph.nodes.len());
        assert_eq!(restored.connections.len(), graph.connections.len());
        assert_eq!(restored.root_node, graph.root_node);
    }

    #[test]
    fn test_serialize_produces_valid_json() {
        let graph = create_test_graph();
        let json = BtSerializer::serialize_tree(&graph).expect("Failed to serialize");

        // Should parse as JSON
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("Invalid JSON");
        assert!(parsed.get("version").is_some());
        assert!(parsed.get("nodes").is_some());
        assert!(parsed.get("connections").is_some());
    }

    #[test]
    fn test_deserialize_invalid_json() {
        let result = BtDeserializer::deserialize_tree("not json");
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_unknown_node_type() {
        let json = r#"{
            "version": 1,
            "metadata": { "name": "Bad", "description": "", "tags": [], "created_at": 0, "modified_at": 0, "editor_version": "0.1.0" },
            "nodes": [{"id": 1, "node_type": "UnknownType", "name": "Bad", "position": [0,0], "properties": {}}],
            "connections": [],
            "root_node_id": 1
        }"#;
        let result = BtDeserializer::deserialize_tree(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_version_mismatch() {
        let json = r#"{
            "version": 999,
            "metadata": { "name": "Future", "description": "", "tags": [], "created_at": 0, "modified_at": 0, "editor_version": "0.1.0" },
            "nodes": [],
            "connections": [],
            "root_node_id": null
        }"#;
        let result = BtDeserializer::deserialize_tree(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_migrate_v0_to_v1() {
        let old_json = r#"{
            "name": "Old Tree",
            "nodes": [{"id": 1, "node_type": "Action", "name": "Act", "position": [0,0], "properties": {}}],
            "connections": [],
            "root_node_id": 1
        }"#;
        let result = BtDeserializer::migrate_from(old_json, 0);
        assert!(result.is_ok());

        let migrated = result.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&migrated).expect("Invalid JSON");
        assert_eq!(parsed.get("version").and_then(|v| v.as_u64()), Some(1));
    }

    #[test]
    fn test_runtime_conversion() {
        let graph = create_test_graph();
        let runtime = BtRuntimeConverter::to_runtime(&graph);
        assert!(runtime.is_some());
    }

    #[test]
    fn test_blackboard_value_parsing() {
        assert!(matches!(
            BtRuntimeConverter::parse_blackboard_value("true".to_string()),
            quasar_ai::BlackboardValue::Bool(true)
        ));
        assert!(matches!(
            BtRuntimeConverter::parse_blackboard_value("42".to_string()),
            quasar_ai::BlackboardValue::Int(42)
        ));
        assert!(matches!(
            BtRuntimeConverter::parse_blackboard_value("3.14".to_string()),
            quasar_ai::BlackboardValue::Float(f) if (f - 3.14).abs() < 0.001
        ));
        assert!(matches!(
            BtRuntimeConverter::parse_blackboard_value("hello".to_string()),
            quasar_ai::BlackboardValue::String(s) if s == "hello"
        ));
    }

    #[test]
    fn test_serialize_with_metadata() {
        let graph = create_test_graph();
        let tags = vec!["combat".to_string(), "patrol".to_string()];
        let json = BtSerializer::serialize_tree_with_metadata(&graph, "A patrol tree", &tags);
        assert!(json.is_ok());

        let parsed: serde_json::Value = serde_json::from_str(&json.unwrap()).expect("Invalid JSON");
        let meta = parsed.get("metadata").expect("Missing metadata");
        assert_eq!(
            meta.get("description").and_then(|v| v.as_str()),
            Some("A patrol tree")
        );
        assert_eq!(
            meta.get("tags").and_then(|v| v.as_array()).map(|a| a.len()),
            Some(2)
        );
    }
}
