//! Widget tree — the retained-mode UI data structure.

use crate::style::UiStyle;

/// Opaque identifier for a widget in the UI tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WidgetId(pub u32);

/// Content inside a UI node.
#[derive(Debug, Clone)]
pub enum UiContent {
    /// No content — container only.
    None,
    /// Text label.
    Text(String),
    /// Image (path to texture asset).
    Image(String),
}

impl Default for UiContent {
    fn default() -> Self {
        Self::None
    }
}

/// UI interaction state.
#[derive(Debug, Clone, Copy, Default)]
pub struct UiInteraction {
    /// Cursor is hovering this node.
    pub hovered: bool,
    /// Mouse button is pressed on this node.
    pub pressed: bool,
    /// Mouse was released over this node (click).
    pub clicked: bool,
}

/// A single node in the UI tree.
#[derive(Debug, Clone)]
pub struct UiNode {
    pub id: WidgetId,
    pub style: UiStyle,
    pub content: UiContent,
    pub children: Vec<WidgetId>,
    pub parent: Option<WidgetId>,
    pub interaction: UiInteraction,
}

impl UiNode {
    pub fn new(id: WidgetId) -> Self {
        Self {
            id,
            style: UiStyle::default(),
            content: UiContent::None,
            children: Vec::new(),
            parent: None,
            interaction: UiInteraction::default(),
        }
    }

    pub fn with_style(mut self, style: UiStyle) -> Self {
        self.style = style;
        self
    }

    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.content = UiContent::Text(text.into());
        self
    }

    pub fn with_image(mut self, path: impl Into<String>) -> Self {
        self.content = UiContent::Image(path.into());
        self
    }
}

/// Retained-mode UI tree.
///
/// Nodes are stored flat; parent–child relationships are tracked by IDs.
pub struct UiTree {
    nodes: Vec<UiNode>,
    next_id: u32,
    /// Root nodes (no parent).
    roots: Vec<WidgetId>,
}

impl UiTree {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            next_id: 0,
            roots: Vec::new(),
        }
    }

    /// Add a root node and return its ID.
    pub fn add_root(&mut self, style: UiStyle) -> WidgetId {
        let id = self.alloc_id();
        let node = UiNode::new(id).with_style(style);
        self.nodes.push(node);
        self.roots.push(id);
        id
    }

    /// Add a child node under `parent`.
    pub fn add_child(&mut self, parent: WidgetId, style: UiStyle) -> WidgetId {
        let id = self.alloc_id();
        let mut node = UiNode::new(id).with_style(style);
        node.parent = Some(parent);
        self.nodes.push(node);
        if let Some(p) = self.get_mut(parent) {
            p.children.push(id);
        }
        id
    }

    /// Add a text node under `parent`.
    pub fn add_text(
        &mut self,
        parent: WidgetId,
        text: impl Into<String>,
        style: UiStyle,
    ) -> WidgetId {
        let id = self.alloc_id();
        let mut node = UiNode::new(id).with_style(style).with_text(text);
        node.parent = Some(parent);
        self.nodes.push(node);
        if let Some(p) = self.get_mut(parent) {
            p.children.push(id);
        }
        id
    }

    /// Get a node by ID.
    pub fn get(&self, id: WidgetId) -> Option<&UiNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// Get a mutable node by ID.
    pub fn get_mut(&mut self, id: WidgetId) -> Option<&mut UiNode> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    /// Remove a node (and recursively its children).
    pub fn remove(&mut self, id: WidgetId) {
        // Collect children first.
        let children: Vec<WidgetId> = self
            .get(id)
            .map(|n| n.children.clone())
            .unwrap_or_default();
        for child in children {
            self.remove(child);
        }
        // Remove from parent's child list.
        if let Some(parent_id) = self.get(id).and_then(|n| n.parent) {
            if let Some(parent) = self.get_mut(parent_id) {
                parent.children.retain(|c| *c != id);
            }
        }
        self.roots.retain(|r| *r != id);
        self.nodes.retain(|n| n.id != id);
    }

    /// Iterate over all root widget IDs.
    pub fn roots(&self) -> &[WidgetId] {
        &self.roots
    }

    /// Total number of nodes.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Clear all nodes.
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.roots.clear();
    }

    fn alloc_id(&mut self) -> WidgetId {
        let id = WidgetId(self.next_id);
        self.next_id += 1;
        id
    }
}

impl Default for UiTree {
    fn default() -> Self {
        Self::new()
    }
}
