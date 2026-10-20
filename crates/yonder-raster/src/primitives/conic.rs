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

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum ConicGeometry {
    Circle { cx: i32, cy: i32, radius: i32 },
    Ellipse { cx: i32, cy: i32, rx: i32, ry: i32 },
    General,
}

/// An algebraic conic section (circle, ellipse) bound to a rectangular evaluation area.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Conic {
    pub matrix: ConicMatrix,
    pub bounds: Rect,
    pub(crate) geometry: ConicGeometry,
}

impl Conic {
    /// Creates a new conic primitive with bounding evaluation rectangle.
    #[inline]
    pub const fn new(matrix: ConicMatrix, bounds: Rect) -> Self {
        Self {
            matrix,
            bounds,
            geometry: ConicGeometry::General,
        }
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
        Self {
            matrix,
            bounds,
            geometry: ConicGeometry::Circle { cx, cy, radius },
        }
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
        Self {
            matrix,
            bounds,
            geometry: ConicGeometry::Ellipse { cx, cy, rx, ry },
        }
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
            if self.style.antialiased {
                let bounds = self.primitive.bounds.dilate(1);
                rasterize_conic_aa(target, &self.primitive, bounds, color, alpha)?;
            } else {
                rasterize_conic(
                    target,
                    &self.primitive.matrix,
                    self.primitive.bounds,
                    color,
                    alpha,
                )?;
            }
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

#[inline]
const fn integer_sqrt_u64(n: u64) -> u64 {
    if n == 0 {
        return 0;
    }
    let mut x0 = 1u64 << (64 - n.leading_zeros()).div_ceil(2);
    loop {
        let x1 = (x0 + n / x0) / 2;
        if x1 >= x0 {
            return x0;
        }
        x0 = x1;
    }
}

/// Rasterizes a conic section onto `target` with subpixel antialiasing and alpha blending.
pub fn rasterize_conic_aa<C: Copy, T: CompositingTarget<Color = C>>(
    target: &mut T,
    conic: &Conic,
    bounds: Rect,
    color: C,
    alpha: u8,
) -> Result<(), T::Error> {
    if alpha == 0 || bounds.is_empty() {
        return Ok(());
    }

    match conic.geometry {
        ConicGeometry::Circle { cx, cy, radius } => {
            rasterize_circle_aa(target, cx, cy, radius, bounds, color, alpha)
        }
        ConicGeometry::Ellipse { cx, cy, rx, ry } => {
            rasterize_ellipse_aa(target, Point::new(cx, cy), (rx, ry), bounds, color, alpha)
        }
        ConicGeometry::General => rasterize_conic(target, &conic.matrix, bounds, color, alpha),
    }
}

fn rasterize_circle_aa<C: Copy, T: CompositingTarget<Color = C>>(
    target: &mut T,
    cx: i32,
    cy: i32,
    radius: i32,
    bounds: Rect,
    color: C,
    alpha: u8,
) -> Result<(), T::Error> {
    if radius <= 0 {
        return Ok(());
    }

    let two_r = (2 * radius) as i64;
    let r_val = radius as i64;
    let end_y = bounds.bottom();

    for y in bounds.y..end_y {
        let dy = (y - cy) as i64;
        if dy.abs() > r_val + 1 {
            continue;
        }

        let mut solid_start: Option<i32> = None;
        let mut solid_end: Option<i32> = None;

        let mut fringe_count = 0;
        let mut fringe_pixels: [(Point, u8); 8] = [(Point::zero(), 0); 8];

        for x in bounds.x..bounds.right() {
            let dx = (x - cx) as i64;
            let q = dx * dx + dy * dy - r_val * r_val;

            if q <= -r_val {
                if solid_start.is_none() {
                    solid_start = Some(x);
                }
                solid_end = Some(x);
            } else if q < r_val {
                let num = r_val - q;
                let cov = ((num * 255 + r_val) / two_r) as u8;
                let pixel_alpha = ((alpha as u32 * cov as u32 + 128) / 255) as u8;
                if pixel_alpha > 0 && fringe_count < fringe_pixels.len() {
                    fringe_pixels[fringe_count] = (Point::new(x, y), pixel_alpha);
                    fringe_count += 1;
                }
            } else if solid_end.is_some() {
                break;
            }
        }

        // Emit fringe pixels
        for &(pt, a) in &fringe_pixels[..fringe_count] {
            let rect = Rectangle::new(pt, Size::new(1, 1));
            target.blend_solid(&rect, color, a)?;
        }

        // Emit solid interior span
        if let (Some(x_start), Some(x_end)) = (solid_start, solid_end) {
            let width = (x_end - x_start + 1) as u32;
            let rect = Rectangle::new(Point::new(x_start, y), Size::new(width, 1));
            target.blend_solid(&rect, color, alpha)?;
        }
    }

    Ok(())
}

fn rasterize_ellipse_aa<C: Copy, T: CompositingTarget<Color = C>>(
    target: &mut T,
    center: Point,
    radii: (i32, i32),
    bounds: Rect,
    color: C,
    alpha: u8,
) -> Result<(), T::Error> {
    let (cx, cy) = (center.x, center.y);
    let (rx, ry) = radii;
    if rx <= 0 || ry <= 0 {
        return Ok(());
    }

    let rx_val = rx as i64;
    let ry_val = ry as i64;
    let rx2 = rx_val * rx_val;
    let ry2 = ry_val * ry_val;
    let rx2_ry2 = rx2 * ry2;

    let end_y = bounds.bottom();

    for y in bounds.y..end_y {
        let dy = (y - cy) as i64;
        if dy.abs() > ry_val + 1 {
            continue;
        }

        let mut solid_start: Option<i32> = None;
        let mut solid_end: Option<i32> = None;

        let mut fringe_count = 0;
        let mut fringe_pixels: [(Point, u8); 8] = [(Point::zero(), 0); 8];

        for x in bounds.x..bounds.right() {
            let dx = (x - cx) as i64;
            let q = ry2 * dx * dx + rx2 * dy * dy - rx2_ry2;

            let gx = 2 * ry2 * dx;
            let gy = 2 * rx2 * dy;
            let g_sq = (gx * gx + gy * gy) as u64;

            if g_sq == 0 {
                if solid_start.is_none() {
                    solid_start = Some(x);
                }
                solid_end = Some(x);
                continue;
            }

            let g_len = integer_sqrt_u64(g_sq) as i64;
            let half_g = g_len / 2;

            if q <= -half_g {
                if solid_start.is_none() {
                    solid_start = Some(x);
                }
                solid_end = Some(x);
            } else if q < half_g {
                let num = half_g - q;
                let cov = if g_len > 0 {
                    ((num * 255) / g_len).clamp(0, 255) as u8
                } else {
                    255
                };
                let pixel_alpha = ((alpha as u32 * cov as u32 + 128) / 255) as u8;
                if pixel_alpha > 0 && fringe_count < fringe_pixels.len() {
                    fringe_pixels[fringe_count] = (Point::new(x, y), pixel_alpha);
                    fringe_count += 1;
                }
            } else if solid_end.is_some() {
                break;
            }
        }

        // Emit fringe pixels
        for &(pt, a) in &fringe_pixels[..fringe_count] {
            let rect = Rectangle::new(pt, Size::new(1, 1));
            target.blend_solid(&rect, color, a)?;
        }

        // Emit solid interior span
        if let (Some(x_start), Some(x_end)) = (solid_start, solid_end) {
            let width = (x_end - x_start + 1) as u32;
            let rect = Rectangle::new(Point::new(x_start, y), Size::new(width, 1));
            target.blend_solid(&rect, color, alpha)?;
        }
    }

    Ok(())
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

    #[test]
    fn test_rasterize_conic_aa_circle() {
        let mut mem = [0u16; 64 * 64];
        let bounds = Rect::new(0, 0, 64, 64);
        let mut viewport = Viewport::new(&mut mem, bounds);

        let red = Rgb565::new(31, 0, 0);
        let circle = Conic::circle(32, 32, 10)
            .into_styled(CompositingStyle::with_fill(red).with_antialiasing(true));

        circle.draw(&mut viewport).unwrap();

        // Center must be 100% solid red
        let idx_center = viewport.global_to_local_index(32, 32).unwrap();
        assert_eq!(viewport.buffer()[idx_center], red.into_storage());

        // Far exterior must be 0
        let idx_far = viewport.global_to_local_index(0, 0).unwrap();
        assert_eq!(viewport.buffer()[idx_far], 0);

        // Transition pixel just outside nominal boundary (radius = 10, so x = 43 or 42 is boundary fringe)
        // At x = 42, y = 32 (dx = 10): on boundary, should have non-zero blended color
        let idx_fringe = viewport.global_to_local_index(42, 32).unwrap();
        assert_ne!(viewport.buffer()[idx_fringe], 0);
    }

    #[test]
    fn test_rasterize_conic_aa_ellipse() {
        let mut mem = [0u16; 64 * 64];
        let bounds = Rect::new(0, 0, 64, 64);
        let mut viewport = Viewport::new(&mut mem, bounds);

        let blue = Rgb565::new(0, 0, 31);
        let ellipse = Conic::ellipse(32, 32, 15, 8)
            .into_styled(CompositingStyle::with_fill(blue).with_antialiasing(true));

        ellipse.draw(&mut viewport).unwrap();

        // Center must be solid blue
        let idx_center = viewport.global_to_local_index(32, 32).unwrap();
        assert_eq!(viewport.buffer()[idx_center], blue.into_storage());

        // Far exterior must be 0
        assert_eq!(viewport.buffer()[0], 0);

        // Boundary fringe should have non-zero value
        let idx_fringe = viewport.global_to_local_index(47, 32).unwrap();
        assert_ne!(viewport.buffer()[idx_fringe], 0);
    }
}
