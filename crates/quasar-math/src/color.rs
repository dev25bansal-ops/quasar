//! Color types for the Quasar Engine.

use bytemuck::{Pod, Zeroable};

/// Linear RGBA color (0.0–1.0 per channel).
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
#[repr(C)]
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
    pub const RED: Self = Self {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    pub const GREEN: Self = Self {
        r: 0.0,
        g: 1.0,
        b: 0.0,
        a: 1.0,
    };
    pub const BLUE: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 1.0,
        a: 1.0,
    };
    pub const YELLOW: Self = Self {
        r: 1.0,
        g: 1.0,
        b: 0.0,
        a: 1.0,
    };
    pub const CYAN: Self = Self {
        r: 0.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };
    pub const MAGENTA: Self = Self {
        r: 1.0,
        g: 0.0,
        b: 1.0,
        a: 1.0,
    };
    pub const TRANSPARENT: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };

    /// Cornflower blue — the classic default clear color.
    pub const CORNFLOWER_BLUE: Self = Self {
        r: 0.392,
        g: 0.584,
        b: 0.929,
        a: 1.0,
    };

    /// Create a color from RGB (alpha = 1.0).
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// Create a color from RGBA.
    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Create from 0–255 integer values.
    pub fn from_u8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        }
    }

    /// Convert to a `[f32; 4]` array (useful for GPU uniforms).
    pub fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    /// Convert to `(f64, f64, f64, f64)` tuple (useful for GPU clear colors).
    pub fn to_f64_tuple(self) -> (f64, f64, f64, f64) {
        (self.r as f64, self.g as f64, self.b as f64, self.a as f64)
    }
}

impl From<[f32; 4]> for Color {
    fn from(arr: [f32; 4]) -> Self {
        Self {
            r: arr[0],
            g: arr[1],
            b: arr[2],
            a: arr[3],
        }
    }
}

impl From<[f32; 3]> for Color {
    fn from(arr: [f32; 3]) -> Self {
        Self::rgb(arr[0], arr[1], arr[2])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_sets_alpha_to_one() {
        let c = Color::rgb(0.5, 0.6, 0.7);
        assert_eq!(c.a, 1.0);
        assert_eq!(c.r, 0.5);
    }

    #[test]
    fn rgba_preserves_all_channels() {
        let c = Color::rgba(0.1, 0.2, 0.3, 0.4);
        assert_eq!(c.to_array(), [0.1, 0.2, 0.3, 0.4]);
    }

    #[test]
    fn from_u8_converts_correctly() {
        let c = Color::from_u8(255, 0, 128, 255);
        assert!((c.r - 1.0).abs() < 1e-6);
        assert!((c.g - 0.0).abs() < 1e-6);
        assert!((c.b - 128.0 / 255.0).abs() < 1e-6);
        assert!((c.a - 1.0).abs() < 1e-6);
    }

    #[test]
    fn to_f64_tuple_matches() {
        let c = Color::RED;
        let (r, g, b, a) = c.to_f64_tuple();
        assert_eq!(r, 1.0);
        assert_eq!(g, 0.0);
        assert_eq!(b, 0.0);
        assert_eq!(a, 1.0);
    }

    #[test]
    fn from_array4() {
        let c: Color = [0.1, 0.2, 0.3, 0.4].into();
        assert_eq!(c.r, 0.1);
        assert_eq!(c.a, 0.4);
    }

    #[test]
    fn from_array3_alpha_is_one() {
        let c: Color = [0.1, 0.2, 0.3].into();
        assert_eq!(c.a, 1.0);
    }

    #[test]
    fn constant_colors_are_opaque() {
        for c in [
            Color::WHITE,
            Color::BLACK,
            Color::RED,
            Color::GREEN,
            Color::BLUE,
            Color::YELLOW,
            Color::CYAN,
            Color::MAGENTA,
            Color::CORNFLOWER_BLUE,
        ] {
            assert_eq!(c.a, 1.0, "constant color should be opaque");
        }
        assert_eq!(Color::TRANSPARENT.a, 0.0);
    }
}
