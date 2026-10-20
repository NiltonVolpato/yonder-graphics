//! Incremental homogeneous edge equation rasterizer for oriented projective triangles.

use embedded_graphics_core::pixelcolor::Rgb565;
use yonder_core::{join, orientation, Coord3, Orientation};

use crate::target::CompositingTarget;

/// Determines if an edge is a "top-left" edge for rasterization tie-breaking.
///
/// In screen space (+y down, CCW winding):
/// A top edge is horizontal and directed to the right ($a = 0 \land b > 0$).
/// A left edge is directed upwards ($a > 0$).
#[inline(always)]
fn is_top_left(a: i32, b: i32) -> bool {
    a > 0 || (a == 0 && b > 0)
}

/// Rasterizes an oriented projective triangle onto `target` using incremental edge steppers.
///
/// Steps horizontally using single integer additions:
/// $$E(x + 1, y) = E(x, y) + a$$
/// Stepping down a scanline:
/// $$E(x, y + 1) = E(x, y) + b$$
///
/// If `alpha` is `Some(a)` with $a < 255$, uses `blend_span`, otherwise `fill_span`.
pub fn rasterize_triangle<T: CompositingTarget<Color = Rgb565>>(
    target: &mut T,
    p1: Coord3,
    mut p2: Coord3,
    mut p3: Coord3,
    color: Rgb565,
    alpha: Option<u8>,
) -> Result<(), T::Error> {
    let orient = orientation(p1, p2, p3);
    match orient {
        Orientation::Collinear => return Ok(()), // Degenerate triangle
        Orientation::Clockwise => {
            // Swap p2 and p3 to ensure counter-clockwise winding
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

    let a1 = l1.a() as i64;
    let b1 = l1.b() as i64;
    let c1 = l1.c() as i64;

    let a2 = l2.a() as i64;
    let b2 = l2.b() as i64;
    let c2 = l2.c() as i64;

    let a3 = l3.a() as i64;
    let b3 = l3.b() as i64;
    let c3 = l3.c() as i64;

    // Tie-breaking offsets: if top-left edge, 0; else -1
    let bias1 = if is_top_left(l1.a(), l1.b()) { 0 } else { -1 };
    let bias2 = if is_top_left(l2.a(), l2.b()) { 0 } else { -1 };
    let bias3 = if is_top_left(l3.a(), l3.b()) { 0 } else { -1 };

    // Initial edge values at (min_x, min_y)
    let mut row_e1 = a1 * min_x as i64 + b1 * min_y as i64 + c1 + bias1;
    let mut row_e2 = a2 * min_x as i64 + b2 * min_y as i64 + c2 + bias2;
    let mut row_e3 = a3 * min_x as i64 + b3 * min_y as i64 + c3 + bias3;

    for y in min_y..=max_y {
        let mut e1 = row_e1;
        let mut e2 = row_e2;
        let mut e3 = row_e3;

        let mut span_start: Option<i32> = None;
        let mut span_end: Option<i32> = None;

        for x in min_x..=max_x {
            if e1 >= 0 && e2 >= 0 && e3 >= 0 {
                if span_start.is_none() {
                    span_start = Some(x);
                }
                span_end = Some(x);
            }

            // Incremental step in X
            e1 += a1;
            e2 += a2;
            e3 += a3;
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

        // Incremental step in Y
        row_e1 += b1;
        row_e2 += b2;
        row_e3 += b3;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scratchpad::ViewportScratchpad;
    use embedded_graphics_core::pixelcolor::IntoStorage;
    use yonder_damage::Rect;

    #[test]
    fn test_rasterize_triangle_basic() {
        let mut mem = [0u16; 64 * 64];
        let bounds = Rect::new(0, 0, 64, 64);
        let mut scratch = ViewportScratchpad::new(&mut mem, bounds);

        // Right triangle at (10, 10), (30, 10), (10, 30)
        let p1 = Coord3::point(10, 10);
        let p2 = Coord3::point(30, 10);
        let p3 = Coord3::point(10, 30);

        let red = Rgb565::new(31, 0, 0);
        rasterize_triangle(&mut scratch, p1, p2, p3, red, None).unwrap();

        // (10, 10) should be filled
        let idx_10_10 = scratch.global_to_local_index(10, 10).unwrap();
        assert_eq!(scratch.buffer()[idx_10_10], red.into_storage());

        // (0, 0) outside should be 0
        let idx_0_0 = scratch.global_to_local_index(0, 0).unwrap();
        assert_eq!(scratch.buffer()[idx_0_0], 0);
    }

    #[test]
    fn test_rasterize_triangle_collinear_and_clockwise() {
        let mut mem = [0u16; 32 * 32];
        let mut scratch = ViewportScratchpad::new(&mut mem, Rect::new(0, 0, 32, 32));

        // Collinear triangle -> should not error or paint
        let p1 = Coord3::point(5, 5);
        let p2 = Coord3::point(10, 10);
        let p3 = Coord3::point(15, 15);
        rasterize_triangle(&mut scratch, p1, p2, p3, Rgb565::new(31, 0, 0), None).unwrap();
        assert_eq!(scratch.buffer()[0], 0);

        // Clockwise winding should automatically swap and paint
        let p_cw1 = Coord3::point(5, 5);
        let p_cw2 = Coord3::point(5, 15);
        let p_cw3 = Coord3::point(15, 5);
        rasterize_triangle(
            &mut scratch,
            p_cw1,
            p_cw2,
            p_cw3,
            Rgb565::new(0, 63, 0),
            Some(128),
        )
        .unwrap();
    }
}
