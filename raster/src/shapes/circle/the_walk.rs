//! The walk: a circle rasterizer that visits only the pixels the circle cuts.
//!
//! It follows the top-left quarter of the boundary, from the top pixel next to the vertical axis down to the row at
//! the center. Each pixel is entered through one crossing and left through another, and its coverage is the area
//! between them. The other three quarters are mirror images, and the pixels between them are filled with spans.

use yonder_geometry::{screen::Point, shapes::Circle};

use super::{Disk, FULL, SIDE, SINGLE_PIXEL_AREA, span};
use crate::Coverage;

/// Single-step coverage for an inside right triangle with legs in 1/SIDE.
/// Pure 32-bit integer arithmetic.
#[inline(always)]
fn triangle_coverage(leg_a: i32, leg_b: i32) -> u8 {
    ((((leg_a * leg_b) >> 1) * 255 + 32768) >> 16) as u8
}

/// Single-step coverage for an inverted triangle (outside corner cut) with uncut legs in 1/SIDE.
/// Pure 32-bit integer arithmetic.
#[inline(always)]
fn inverted_triangle_coverage(uncut_leg_a: i32, uncut_leg_b: i32) -> u8 {
    ((16_744_448 - ((uncut_leg_a * uncut_leg_b) >> 1) * 255) >> 16) as u8
}

/// Single-step coverage for a trapezoid one cell wide with parallel sides in 1/SIDE.
/// Pure 32-bit integer arithmetic.
#[inline(always)]
fn trapezoid_coverage(side_a: i32, side_b: i32) -> u8 {
    (((side_a + side_b) * 32640 + 32768) >> 16) as u8
}

/// How much of an edge is inside, in 1/SIDE, from F at its inside end and the edge difference, rounded to nearest.
/// Pure 32-bit integer division.
#[inline(always)]
fn inside(f_inside: i32, edge: i32) -> i32 {
    (f_inside * SIDE + (edge >> 1)) / edge
}

/// The pixel the walk is on, in doubled coordinate space W where the circle center is (0, 0) and each pixel has
/// width 2 and height 2, bounded by [X, X + 2] x [Y, Y + 2]. F = X^2 + Y^2 - diameter^2 at pixel corners.
#[derive(Clone, Copy)]
struct Cursor {
    point: (i32, i32),
    /// F at the pixel's bottom-left corner. The sign of this value determines whether the circle exits Left (< 0),
    /// Bottom (> 0), or diagonally through the corner (== 0).
    bottom_left: i32,
    /// How much of the entry edge is inside the circle, in 1/SIDE.
    edge_inside: i32,
    /// True if entered from above (or diagonal): this pixel is the innermost of its row.
    innermost: bool,
    /// True if entered from the top edge.
    from_top: bool,
}

impl Cursor {
    #[inline(always)]
    fn dx(&self) -> i32 {
        (self.point.0 + 1) << 2
    }

    #[inline(always)]
    fn dy(&self) -> i32 {
        (self.point.1 + 1) << 2
    }

    /// The first pixel, in the top row, on the vertical axis or just left of it, and the crossing where the circle
    /// enters it: its right edge. The only place that multiplies.
    fn first(diameter: i32, offset_x: bool, offset_y: bool) -> Self {
        let x = if offset_x { -1 } else { -2 };
        let y = -diameter - i32::from((diameter % 2 != 0) ^ offset_y);

        let dx = (x + 1) << 2;
        let dy = (y + 1) << 2;

        let top_right = y * y - diameter * diameter + i32::from(offset_x);
        let bottom_right = top_right + dy;
        let bottom_left = bottom_right - dx;
        let edge_inside = inside(bottom_right, dy);

        Self {
            point: (x, y),
            bottom_left,
            edge_inside,
            innermost: false,
            from_top: false,
        }
    }

    /// One pixel of the top-left quarter, where the circle runs down and to the left, so it leaves each pixel through
    /// its left edge or its bottom edge, and F at the pixel's bottom-left corner tells which. Moves to the next pixel
    /// and returns the current step, or `None` when the walk has reached or crossed the center horizontal axis.
    #[cfg_attr(not(feature = "no-inline"), inline(always))]
    #[cfg_attr(feature = "no-inline", inline(never))]
    fn step(&mut self) -> Option<Step> {
        if self.point.1 >= 0 {
            return None;
        }

        let point = self.point;
        let innermost = self.innermost;

        let coverage = if self.bottom_left < 0 {
            let b = inside(self.bottom_left, self.dy());
            let cov = if self.from_top {
                inverted_triangle_coverage(SIDE - self.edge_inside, SIDE - b)
            } else {
                trapezoid_coverage(self.edge_inside, b)
            };
            self.point.0 -= 2;
            self.bottom_left -= self.dx();
            self.innermost = false;
            self.from_top = false;
            self.edge_inside = b;
            cov
        } else if self.bottom_left == 0 && self.point.1 != -2 {
            // The bottom-left corner is exactly on the circle, and not on the horizontal axis. The pixels on the
            // other sides of the corner are covered completely or not at all, so go through the corner.
            // (On the horizontal axis the circle is vertical, and the next pixel down is cut.)
            let cov = if self.from_top {
                inverted_triangle_coverage(SIDE - self.edge_inside, SIDE)
            } else {
                trapezoid_coverage(self.edge_inside, 0)
            };
            self.point.0 -= 2;
            self.point.1 += 2;
            self.bottom_left = self.dy() - self.dx();
            self.innermost = true;
            self.from_top = false;
            self.edge_inside = SIDE;
            cov
        } else {
            let dx = self.dx();
            let b = inside(self.bottom_left + dx, dx);
            let cov = if self.from_top {
                trapezoid_coverage(self.edge_inside, b)
            } else {
                triangle_coverage(self.edge_inside, b)
            };
            self.point.1 += 2;
            self.bottom_left += self.dy();
            self.innermost = true;
            self.from_top = true;
            self.edge_inside = b;
            cov
        };

        Some(Step {
            point,
            coverage,
            innermost,
        })
    }
}

/// One pixel of the walk.
#[derive(Clone, Copy)]
pub struct Step {
    pub point: (i32, i32),
    pub coverage: u8,
    /// The innermost pixel of its row: the pixels from the next one to its mirror image are inside.
    pub innermost: bool,
}

/// Walks the top-left quarter of the boundary of a circle `diameter` pixels across, from the top pixel next to the
/// vertical axis down to the row at the center, and calls `visit` for each pixel. Pixels are in doubled coordinates
/// relative to the center. Coverage is 0 when the circle cuts a pixel too little to show.
pub fn walk_it(diameter: i32, (offset_x, offset_y): (bool, bool), mut visit: impl FnMut(Step)) {
    assert!(
        (1..=1 << 15).contains(&diameter),
        "diameter must be in 1..=32768"
    ); // keeps F in an i32

    if diameter == 1 && (offset_x || offset_y) {
        // No corner is inside the circle, so no edge is crossed.
        if offset_x && offset_y {
            // One pixel with the whole circle inside it.
            const SINGLE_PIXEL_COVERAGE: u8 = ((SINGLE_PIXEL_AREA * 255 + FULL / 2) >> 16) as u8;
            visit(Step {
                point: (-1, -1),
                coverage: SINGLE_PIXEL_COVERAGE,
                innermost: true,
            });
        }
        // shortcut: a circle centered on the middle of a pixel edge is dropped, like ChordWalk does; emit π/8 on the
        // two pixels if it matters.
        return;
    }

    let mut cursor = Cursor::first(diameter, offset_x, offset_y);
    while let Some(step) = cursor.step() {
        visit(step);
    }
}

/// Fills the disk of `circle`: the pixels the walk visits, their mirror images in the four quarters, and the full
/// span between the innermost pixel of each row and its mirror image. Not clipped yet.
pub fn fill(circle: &Circle, mut emit: impl FnMut(Coverage)) {
    let disk = Disk::grown(circle, 0); // in half pixels, so its radius is the diameter
    if disk.radius < 1 {
        return;
    }
    let (center_x, center_y) = (disk.center_x, disk.center_y);
    let (offset_x, offset_y) = (center_x & 1 == 1, center_y & 1 == 1);
    let no_clip = (i32::MIN, i32::MAX);

    walk_it(disk.radius, (offset_x, offset_y), |step| {
        let (x, y) = (
            (center_x + step.point.0) >> 1,
            (center_y + step.point.1) >> 1,
        );
        let mirror_x = (center_x - step.point.0 - 2) >> 1;
        let mirror_y = (center_y - step.point.1 - 2) >> 1;
        let mut row = |y: i32| {
            if step.innermost {
                span(x + 1, mirror_x, y as i16, no_clip, &mut emit);
            }
            if step.coverage > 0 {
                emit(Coverage::Boundary(
                    Point::new(x as i16, y as i16),
                    step.coverage,
                ));
                if mirror_x != x {
                    emit(Coverage::Boundary(
                        Point::new(mirror_x as i16, y as i16),
                        step.coverage,
                    ));
                }
            }
        };
        row(y);
        // The row through a pixel center is its own mirror image.
        if mirror_y != y {
            row(mirror_y);
        }
    });
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::format;
    use std::vec::Vec;

    use yonder_geometry::projective::PointT2;
    use yonder_geometry::screen::BoundingBox;

    use super::*;
    use crate::{Filled, Rasterizable};

    #[test]
    fn walk_visits_only_cut_pixels_one_step_at_a_time() {
        let offsets = [(0, 0), (1, 1), (0, 1), (1, 0)];
        for diameter in 1..=200 {
            for (offset_x, offset_y) in offsets {
                if diameter == 1 && (offset_x, offset_y) != (0, 0) {
                    continue;
                }
                // In doubled coordinate space W, the circle center is always (0, 0) and radius is diameter.
                // F at corner (x, y) from scratch:
                let f = |x: i32, y: i32| x * x + y * y - diameter * diameter;
                let mut points = Vec::new();
                walk_it(diameter, (offset_x == 1, offset_y == 1), |step| {
                    points.push(step.point)
                });
                let context = format!("diameter {diameter}, offset ({offset_x}, {offset_y})");

                for &(x, y) in &points {
                    // Pixel bounding box is [x, x + 2] x [y, y + 2].
                    let corners = [f(x, y), f(x + 2, y), f(x, y + 2), f(x + 2, y + 2)];
                    assert!(
                        corners.iter().any(|&f| f < 0) && corners.iter().any(|&f| f >= 0),
                        "{context}: pixel ({x}, {y}) is not cut"
                    );
                }
                for pair in points.windows(2) {
                    let (dx, dy) = (pair[1].0 - pair[0].0, pair[1].1 - pair[0].1);
                    assert!(
                        (dy == 0 && dx == -2) || (dy == 2 && (dx == 0 || dx == -2)),
                        "{context}: bad move from {:?} to {:?}",
                        pair[0],
                        pair[1]
                    );
                }
                let (first, last) = (points[0], points[points.len() - 1]);
                assert_eq!(first.0, offset_x - 2, "{context}: first pixel");
                assert_eq!(last.1, offset_y - 2, "{context}: last pixel");
            }
        }
    }

    /// Coverage per pixel, checking that no pixel comes twice.
    fn collect(pixels: &mut HashMap<(i16, i16), u8>, item: Coverage) {
        let mut put = |x, y, coverage| {
            assert!(
                pixels.insert((x, y), coverage).is_none(),
                "pixel ({x}, {y}) twice"
            )
        };
        match item {
            Coverage::Boundary(point, coverage) => {
                assert!(coverage > 0);
                put(point.x, point.y, coverage);
            }
            Coverage::Partial(..) => unreachable!("circles emit no uniform partial areas"),
            Coverage::Interior(bounds) => {
                for y in bounds.top_left.y..bounds.bottom_right.y {
                    for x in bounds.top_left.x..bounds.bottom_right.x {
                        put(x, y, 255);
                    }
                }
            }
        }
    }

    /// The walk computes the same crossings and areas as `ChordWalk`, so the pixels must match. The only difference
    /// is a pixel that rounds to full: `ChordWalk` emits 255, the walk clamps it to 254.
    #[test]
    fn fill_matches_chord_walk() {
        let screen = BoundingBox::new(Point::new(-300, -300), Point::new(300, 300));
        let circles = (1..=100)
            .flat_map(|radius| {
                [
                    Circle::new(PointT2::new(3, -2, 1), radius),
                    Circle::at_pixel_center(Point::new(-4, 5), radius),
                ]
            })
            // Square boxes, and boxes one pixel wider or taller: their center is on a pixel edge on one axis.
            .chain((1..=200).flat_map(|side| {
                [(0, 0), (1, 0), (0, 1)].map(|(wider, taller)| {
                    Circle::in_box(BoundingBox::new(
                        Point::new(-7, 3),
                        Point::new(side + wider - 7, side + taller + 3),
                    ))
                })
            }));
        for circle in circles {
            let (mut walk, mut chord_walk) = (HashMap::new(), HashMap::new());
            fill(&circle, |item| collect(&mut walk, item));
            Filled(&circle).rasterize(screen, |item| collect(&mut chord_walk, item));
            let [x, y, w, r] = circle.homogeneous();
            for (pixel, &expected) in &chord_walk {
                let got = walk.get(pixel).copied();
                let matches = match got {
                    Some(g) => (g as i16 - expected as i16).abs() <= 1,
                    None => false,
                };
                assert!(
                    matches,
                    "circle [{x}, {y}, {w}, {r}] pixel {pixel:?}: {got:?}, expected {expected}"
                );
            }
            assert_eq!(
                walk.len(),
                chord_walk.len(),
                "circle [{x}, {y}, {w}, {r}]: extra pixels"
            );
        }
    }
}
