//! Incremental homogeneous edge equation rasterizer and primitive for oriented projective triangles.

use embedded_graphics::primitives::PrimitiveStyle;
use embedded_graphics_core::draw_target::DrawTarget;
use embedded_graphics_core::geometry::{Point, Size};
use embedded_graphics_core::pixelcolor::PixelColor;
use embedded_graphics_core::primitives::Rectangle;
use embedded_graphics_core::Drawable;
use yonder_core::{join, orientation, Coord3, Orientation};

use super::style::{CompositingStyle, Styled};
use crate::target::CompositingTarget;

/// An oriented projective triangle in $\mathbb{T}^2$.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Triangle {
    pub p1: Coord3,
    pub p2: Coord3,
    pub p3: Coord3,
}

impl Triangle {
    /// Creates a new triangle from three homogeneous coordinates.
    #[inline]
    pub const fn new(p1: Coord3, p2: Coord3, p3: Coord3) -> Self {
        Self { p1, p2, p3 }
    }

    /// Attaches a visual style to this triangle.
    #[inline]
    pub fn into_styled<S>(self, style: S) -> Styled<Self, S> {
        Styled::new(self, style)
    }
}

impl<C: PixelColor> Styled<Triangle, CompositingStyle<C>> {
    /// Draws the styled triangle onto any compositing target.
    #[inline]
    pub fn draw<T: CompositingTarget<Color = C>>(&self, target: &mut T) -> Result<(), T::Error> {
        if let Some(color) = self.style.fill_color {
            let alpha = self.style.fill_alpha.unwrap_or(255);
            rasterize_triangle(
                target,
                self.primitive.p1,
                self.primitive.p2,
                self.primitive.p3,
                color,
                alpha,
            )?;
        }
        Ok(())
    }
}

impl<C: PixelColor> Drawable for Styled<Triangle, PrimitiveStyle<C>> {
    type Color = C;
    type Output = ();

    #[inline]
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &self,
        target: &mut D,
    ) -> Result<Self::Output, D::Error> {
        if let Some(color) = self.style.fill_color {
            rasterize_triangle_solid(
                target,
                self.primitive.p1,
                self.primitive.p2,
                self.primitive.p3,
                color,
            )?;
        }
        Ok(())
    }
}

/// Determines if an edge is a "top-left" edge for rasterization tie-breaking.
#[inline(always)]
fn is_top_left(a: i32, b: i32) -> bool {
    a > 0 || (a == 0 && b > 0)
}

/// Evaluates an oriented projective triangle and yields horizontal scanline spans for rasterization.
#[inline]
pub(crate) fn rasterize_triangle_spans<F, E>(
    p1: Coord3,
    mut p2: Coord3,
    mut p3: Coord3,
    mut emit_span: F,
) -> Result<(), E>
where
    F: FnMut(&Rectangle) -> Result<(), E>,
{
    let orient = orientation(p1, p2, p3);
    match orient {
        Orientation::Collinear => return Ok(()),
        Orientation::Clockwise => {
            core::mem::swap(&mut p2, &mut p3);
        }
        Orientation::CounterClockwise => {}
    }

    let (x1, y1) = match p1.to_euclidean() {
        Some(pt) => pt,
        None => return Ok(()),
    };
    let (x2, y2) = match p2.to_euclidean() {
        Some(pt) => pt,
        None => return Ok(()),
    };
    let (x3, y3) = match p3.to_euclidean() {
        Some(pt) => pt,
        None => return Ok(()),
    };

    let min_x = x1.min(x2).min(x3);
    let max_x = x1.max(x2).max(x3);
    let min_y = y1.min(y2).min(y3);
    let max_y = y1.max(y2).max(y3);

    let l1 = join(p1, p2);
    let l2 = join(p2, p3);
    let l3 = join(p3, p1);

    let (a1, b1) = (l1.a() as i64, l1.b() as i64);
    let (a2, b2) = (l2.a() as i64, l2.b() as i64);
    let (a3, b3) = (l3.a() as i64, l3.b() as i64);

    let bias1 = if is_top_left(l1.a(), l1.b()) { 0 } else { -1 };
    let bias2 = if is_top_left(l2.a(), l2.b()) { 0 } else { -1 };
    let bias3 = if is_top_left(l3.a(), l3.b()) { 0 } else { -1 };

    let p_start = Coord3::point(min_x, min_y);
    let mut row_e1 = l1.evaluate_coord(p_start) + bias1;
    let mut row_e2 = l2.evaluate_coord(p_start) + bias2;
    let mut row_e3 = l3.evaluate_coord(p_start) + bias3;

    for y in min_y..=max_y {
        let mut e1 = row_e1;
        let mut e2 = row_e2;
        let mut e3 = row_e3;

        let mut span_start: Option<i32> = None;
        let mut span_end: Option<i32> = None;

        for x in min_x..=max_x {
            if (e1 | e2 | e3) >= 0 {
                if span_start.is_none() {
                    span_start = Some(x);
                }
                span_end = Some(x);
            }

            e1 += a1;
            e2 += a2;
            e3 += a3;
        }

        if let (Some(x_start), Some(x_end)) = (span_start, span_end) {
            let width = (x_end - x_start + 1) as u32;
            let span_rect = Rectangle::new(Point::new(x_start, y), Size::new(width, 1));
            emit_span(&span_rect)?;
        }

        row_e1 += b1;
        row_e2 += b2;
        row_e3 += b3;
    }

    Ok(())
}

/// Rasterizes an oriented projective triangle onto any `DrawTarget` using incremental edge steppers.
pub fn rasterize_triangle_solid<C: Copy, D: DrawTarget<Color = C>>(
    target: &mut D,
    p1: Coord3,
    p2: Coord3,
    p3: Coord3,
    color: C,
) -> Result<(), D::Error> {
    rasterize_triangle_spans(p1, p2, p3, |rect| target.fill_solid(rect, color))
}

/// Rasterizes an oriented projective triangle onto `target` with alpha factor.
pub fn rasterize_triangle<C: Copy, T: CompositingTarget<Color = C>>(
    target: &mut T,
    p1: Coord3,
    p2: Coord3,
    p3: Coord3,
    color: C,
    alpha: u8,
) -> Result<(), T::Error> {
    if alpha == 0 {
        return Ok(());
    }
    rasterize_triangle_spans(p1, p2, p3, |rect| target.blend_solid(rect, color, alpha))
}

#[cfg(test)]
mod tests {
    use embedded_graphics_core::pixelcolor::{IntoStorage, Rgb565};
    use yonder_damage::Rect;

    use super::*;
    use crate::viewport::Viewport;

    #[test]
    fn test_rasterize_triangle_basic() {
        let mut mem = [0u16; 64 * 64];
        let bounds = Rect::new(0, 0, 64, 64);
        let mut viewport = Viewport::new(&mut mem, bounds);

        let p1 = Coord3::point(10, 10);
        let p2 = Coord3::point(30, 10);
        let p3 = Coord3::point(10, 30);

        let red = Rgb565::new(31, 0, 0);
        let tri = Triangle::new(p1, p2, p3).into_styled(CompositingStyle::with_fill(red));
        tri.draw(&mut viewport).unwrap();

        let idx_10_10 = viewport.global_to_local_index(10, 10).unwrap();
        assert_eq!(viewport.buffer()[idx_10_10], red.into_storage());

        let idx_0_0 = viewport.global_to_local_index(0, 0).unwrap();
        assert_eq!(viewport.buffer()[idx_0_0], 0);
    }

    #[test]
    fn test_rasterize_triangle_drawable_opaque() {
        let mut mem = [0u16; 64 * 64];
        let mut viewport = Viewport::new(&mut mem, Rect::new(0, 0, 64, 64));

        let p1 = Coord3::point(10, 10);
        let p2 = Coord3::point(30, 10);
        let p3 = Coord3::point(10, 30);

        let green = Rgb565::new(0, 63, 0);
        let tri = Triangle::new(p1, p2, p3).into_styled(PrimitiveStyle::with_fill(green));
        tri.draw(&mut viewport).unwrap();

        let idx_10_10 = viewport.global_to_local_index(10, 10).unwrap();
        assert_eq!(viewport.buffer()[idx_10_10], green.into_storage());
    }

    #[test]
    fn test_rasterize_triangle_collinear_and_clockwise() {
        let mut mem = [0u16; 32 * 32];
        let mut viewport = Viewport::new(&mut mem, Rect::new(0, 0, 32, 32));

        let p1 = Coord3::point(5, 5);
        let p2 = Coord3::point(10, 10);
        let p3 = Coord3::point(15, 15);
        rasterize_triangle(&mut viewport, p1, p2, p3, Rgb565::new(31, 0, 0), 255).unwrap();
        assert_eq!(viewport.buffer()[0], 0);

        let p_cw1 = Coord3::point(5, 5);
        let p_cw2 = Coord3::point(5, 15);
        let p_cw3 = Coord3::point(15, 5);
        rasterize_triangle(
            &mut viewport,
            p_cw1,
            p_cw2,
            p_cw3,
            Rgb565::new(0, 63, 0),
            128,
        )
        .unwrap();
    }
}
