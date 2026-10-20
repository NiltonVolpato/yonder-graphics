//! Compositing target trait extending `embedded-graphics` DrawTarget.

pub(crate) mod damage_grid;

use embedded_graphics_core::draw_target::DrawTarget;
use embedded_graphics_core::geometry::Point;
use embedded_graphics_core::primitives::Rectangle;

/// A drawing target that supports alpha compositing and blending operations.
///
/// Mirrors `embedded_graphics_core::draw_target::DrawTarget` with alpha-aware variants.
pub trait CompositingTarget: DrawTarget {
    /// Blends a solid rectangular area with a uniform color and alpha factor ($0 = \text{transparent}, 255 = \text{opaque}$).
    fn blend_solid(
        &mut self,
        area: &Rectangle,
        color: Self::Color,
        alpha: u8,
    ) -> Result<(), Self::Error>;

    /// Blends an iterator of individual pixels with their respective alpha factors.
    fn blend_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = (Point, Self::Color, u8)>;

    /// Blends contiguous pixel colors and alphas over a rectangular area in row-major order.
    fn blend_contiguous<I>(&mut self, area: &Rectangle, items: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = (Self::Color, u8)>;
}
