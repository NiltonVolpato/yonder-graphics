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
        }
    }

    /// Returns the next row of boundary pixels, or `None` when the walk has reached or crossed the horizontal axis.
    #[inline(always)]
    fn next_row<'a>(&'a mut self, from_top: &'a mut bool) -> Option<Row<'a>> {
        if self.point.1 >= 0 {
            return None;
        }

        Some(Row {
            innermost_x: self.point.0,
            y: self.point.1,
            cursor: self,
            from_top,
            finished: false,
        })
    }
}

/// A streaming iterator over the boundary pixels of a single horizontal row in the top-left quadrant.
struct Row<'a> {
    cursor: &'a mut Cursor,
    from_top: &'a mut bool,
    pub innermost_x: i32,
    pub y: i32,
    finished: bool,
}

impl Iterator for Row<'_> {
    type Item = (i32, u8);

    #[cfg_attr(not(feature = "no-inline"), inline(always))]
    #[cfg_attr(feature = "no-inline", inline(never))]
    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }

        let px = self.cursor.point.0;

        if self.cursor.bottom_left < 0 {
            let dy = self.cursor.dy();
            let b = inside(self.cursor.bottom_left, dy);
            let cov = if *self.from_top {
                inverted_triangle_coverage(SIDE - self.cursor.edge_inside, SIDE - b)
            } else {
                trapezoid_coverage(self.cursor.edge_inside, b)
            };
            self.cursor.point.0 -= 2;
            self.cursor.bottom_left -= self.cursor.dx();
            self.cursor.edge_inside = b;
            *self.from_top = false;
            Some((px, cov))
        } else if self.cursor.bottom_left == 0 && self.cursor.point.1 != -2 {
            let cov = if *self.from_top {
                inverted_triangle_coverage(SIDE - self.cursor.edge_inside, SIDE)
            } else {
                trapezoid_coverage(self.cursor.edge_inside, 0)
            };
            self.cursor.point.0 -= 2;
            self.cursor.point.1 += 2;
            self.cursor.bottom_left = self.cursor.dy() - self.cursor.dx();
            self.cursor.edge_inside = SIDE;
            *self.from_top = false;
            self.finished = true;
            Some((px, cov))
        } else {
            let dx = self.cursor.dx();
            let b = inside(self.cursor.bottom_left + dx, dx);
            let cov = if *self.from_top {
                trapezoid_coverage(self.cursor.edge_inside, b)
            } else {
                triangle_coverage(self.cursor.edge_inside, b)
            };
            self.cursor.point.1 += 2;
            self.cursor.bottom_left += self.cursor.dy();
            self.cursor.edge_inside = b;
            *self.from_top = true;
            self.finished = true;
            Some((px, cov))
        }
    }
}

/// Fills the disk of `circle`: the pixels the walk visits, their mirror images in the four quarters, and the full
/// span between the innermost pixel of each row and its mirror image. Not clipped yet.
pub fn fill(circle: &Circle, mut emit: impl FnMut(Coverage)) {
    let disk = Disk::grown(circle, 0); // in half pixels, so its radius is the diameter
    if disk.radius < 1 {
        return;
    }
    assert!(
        (1..=1 << 15).contains(&disk.radius),
        "diameter must be in 1..=32768"
    ); // keeps F in an i32

    let (center_x, center_y) = (disk.center_x, disk.center_y);
    let (offset_x, offset_y) = (center_x & 1 == 1, center_y & 1 == 1);
    let no_clip = (i32::MIN, i32::MAX);

    if disk.radius == 1 && (offset_x || offset_y) {
        // No corner is inside the circle, so no edge is crossed.
        if offset_x && offset_y {
            // One pixel with the whole circle inside it.
            const SINGLE_PIXEL_COVERAGE: u8 = ((SINGLE_PIXEL_AREA * 255 + FULL / 2) >> 16) as u8;
            emit(Coverage::Boundary(
                Point::new((center_x >> 1) as i16, (center_y >> 1) as i16),
                SINGLE_PIXEL_COVERAGE,
            ));
        }
        // shortcut: a circle centered on the middle of a pixel edge is dropped, like ChordWalk does; emit π/8 on the
        // two pixels if it matters.
        return;
    }

    let translate_y = |y: i32| (center_y + y) >> 1;
    let translate_x = |x: i32| (center_x + x) >> 1;

    let mut cursor = Cursor::first(disk.radius, offset_x, offset_y);
    let mut from_top = false;

    while let Some(mut row) = cursor.next_row(&mut from_top) {
        let y_top = translate_y(row.y);
        let y_bottom = translate_y(-row.y - 2);

        let mut x_left = translate_x(row.innermost_x) + 1;
        let mut x_right = translate_x(-row.innermost_x - 2) - 1;
        span(x_left, x_right + 1, y_top as i16, no_clip, &mut emit);
        if row.y != -1 {
            span(x_left, x_right + 1, y_bottom as i16, no_clip, &mut emit);
        }

        let y_walk = row.y;
        for (pixel_x, coverage) in &mut row {
            x_left -= 1;
            x_right += 1;

            if coverage == 0 {
                continue; // Skip truncation rounding errors for small coverage.
            }

            emit(Coverage::Boundary(
                Point::new(x_left as i16, y_top as i16),
                coverage,
            ));
            if pixel_x != -1 {
                emit(Coverage::Boundary(
                    Point::new(x_right as i16, y_top as i16),
                    coverage,
                ));
            }
            if y_walk == -1 {
                continue;
            }
            emit(Coverage::Boundary(
                Point::new(x_left as i16, y_bottom as i16),
                coverage,
            ));
            if pixel_x != -1 {
                emit(Coverage::Boundary(
                    Point::new(x_right as i16, y_bottom as i16),
                    coverage,
                ));
            }
        }
    }
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
                let mut cursor = Cursor::first(diameter, offset_x == 1, offset_y == 1);
                let mut from_top = false;
                while let Some(mut row) = cursor.next_row(&mut from_top) {
                    let y = row.y;
                    for (x, _cov) in &mut row {
                        points.push((x, y));
                    }
                }
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
