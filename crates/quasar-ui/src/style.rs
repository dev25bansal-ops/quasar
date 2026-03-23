//! UI styling — colors, sizing, flex properties.

/// RGBA color (each channel 0.0–1.0).
#[derive(Debug, Clone, Copy, PartialEq)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Anchor {
    #[default]
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

/// Flex direction for child layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlexDirection {
    Row,
    #[default]
    Column,
}

/// A size dimension that can be fixed or flexible.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum SizeDimension {
    /// Fixed pixel size.
    Px(f32),
    /// Percentage of parent.
    Percent(f32),
    /// Fit to content.
    #[default]
    Auto,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_constants() {
        assert_eq!(Color::WHITE.r, 1.0);
        assert_eq!(Color::WHITE.g, 1.0);
        assert_eq!(Color::WHITE.b, 1.0);
        assert_eq!(Color::WHITE.a, 1.0);

        assert_eq!(Color::BLACK.r, 0.0);
        assert_eq!(Color::BLACK.a, 1.0);

        assert_eq!(Color::TRANSPARENT.a, 0.0);
    }

    #[test]
    fn test_color_rgba() {
        let color = Color::rgba(0.5, 0.6, 0.7, 0.8);
        assert_eq!(color.r, 0.5);
        assert_eq!(color.g, 0.6);
        assert_eq!(color.b, 0.7);
        assert_eq!(color.a, 0.8);
    }

    #[test]
    fn test_color_default() {
        let color = Color::default();
        assert_eq!(color.r, Color::WHITE.r);
        assert_eq!(color.a, 1.0);
    }

    #[test]
    fn test_color_clone() {
        let color = Color::rgba(0.1, 0.2, 0.3, 0.4);
        let cloned = color.clone();
        assert_eq!(cloned.r, 0.1);
        assert_eq!(cloned.g, 0.2);
        assert_eq!(cloned.b, 0.3);
        assert_eq!(cloned.a, 0.4);
    }

    #[test]
    fn test_anchor_default() {
        let anchor = Anchor::default();
        assert_eq!(anchor, Anchor::TopLeft);
    }

    #[test]
    fn test_anchor_variants() {
        let variants = [
            Anchor::TopLeft,
            Anchor::TopCenter,
            Anchor::TopRight,
            Anchor::CenterLeft,
            Anchor::Center,
            Anchor::CenterRight,
            Anchor::BottomLeft,
            Anchor::BottomCenter,
            Anchor::BottomRight,
        ];

        for (i, v1) in variants.iter().enumerate() {
            for (j, v2) in variants.iter().enumerate() {
                if i == j {
                    assert_eq!(v1, v2);
                } else {
                    assert_ne!(v1, v2);
                }
            }
        }
    }

    #[test]
    fn test_flex_direction_default() {
        let direction = FlexDirection::default();
        assert_eq!(direction, FlexDirection::Column);
    }

    #[test]
    fn test_flex_direction_variants() {
        assert_eq!(FlexDirection::Row, FlexDirection::Row);
        assert_eq!(FlexDirection::Column, FlexDirection::Column);
        assert_ne!(FlexDirection::Row, FlexDirection::Column);
    }

    #[test]
    fn test_size_dimension_default() {
        let size = SizeDimension::default();
        assert_eq!(size, SizeDimension::Auto);
    }

    #[test]
    fn test_size_dimension_px() {
        let size = SizeDimension::Px(100.0);
        if let SizeDimension::Px(v) = size {
            assert_eq!(v, 100.0);
        } else {
            panic!("Expected Px variant");
        }
    }

    #[test]
    fn test_size_dimension_percent() {
        let size = SizeDimension::Percent(50.0);
        if let SizeDimension::Percent(v) = size {
            assert_eq!(v, 50.0);
        } else {
            panic!("Expected Percent variant");
        }
    }

    #[test]
    fn test_size_dimension_clone() {
        let size = SizeDimension::Px(200.0);
        let cloned = size.clone();
        assert_eq!(cloned, SizeDimension::Px(200.0));
    }

    #[test]
    fn test_ui_style_default() {
        let style = UiStyle::default();

        assert_eq!(style.width, SizeDimension::Auto);
        assert_eq!(style.height, SizeDimension::Auto);
        assert_eq!(style.min_width, 0.0);
        assert_eq!(style.min_height, 0.0);
        assert_eq!(style.padding, [0.0; 4]);
        assert_eq!(style.margin, [0.0; 4]);
        assert_eq!(style.flex_direction, FlexDirection::Column);
        assert_eq!(style.gap, 0.0);
        assert_eq!(style.background_color, Color::TRANSPARENT);
        assert_eq!(style.text_color, Color::WHITE);
        assert_eq!(style.font_size, 16.0);
        assert_eq!(style.border_radius, 0.0);
        assert_eq!(style.border_width, 0.0);
        assert_eq!(style.border_color, Color::TRANSPARENT);
        assert_eq!(style.anchor, Anchor::TopLeft);
        assert_eq!(style.position, [0.0; 2]);
        assert!(!style.absolute);
        assert!(style.visible);
    }

    #[test]
    fn test_ui_style_clone() {
        let style = UiStyle {
            width: SizeDimension::Px(100.0),
            height: SizeDimension::Px(50.0),
            font_size: 24.0,
            ..Default::default()
        };
        let cloned = style.clone();

        assert_eq!(cloned.width, SizeDimension::Px(100.0));
        assert_eq!(cloned.height, SizeDimension::Px(50.0));
        assert_eq!(cloned.font_size, 24.0);
    }

    #[test]
    fn test_ui_style_visible() {
        let mut style = UiStyle::default();
        style.visible = false;
        assert!(!style.visible);
    }

    #[test]
    fn test_ui_style_absolute() {
        let mut style = UiStyle::default();
        style.absolute = true;
        style.position = [100.0, 200.0];
        style.anchor = Anchor::Center;

        assert!(style.absolute);
        assert_eq!(style.position, [100.0, 200.0]);
        assert_eq!(style.anchor, Anchor::Center);
    }
}
