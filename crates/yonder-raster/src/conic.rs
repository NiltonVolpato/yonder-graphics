//! Conic section scanline rasterizer using 2nd-order forward differencing.

use embedded_graphics_core::pixelcolor::Rgb565;
use yonder_core::ConicMatrix;
use yonder_damage::Rect;

use crate::target::CompositingTarget;

/// Rasterizes a conic section (circle, ellipse) within `bounds` onto `target`.
///
/// Uses the 2nd-order incremental forward differencing stepper:
/// $$Q(x + 1, y) = Q(x, y) + \Delta Q_x$$
/// $$\Delta Q_x(x + 1) = \Delta Q_x(x) + 2A$$
/// Requiring only **two integer additions per pixel** in the inner loop with zero square roots.
pub fn rasterize_conic<T: CompositingTarget<Color = Rgb565>>(
    target: &mut T,
    conic: &ConicMatrix,
    bounds: Rect,
    color: Rgb565,
    alpha: Option<u8>,
) -> Result<(), T::Error> {
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
            match alpha {
                Some(a) if a < 255 => {
                    target.blend_span(y, x_start, x_end, color, a)?;
                }
                _ => {
                    target.fill_span(y, x_start, x_end, color)?;
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scratchpad::ViewportScratchpad;
    use embedded_graphics_core::pixelcolor::IntoStorage;

    #[test]
    fn test_rasterize_circle_and_ellipse() {
        let mut mem = [0u16; 64 * 64];
        let bounds = Rect::new(0, 0, 64, 64);
        let mut scratch = ViewportScratchpad::new(&mut mem, bounds);

        // Circle with center (32, 32), radius 10
        let circle = ConicMatrix::circle(32, 32, 10);
        let circle_bounds = Rect::new(22, 22, 21, 21);
        let red = Rgb565::new(31, 0, 0);

        rasterize_conic(&mut scratch, &circle, circle_bounds, red, None).unwrap();

        // Center (32, 32) must be filled red
        let idx_center = scratch.global_to_local_index(32, 32).unwrap();
        assert_eq!(scratch.buffer()[idx_center], red.into_storage());

        // External pixel (0, 0) must be 0
        assert_eq!(scratch.buffer()[0], 0);

        // Empty bounds test
        rasterize_conic(&mut scratch, &circle, Rect::ZERO, red, None).unwrap();
    }
}
