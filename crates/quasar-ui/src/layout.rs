//! Layout solver — computes screen-space rectangles for UI nodes.

use crate::style::{Anchor, FlexDirection, SizeDimension, UiStyle};
use crate::widget::{UiTree, WidgetId};

/// A computed screen-space rectangle.
#[derive(Debug, Clone, Copy, Default)]
pub struct LayoutRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl LayoutRect {
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px <= self.x + self.width && py >= self.y && py <= self.y + self.height
    }
}

/// Solves layout for a UiTree and produces a `LayoutRect` per widget.
pub struct LayoutSolver {
    /// Computed rectangles keyed by WidgetId.
    rects: Vec<(WidgetId, LayoutRect)>,
}

impl LayoutSolver {
    pub fn new() -> Self {
        Self { rects: Vec::new() }
    }

    /// Run layout for the given viewport size.
    pub fn solve(&mut self, tree: &UiTree, viewport_w: f32, viewport_h: f32) {
        self.rects.clear();

        let viewport = LayoutRect {
            x: 0.0,
            y: 0.0,
            width: viewport_w,
            height: viewport_h,
        };

        for &root in tree.roots() {
            self.layout_node(tree, root, &viewport);
        }
    }

    /// Retrieve the computed rect for a widget.
    pub fn rect(&self, id: WidgetId) -> Option<LayoutRect> {
        self.rects
            .iter()
            .find(|(wid, _)| *wid == id)
            .map(|(_, r)| *r)
    }

    /// All computed rects.
    pub fn rects(&self) -> &[(WidgetId, LayoutRect)] {
        &self.rects
    }

    fn layout_node(&mut self, tree: &UiTree, id: WidgetId, parent_rect: &LayoutRect) {
        let node = match tree.get(id) {
            Some(n) => n,
            None => return,
        };
        let style = &node.style;

        if !style.visible {
            return;
        }

        // Resolve size.
        let width = self.resolve_size(style.width, parent_rect.width, style.min_width);
        let height = self.resolve_size(style.height, parent_rect.height, style.min_height);

        // Resolve position.
        let (x, y) = if style.absolute {
            self.resolve_anchor(style, parent_rect, width, height)
        } else {
            // Relative positioning (assigned by parent during flex layout).
            (
                parent_rect.x + style.margin[3],
                parent_rect.y + style.margin[0],
            )
        };

        let rect = LayoutRect {
            x,
            y,
            width,
            height,
        };
        self.rects.push((id, rect));

        // Layout children with flex.
        let content_rect = LayoutRect {
            x: rect.x + style.padding[3],
            y: rect.y + style.padding[0],
            width: (rect.width - style.padding[1] - style.padding[3]).max(0.0),
            height: (rect.height - style.padding[0] - style.padding[2]).max(0.0),
        };

        let mut cursor_x = content_rect.x;
        let mut cursor_y = content_rect.y;

        for &child_id in &node.children {
            let child = match tree.get(child_id) {
                Some(c) => c,
                None => continue,
            };

            if child.style.absolute {
                // Absolute children layout against the parent rect.
                self.layout_node(tree, child_id, &rect);
                continue;
            }

            let child_w =
                self.resolve_size(child.style.width, content_rect.width, child.style.min_width);
            let child_h = self.resolve_size(
                child.style.height,
                content_rect.height,
                child.style.min_height,
            );

            let child_rect = LayoutRect {
                x: cursor_x + child.style.margin[3],
                y: cursor_y + child.style.margin[0],
                width: child_w,
                height: child_h,
            };

            // Push child rect.
            self.rects.push((child_id, child_rect));

            // Recursively layout grandchildren.
            let child_content_rect = LayoutRect {
                x: child_rect.x + child.style.padding[3],
                y: child_rect.y + child.style.padding[0],
                width: (child_rect.width - child.style.padding[1] - child.style.padding[3])
                    .max(0.0),
                height: (child_rect.height - child.style.padding[0] - child.style.padding[2])
                    .max(0.0),
            };

            // Layout grandchildren inside child.
            let gc_x = child_content_rect.x;
            let gc_y = child_content_rect.y;

            let gc_children: Vec<WidgetId> = tree
                .get(child_id)
                .map(|n| n.children.clone())
                .unwrap_or_default();

            for gc_id in gc_children {
                self.layout_node(
                    tree,
                    gc_id,
                    &LayoutRect {
                        x: gc_x,
                        y: gc_y,
                        width: child_content_rect.width,
                        height: child_content_rect.height,
                    },
                );
            }

            // Advance cursor.
            match style.flex_direction {
                FlexDirection::Row => {
                    cursor_x += child_w + child.style.margin[1] + child.style.margin[3] + style.gap;
                }
                FlexDirection::Column => {
                    cursor_y += child_h + child.style.margin[0] + child.style.margin[2] + style.gap;
                }
            }
        }
    }

    fn resolve_size(&self, dim: SizeDimension, parent_size: f32, min_size: f32) -> f32 {
        let raw = match dim {
            SizeDimension::Px(px) => px,
            SizeDimension::Percent(pct) => parent_size * pct / 100.0,
            SizeDimension::Auto => parent_size,
        };
        raw.max(min_size)
    }

    fn resolve_anchor(
        &self,
        style: &UiStyle,
        parent: &LayoutRect,
        width: f32,
        height: f32,
    ) -> (f32, f32) {
        let base = match style.anchor {
            Anchor::TopLeft => (parent.x, parent.y),
            Anchor::TopCenter => (parent.x + (parent.width - width) / 2.0, parent.y),
            Anchor::TopRight => (parent.x + parent.width - width, parent.y),
            Anchor::CenterLeft => (parent.x, parent.y + (parent.height - height) / 2.0),
            Anchor::Center => (
                parent.x + (parent.width - width) / 2.0,
                parent.y + (parent.height - height) / 2.0,
            ),
            Anchor::CenterRight => (
                parent.x + parent.width - width,
                parent.y + (parent.height - height) / 2.0,
            ),
            Anchor::BottomLeft => (parent.x, parent.y + parent.height - height),
            Anchor::BottomCenter => (
                parent.x + (parent.width - width) / 2.0,
                parent.y + parent.height - height,
            ),
            Anchor::BottomRight => (
                parent.x + parent.width - width,
                parent.y + parent.height - height,
            ),
        };
        (base.0 + style.position[0], base.1 + style.position[1])
    }
}

impl Default for LayoutSolver {
    fn default() -> Self {
        Self::new()
    }
}
