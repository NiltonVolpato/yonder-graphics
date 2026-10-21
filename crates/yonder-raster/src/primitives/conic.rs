//! Conic section scanline rasterizer and primitive using 2nd-order forward differencing.

use embedded_graphics::primitives::PrimitiveStyle;
use embedded_graphics_core::draw_target::DrawTarget;
use embedded_graphics_core::geometry::{Point, Size};
use embedded_graphics_core::pixelcolor::PixelColor;
use embedded_graphics_core::primitives::Rectangle;
use embedded_graphics_core::Drawable;
use yonder_core::{ConicMatrix, Coord3};
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
            if self.style.antialiased {
                let bounds = self.primitive.bounds.dilate(1);
                rasterize_conic_aa(target, &self.primitive.matrix, bounds, color, alpha)?;
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
fn edge_zero_crossing(v_in: i64, v_out: i64) -> i64 {
    let num = -v_in * 255;
    let denom = v_out - v_in;
    if denom == 0 {
        return 0;
    }
    (num / denom).clamp(0, 255)
}

#[inline]
fn outside_corner_zero_crossing(v_out: i64, v_in: i64) -> i64 {
    let num = v_out * 255;
    let denom = v_out - v_in;
    if denom == 0 {
        return 0;
    }
    (num / denom).clamp(0, 255)
}

/// Evaluates the 4 corners of pixel $[x, x+1] \times [y, y+1]$ against `conic`
/// and computes subpixel polygon coverage in $0..255$.
#[inline]
pub(crate) fn compute_corner_coverage(conic: &ConicMatrix, x: i32, y: i32) -> u8 {
    let v_tl = conic.evaluate_pixel(x, y);
    let v_tr = conic.evaluate_pixel(x + 1, y);
    let v_bl = conic.evaluate_pixel(x, y + 1);
    let v_br = conic.evaluate_pixel(x + 1, y + 1);

    let in_tl = v_tl <= 0;
    let in_tr = v_tr <= 0;
    let in_bl = v_bl <= 0;
    let in_br = v_br <= 0;

    let mask = (in_tl as u8) | ((in_tr as u8) << 1) | ((in_bl as u8) << 2) | ((in_br as u8) << 3);

    match mask {
        // All corners outside
        0b0000 => {
            // Check center point for subpixel-scale features
            let center = Coord3::new(2 * x + 1, 2 * y + 1, 2);
            if conic.evaluate_coord(center) <= 0 {
                64
            } else {
                0
            }
        }
        // All corners inside
        0b1111 => 255,

        // Single corner inside (Triangle cut)
        // TL inside
        0b0001 => {
            let t_top = edge_zero_crossing(v_tl, v_tr);
            let t_left = edge_zero_crossing(v_tl, v_bl);
            ((t_top * t_left) / (2 * 255)).clamp(0, 255) as u8
        }
        // TR inside
        0b0010 => {
            let t_top = edge_zero_crossing(v_tr, v_tl);
            let t_right = edge_zero_crossing(v_tr, v_br);
            ((t_top * t_right) / (2 * 255)).clamp(0, 255) as u8
        }
        // BL inside
        0b0100 => {
            let t_left = edge_zero_crossing(v_bl, v_tl);
            let t_bottom = edge_zero_crossing(v_bl, v_br);
            ((t_left * t_bottom) / (2 * 255)).clamp(0, 255) as u8
        }
        // BR inside
        0b1000 => {
            let t_bottom = edge_zero_crossing(v_br, v_bl);
            let t_right = edge_zero_crossing(v_br, v_tr);
            ((t_bottom * t_right) / (2 * 255)).clamp(0, 255) as u8
        }

        // Two adjacent corners inside (Trapezoid cut)
        // TL and TR inside (horizontal trapezoid across top)
        0b0011 => {
            let t_left = edge_zero_crossing(v_tl, v_bl);
            let t_right = edge_zero_crossing(v_tr, v_br);
            ((t_left + t_right) / 2).clamp(0, 255) as u8
        }
        // BL and BR inside (horizontal trapezoid across bottom)
        0b1100 => {
            let t_left = edge_zero_crossing(v_bl, v_tl);
            let t_right = edge_zero_crossing(v_br, v_tr);
            ((t_left + t_right) / 2).clamp(0, 255) as u8
        }
        // TL and BL inside (vertical trapezoid across left)
        0b0101 => {
            let t_top = edge_zero_crossing(v_tl, v_tr);
            let t_bottom = edge_zero_crossing(v_bl, v_br);
            ((t_top + t_bottom) / 2).clamp(0, 255) as u8
        }
        // TR and BR inside (vertical trapezoid across right)
        0b1010 => {
            let t_top = edge_zero_crossing(v_tr, v_tl);
            let t_bottom = edge_zero_crossing(v_br, v_bl);
            ((t_top + t_bottom) / 2).clamp(0, 255) as u8
        }

        // Three corners inside (One corner outside: 255 - outer triangle)
        // BR outside
        0b0111 => {
            let t_bottom = outside_corner_zero_crossing(v_br, v_bl);
            let t_right = outside_corner_zero_crossing(v_br, v_tr);
            let out_area = (t_bottom * t_right) / (2 * 255);
            (255 - out_area).clamp(0, 255) as u8
        }
        // BL outside
        0b1011 => {
            let t_bottom = outside_corner_zero_crossing(v_bl, v_br);
            let t_left = outside_corner_zero_crossing(v_bl, v_tl);
            let out_area = (t_bottom * t_left) / (2 * 255);
            (255 - out_area).clamp(0, 255) as u8
        }
        // TR outside
        0b1101 => {
            let t_top = outside_corner_zero_crossing(v_tr, v_tl);
            let t_right = outside_corner_zero_crossing(v_tr, v_br);
            let out_area = (t_top * t_right) / (2 * 255);
            (255 - out_area).clamp(0, 255) as u8
        }
        // TL outside
        0b1110 => {
            let t_top = outside_corner_zero_crossing(v_tl, v_tr);
            let t_left = outside_corner_zero_crossing(v_tl, v_bl);
            let out_area = (t_top * t_left) / (2 * 255);
            (255 - out_area).clamp(0, 255) as u8
        }

        // Two opposite corners inside (saddle / diagonal crossing)
        // TL and BR inside
        0b1001 => {
            let center = Coord3::new(2 * x + 1, 2 * y + 1, 2);
            if conic.evaluate_coord(center) <= 0 {
                let a_tr = (outside_corner_zero_crossing(v_tr, v_tl)
                    * outside_corner_zero_crossing(v_tr, v_br))
                    / (2 * 255);
                let a_bl = (outside_corner_zero_crossing(v_bl, v_tl)
                    * outside_corner_zero_crossing(v_bl, v_br))
                    / (2 * 255);
                (255 - a_tr - a_bl).clamp(0, 255) as u8
            } else {
                let a_tl =
                    (edge_zero_crossing(v_tl, v_tr) * edge_zero_crossing(v_tl, v_bl)) / (2 * 255);
                let a_br =
                    (edge_zero_crossing(v_br, v_bl) * edge_zero_crossing(v_br, v_tr)) / (2 * 255);
                (a_tl + a_br).clamp(0, 255) as u8
            }
        }
        // TR and BL inside
        0b0110 => {
            let center = Coord3::new(2 * x + 1, 2 * y + 1, 2);
            if conic.evaluate_coord(center) <= 0 {
                let a_tl = (outside_corner_zero_crossing(v_tl, v_tr)
                    * outside_corner_zero_crossing(v_tl, v_bl))
                    / (2 * 255);
                let a_br = (outside_corner_zero_crossing(v_br, v_bl)
                    * outside_corner_zero_crossing(v_br, v_tr))
                    / (2 * 255);
                (255 - a_tl - a_br).clamp(0, 255) as u8
            } else {
                let a_tr =
                    (edge_zero_crossing(v_tr, v_tl) * edge_zero_crossing(v_tr, v_br)) / (2 * 255);
                let a_bl =
                    (edge_zero_crossing(v_bl, v_tl) * edge_zero_crossing(v_bl, v_br)) / (2 * 255);
                (a_tr + a_bl).clamp(0, 255) as u8
            }
        }
        _ => unreachable!(),
    }
}

/// Rasterizes a conic section onto `target` with subpixel antialiasing and alpha blending.
pub fn rasterize_conic_aa<C: Copy, T: CompositingTarget<Color = C>>(
    target: &mut T,
    conic: &ConicMatrix,
    bounds: Rect,
    color: C,
    alpha: u8,
) -> Result<(), T::Error> {
    if alpha == 0 || bounds.is_empty() {
        return Ok(());
    }

    let end_y = bounds.bottom();
    let end_x = bounds.right();

    for y in bounds.y..end_y {
        let mut solid_start: Option<i32> = None;
        let mut solid_end: Option<i32> = None;
        let mut has_drawn_on_row = false;

        for x in bounds.x..end_x {
            let cov = compute_corner_coverage(conic, x, y);

            if cov == 0 {
                if let (Some(x_start), Some(x_end)) = (solid_start, solid_end) {
                    let width = (x_end - x_start + 1) as u32;
                    let rect = Rectangle::new(Point::new(x_start, y), Size::new(width, 1));
                    target.blend_solid(&rect, color, alpha)?;
                    solid_start = None;
                    solid_end = None;
                }
                if has_drawn_on_row {
                    break;
                }
            } else if cov == 255 {
                has_drawn_on_row = true;
                if solid_start.is_none() {
                    solid_start = Some(x);
                }
                solid_end = Some(x);
            } else {
                has_drawn_on_row = true;
                if let (Some(x_start), Some(x_end)) = (solid_start, solid_end) {
                    let width = (x_end - x_start + 1) as u32;
                    let rect = Rectangle::new(Point::new(x_start, y), Size::new(width, 1));
                    target.blend_solid(&rect, color, alpha)?;
                    solid_start = None;
                    solid_end = None;
                }
                let pixel_alpha = ((alpha as u32 * cov as u32 + 128) / 255) as u8;
                if pixel_alpha > 0 {
                    let rect = Rectangle::new(Point::new(x, y), Size::new(1, 1));
                    target.blend_solid(&rect, color, pixel_alpha)?;
                }
            }
        }

        // Flush remaining solid span for this row
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

        // Transition pixel on diagonal boundary: at (37, 23), dx = 5, dy = -9, cuts across pixel
        let idx_fringe = viewport.global_to_local_index(37, 23).unwrap();
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

        // Boundary fringe pixel at x = 46, y = 32 has BR corner outside
        let idx_fringe = viewport.global_to_local_index(46, 32).unwrap();
        assert_ne!(viewport.buffer()[idx_fringe], 0);
    }

    #[test]
    fn test_conic_aa_pixel_corners_quadrant_symmetry() {
        let circle = ConicMatrix::circle(0, 0, 10);

        // Pixel [7..8] x [7..8] vs its exact mirrors across axes
        let cov_q1 = compute_corner_coverage(&circle, 7, 7);
        let cov_q2 = compute_corner_coverage(&circle, -8, 7);
        let cov_q3 = compute_corner_coverage(&circle, -8, -8);
        let cov_q4 = compute_corner_coverage(&circle, 7, -8);

        assert_eq!(cov_q1, cov_q2);
        assert_eq!(cov_q1, cov_q3);
        assert_eq!(cov_q1, cov_q4);
        assert!(cov_q1 > 0 && cov_q1 < 255);
    }

    #[test]
    fn test_conic_aa_straight_cuts() {
        // A large circle with radius 1000 centered at (1000, 0)
        // At x = 0, the boundary is effectively a vertical line at x = 0.
        // Pixel [0..1] x [0..1] has left edge at x = 0 (on boundary), right edge at x = 1 (outside).
        // Center is at (0.5, 0.5), which is outside the circle since center of circle is at x = 1000.
        // Let's create a circle centered at (0, 0) with radius 10.
        // At y = 0, edge crosses near x = 10.
        let circle = ConicMatrix::circle(0, 0, 10);
        let cov_inside = compute_corner_coverage(&circle, 0, 0);
        assert_eq!(cov_inside, 255);

        let cov_outside = compute_corner_coverage(&circle, 20, 20);
        assert_eq!(cov_outside, 0);
    }

    #[test]
    fn test_conic_aa_rotated_ellipse() {
        let mut mem = [0u16; 64 * 64];
        let bounds = Rect::new(0, 0, 64, 64);
        let mut viewport = Viewport::new(&mut mem, bounds);

        // Rotated ellipse with cross term b = 1
        let rotated = ConicMatrix::new(3, 1, 2, -32 * 3, -32 * 2, 2000);
        let color = Rgb565::new(31, 63, 31);
        rasterize_conic_aa(&mut viewport, &rotated, bounds, color, 255).unwrap();

        // Alpha 0 and empty bounds should be no-ops
        rasterize_conic_aa(&mut viewport, &rotated, bounds, color, 0).unwrap();
        rasterize_conic_aa(&mut viewport, &rotated, Rect::ZERO, color, 255).unwrap();
    }
}
