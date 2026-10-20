//! Compositing target trait extending `embedded-graphics` DrawTarget.

use embedded_graphics_core::draw_target::DrawTarget;

/// A drawing target that supports span-oriented compositing and alpha blending.
///
/// Extends `embedded_graphics_core::draw_target::DrawTarget` to enable direct
/// compatibility with standard embedded fonts, text, and icons while providing
/// accelerated span fills and alpha blending routines directly in local SRAM.
pub trait CompositingTarget: DrawTarget {
    /// Fills a contiguous horizontal span with solid opaque color.
    ///
    /// The coordinates `[x_start, x_end]` are inclusive and specified in global screen space.
    fn fill_span(
        &mut self,
        y: i32,
        x_start: i32,
        x_end: i32,
        color: Self::Color,
    ) -> Result<(), Self::Error>;

    /// Blends a horizontal span with alpha factor ($0 = \text{transparent}, 255 = \text{opaque}$).
    ///
    /// The coordinates `[x_start, x_end]` are inclusive and specified in global screen space.
    fn blend_span(
        &mut self,
        y: i32,
        x_start: i32,
        x_end: i32,
        color: Self::Color,
        alpha: u8,
    ) -> Result<(), Self::Error>;

    /// Blends a single pixel with alpha factor.
    fn blend_pixel(
        &mut self,
        x: i32,
        y: i32,
        color: Self::Color,
        alpha: u8,
    ) -> Result<(), Self::Error>;
}
