//! Styling types for compositing targets.

use embedded_graphics::primitives::{PrimitiveStyle, StrokeAlignment};
use embedded_graphics_core::pixelcolor::PixelColor;

/// Visual styling configuration supporting fill, stroke, and alpha blending.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct CompositingStyle<C> {
    pub fill_color: Option<C>,
    pub fill_alpha: Option<u8>,
    pub stroke_color: Option<C>,
    pub stroke_alpha: Option<u8>,
    pub stroke_width: u32,
    pub stroke_alignment: StrokeAlignment,
}

impl<C> Default for CompositingStyle<C> {
    #[inline]
    fn default() -> Self {
        Self {
            fill_color: None,
            fill_alpha: None,
            stroke_color: None,
            stroke_alpha: None,
            stroke_width: 0,
            stroke_alignment: StrokeAlignment::Center,
        }
    }
}

impl<C: PixelColor> CompositingStyle<C> {
    /// Creates a style with a solid opaque fill color.
    #[inline]
    pub const fn with_fill(color: C) -> Self {
        Self {
            fill_color: Some(color),
            fill_alpha: Some(255),
            stroke_color: None,
            stroke_alpha: None,
            stroke_width: 0,
            stroke_alignment: StrokeAlignment::Center,
        }
    }

    /// Creates a style with a translucent fill color and alpha factor.
    #[inline]
    pub const fn with_fill_alpha(color: C, alpha: u8) -> Self {
        Self {
            fill_color: Some(color),
            fill_alpha: Some(alpha),
            stroke_color: None,
            stroke_alpha: None,
            stroke_width: 0,
            stroke_alignment: StrokeAlignment::Center,
        }
    }

    /// Creates a style with a solid stroke.
    #[inline]
    pub const fn with_stroke(color: C, width: u32) -> Self {
        Self {
            fill_color: None,
            fill_alpha: None,
            stroke_color: Some(color),
            stroke_alpha: Some(255),
            stroke_width: width,
            stroke_alignment: StrokeAlignment::Center,
        }
    }

    /// Creates a style with a translucent stroke.
    #[inline]
    pub const fn with_stroke_alpha(color: C, width: u32, alpha: u8) -> Self {
        Self {
            fill_color: None,
            fill_alpha: None,
            stroke_color: Some(color),
            stroke_alpha: Some(alpha),
            stroke_width: width,
            stroke_alignment: StrokeAlignment::Center,
        }
    }
}

impl<C: PixelColor> From<PrimitiveStyle<C>> for CompositingStyle<C> {
    #[inline]
    fn from(style: PrimitiveStyle<C>) -> Self {
        Self {
            fill_color: style.fill_color,
            fill_alpha: style.fill_color.map(|_| 255),
            stroke_color: style.stroke_color,
            stroke_alpha: style.stroke_color.map(|_| 255),
            stroke_width: style.stroke_width,
            stroke_alignment: style.stroke_alignment,
        }
    }
}

/// A primitive combined with a visual style.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Styled<P, S> {
    pub primitive: P,
    pub style: S,
}

impl<P, S> Styled<P, S> {
    #[inline]
    pub const fn new(primitive: P, style: S) -> Self {
        Self { primitive, style }
    }
}

#[cfg(test)]
mod tests {
    use embedded_graphics::primitives::PrimitiveStyleBuilder;
    use embedded_graphics_core::pixelcolor::Rgb565;

    use super::*;

    #[test]
    fn test_compositing_style_constructors_and_conversions() {
        let red = Rgb565::new(31, 0, 0);
        let blue = Rgb565::new(0, 0, 31);

        let default_style: CompositingStyle<Rgb565> = CompositingStyle::default();
        assert_eq!(default_style.fill_color, None);
        assert_eq!(default_style.stroke_color, None);

        let fill_style = CompositingStyle::with_fill(red);
        assert_eq!(fill_style.fill_color, Some(red));
        assert_eq!(fill_style.fill_alpha, Some(255));

        let fill_alpha_style = CompositingStyle::with_fill_alpha(red, 128);
        assert_eq!(fill_alpha_style.fill_alpha, Some(128));

        let stroke_style = CompositingStyle::with_stroke(blue, 2);
        assert_eq!(stroke_style.stroke_color, Some(blue));
        assert_eq!(stroke_style.stroke_width, 2);
        assert_eq!(stroke_style.stroke_alpha, Some(255));

        let stroke_alpha_style = CompositingStyle::with_stroke_alpha(blue, 3, 100);
        assert_eq!(stroke_alpha_style.stroke_alpha, Some(100));

        let prim_style = PrimitiveStyleBuilder::new()
            .fill_color(red)
            .stroke_color(blue)
            .stroke_width(4)
            .build();
        let converted: CompositingStyle<Rgb565> = prim_style.into();
        assert_eq!(converted.fill_color, Some(red));
        assert_eq!(converted.fill_alpha, Some(255));
        assert_eq!(converted.stroke_color, Some(blue));
        assert_eq!(converted.stroke_alpha, Some(255));
        assert_eq!(converted.stroke_width, 4);
    }
}
