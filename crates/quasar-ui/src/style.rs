//! UI styling — colors, sizing, flex properties.

/// RGBA color (each channel 0.0–1.0).
#[derive(Debug, Clone, Copy)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const WHITE: Self = Self {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };
    pub const BLACK: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    pub const TRANSPARENT: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };

    pub fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }
}

impl Default for Color {
    fn default() -> Self {
        Self::WHITE
    }
}

/// Anchor point for absolute positioning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    Center,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl Default for Anchor {
    fn default() -> Self {
        Self::TopLeft
    }
}

/// Flex direction for child layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexDirection {
    Row,
    Column,
}

impl Default for FlexDirection {
    fn default() -> Self {
        Self::Column
    }
}

/// A size dimension that can be fixed or flexible.
#[derive(Debug, Clone, Copy)]
pub enum SizeDimension {
    /// Fixed pixel size.
    Px(f32),
    /// Percentage of parent.
    Percent(f32),
    /// Fit to content.
    Auto,
}

impl Default for SizeDimension {
    fn default() -> Self {
        Self::Auto
    }
}

/// Styling for a UI node.
#[derive(Debug, Clone)]
pub struct UiStyle {
    /// Width.
    pub width: SizeDimension,
    /// Height.
    pub height: SizeDimension,
    /// Minimum width in pixels.
    pub min_width: f32,
    /// Minimum height in pixels.
    pub min_height: f32,
    /// Padding (top, right, bottom, left) in pixels.
    pub padding: [f32; 4],
    /// Margin (top, right, bottom, left) in pixels.
    pub margin: [f32; 4],
    /// Flex direction for children.
    pub flex_direction: FlexDirection,
    /// Gap between children in pixels.
    pub gap: f32,
    /// Background color.
    pub background_color: Color,
    /// Text color.
    pub text_color: Color,
    /// Font size in pixels.
    pub font_size: f32,
    /// Corner radius for rounded rectangles.
    pub border_radius: f32,
    /// Border width.
    pub border_width: f32,
    /// Border color.
    pub border_color: Color,
    /// Absolute positioning anchor.
    pub anchor: Anchor,
    /// Absolute position offset (only used with Anchor positioning).
    pub position: [f32; 2],
    /// Whether this node uses absolute positioning.
    pub absolute: bool,
    /// Whether this node is visible.
    pub visible: bool,
}

impl Default for UiStyle {
    fn default() -> Self {
        Self {
            width: SizeDimension::Auto,
            height: SizeDimension::Auto,
            min_width: 0.0,
            min_height: 0.0,
            padding: [0.0; 4],
            margin: [0.0; 4],
            flex_direction: FlexDirection::Column,
            gap: 0.0,
            background_color: Color::TRANSPARENT,
            text_color: Color::WHITE,
            font_size: 16.0,
            border_radius: 0.0,
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
            anchor: Anchor::TopLeft,
            position: [0.0; 2],
            absolute: false,
            visible: true,
        }
    }
}
