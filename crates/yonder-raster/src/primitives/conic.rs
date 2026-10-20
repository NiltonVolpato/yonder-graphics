//! Conic section scanline rasterizer and primitive using 2nd-order forward differencing.

use embedded_graphics::primitives::PrimitiveStyle;
use embedded_graphics_core::draw_target::DrawTarget;
use embedded_graphics_core::geometry::{Point, Size};
use embedded_graphics_core::pixelcolor::PixelColor;
use embedded_graphics_core::primitives::Rectangle;
use embedded_graphics_core::Drawable;
use yonder_core::ConicMatrix;
use yonder_damage::Rect;

use super::style::{CompositingStyle, Styled};
use crate::target::CompositingTarget;

/// An algebraic conic section (circle, ellipse) bound to a rectangular evaluation area.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Conic {
    pub matrix: ConicMatrix,
    pub bounds: Rect,
}

impl Conic {
    /// Creates a new conic primitive with bounding evaluation rectangle.
    #[inline]
    pub const fn new(matrix: ConicMatrix, bounds: Rect) -> Self {
        Self { matrix, bounds }
    }

    /// Creates a circle conic primitive.
    ///
    /// # Panics
    /// Panics in debug builds if `radius < 0`.
    #[inline]
    pub fn circle(cx: i32, cy: i32, radius: i32) -> Self {
        debug_assert!(radius >= 0, "radius must be non-negative");
        let matrix = ConicMatrix::circle(cx, cy, radius);
        let bounds = Rect::new(
            cx - radius,
            cy - radius,
            (2 * radius + 1) as u32,
            (2 * radius + 1) as u32,
        );
        Self { matrix, bounds }
    }

    /// Creates an axis-aligned ellipse conic primitive.
    ///
    /// # Panics
    /// Panics in debug builds if `rx < 0` or `ry < 0`.
    #[inline]
    pub fn ellipse(cx: i32, cy: i32, rx: i32, ry: i32) -> Self {
        debug_assert!(rx >= 0, "rx must be non-negative");
        debug_assert!(ry >= 0, "ry must be non-negative");
        let matrix = ConicMatrix::ellipse(cx, cy, rx, ry);
        let bounds = Rect::new(cx - rx, cy - ry, (2 * rx + 1) as u32, (2 * ry + 1) as u32);
        Self { matrix, bounds }
    }

    /// Attaches a visual style to this conic section.
    #[inline]
    pub fn into_styled<S>(self, style: S) -> Styled<Self, S> {
        Styled::new(self, style)
    }
}

impl<C: PixelColor> Styled<Conic, CompositingStyle<C>> {
    /// Draws the styled conic section onto any compositing target.
    #[inline]
    pub fn draw<T: CompositingTarget<Color = C>>(&self, target: &mut T) -> Result<(), T::Error> {
        if let Some(color) = self.style.fill_color {
            let alpha = self.style.fill_alpha.unwrap_or(255);
            rasterize_conic(
                target,
                &self.primitive.matrix,
                self.primitive.bounds,
                color,
                alpha,
            )?;
        }
        Ok(())
    }
}

impl<C: PixelColor> Drawable for Styled<Conic, PrimitiveStyle<C>> {
    type Color = C;
    type Output = ();

    #[inline]
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &self,
        target: &mut D,
    ) -> Result<Self::Output, D::Error> {
        if let Some(color) = self.style.fill_color {
            rasterize_conic_solid(target, &self.primitive.matrix, self.primitive.bounds, color)?;
        }
        Ok(())
    }
}

/// Evaluates a conic section and yields horizontal scanline spans for rasterization.
#[inline]
pub(crate) fn rasterize_conic_spans<F, E>(
    conic: &ConicMatrix,
    bounds: Rect,
    mut emit_span: F,
) -> Result<(), E>
where
    F: FnMut(&Rectangle) -> Result<(), E>,
{
    if bounds.is_empty() {
        return Ok(());
    }

    let end_y = bounds.bottom();
    let end_x = bounds.right();

    for y in bounds.y..end_y {
        let mut stepper = conic.scanline_stepper(bounds.x, y);

        let mut span_start: Option<i32> = None;
        let mut span_end: Option<i32> = None;

        for x in bounds.x..end_x {
            if stepper.step_inside() {
                if span_start.is_none() {
                    span_start = Some(x);
                }
                span_end = Some(x);
            }
        }

        if let (Some(x_start), Some(x_end)) = (span_start, span_end) {
            let width = (x_end - x_start + 1) as u32;
            let span_rect = Rectangle::new(Point::new(x_start, y), Size::new(width, 1));
            emit_span(&span_rect)?;
        }
    }

    Ok(())
}

/// Rasterizes a conic section onto any `DrawTarget` using 2nd-order incremental forward differencing.
pub fn rasterize_conic_solid<C: Copy, D: DrawTarget<Color = C>>(
    target: &mut D,
    conic: &ConicMatrix,
    bounds: Rect,
    color: C,
) -> Result<(), D::Error> {
    rasterize_conic_spans(conic, bounds, |rect| target.fill_solid(rect, color))
}

/// Rasterizes a conic section onto `target` with alpha factor using 2nd-order forward differencing.
pub fn rasterize_conic<C: Copy, T: CompositingTarget<Color = C>>(
    target: &mut T,
    conic: &ConicMatrix,
    bounds: Rect,
    color: C,
    alpha: u8,
) -> Result<(), T::Error> {
    if alpha == 0 {
        return Ok(());
    }
    rasterize_conic_spans(conic, bounds, |rect| target.blend_solid(rect, color, alpha))
}

#[cfg(test)]
mod tests {
    use embedded_graphics_core::pixelcolor::{IntoStorage, Rgb565};

    use super::*;
    use crate::viewport::Viewport;

    #[test]
    fn test_rasterize_circle_and_ellipse() {
        let mut mem = [0u16; 64 * 64];
        let bounds = Rect::new(0, 0, 64, 64);
        let mut viewport = Viewport::new(&mut mem, bounds);

        let circle = ConicMatrix::circle(32, 32, 10);
        let circle_bounds = Rect::new(22, 22, 21, 21);
        let red = Rgb565::new(31, 0, 0);

        let styled_circle =
            Conic::new(circle, circle_bounds).into_styled(CompositingStyle::with_fill(red));
        styled_circle.draw(&mut viewport).unwrap();

        let idx_center = viewport.global_to_local_index(32, 32).unwrap();
        assert_eq!(viewport.buffer()[idx_center], red.into_storage());

        assert_eq!(viewport.buffer()[0], 0);

        // Alpha 0 should be no-op
        rasterize_conic(&mut viewport, &circle, circle_bounds, red, 0).unwrap();

        // Empty bounds test
        rasterize_conic(&mut viewport, &circle, Rect::ZERO, red, 255).unwrap();

        // Test Conic::circle helper and solid primitive drawable
        let green = Rgb565::new(0, 63, 0);
        let c2 = Conic::circle(16, 16, 5).into_styled(PrimitiveStyle::with_fill(green));
        c2.draw(&mut viewport).unwrap();
        let idx_16 = viewport.global_to_local_index(16, 16).unwrap();
        assert_eq!(viewport.buffer()[idx_16], green.into_storage());
    }
}
