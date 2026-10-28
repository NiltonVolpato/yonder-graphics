//! The walk: a circle rasterizer that visits only the pixels the circle cuts.
//!
//! It follows the top-left quarter of the boundary, from the top pixel next to the vertical axis down to the row at
//! the center. Each pixel is entered through one crossing and left through another, and its coverage is the area
//! between them. The other three quarters are mirror images, and the pixels between them are filled with spans.

use yonder_geometry::{screen::Point, shapes::Circle};

use super::{
    DOUBLED_FULL, Disk, FULL, SIDE, SINGLE_PIXEL_AREA, doubled_trapezoid_area,
    doubled_triangle_area, span,
};
use crate::Coverage;

/// A crossing: the edge of a pixel where the circle crosses it, F at both ends of that edge in clockwise order, and
/// how much of the edge is inside the circle, in 1/SIDE, measured from its inside end. Measured that way, the inside
/// part is the same seen from the pixels on both sides of the edge.
#[derive(Clone, Copy)]
pub enum Crossing {
    Top(i32, i32, i32),
    Right(i32, i32, i32),
    Bottom(i32, i32, i32),
    Left(i32, i32, i32),

    /// Through a corner that lies exactly on the circle, to the pixel on the other side of that corner.
    Diagonal,
}

impl Crossing {
    /// The same crossing seen from the pixel on the other side of the edge.
    #[cfg_attr(not(feature = "no-inline"), inline(always))]
    #[cfg_attr(feature = "no-inline", inline(never))]
    pub fn across(self) -> Self {
        match self {
            Crossing::Top(f0, f1, inside) => Crossing::Bottom(f1, f0, inside),
            Crossing::Right(f0, f1, inside) => Crossing::Left(f1, f0, inside),
            Crossing::Bottom(f0, f1, inside) => Crossing::Top(f1, f0, inside),
            Crossing::Left(f0, f1, inside) => Crossing::Right(f1, f0, inside),
            Crossing::Diagonal => Crossing::Diagonal,
        }
    }

    /// The pixel on the other side of this crossing, seen from `pixel`.
    /// A diagonal crossing leads one pixel down and to the left, the way the walk goes.
    #[cfg_attr(not(feature = "no-inline"), inline(always))]
    #[cfg_attr(feature = "no-inline", inline(never))]
    pub fn next_pixel(self, pixel: Point) -> Point {
        let Point { x, y } = pixel;
        match self {
            Crossing::Top(..) => Point::new(x, y - 1),
            Crossing::Right(..) => Point::new(x + 1, y),
            Crossing::Bottom(..) => Point::new(x, y + 1),
            Crossing::Left(..) => Point::new(x - 1, y),
            Crossing::Diagonal => Point::new(x - 1, y + 1),
        }
    }

    /// Twice the inside area of a pixel entered at `self` and left at `exit`, in 1/SIDE².
    #[inline(always)]
    fn upper_area(self, exit: Crossing) -> i32 {
        use Crossing::{Bottom, Diagonal, Left, Right, Top};
        // A diagonal goes through a corner on the circle. Entering through the top-right corner, all the right edge
        // is inside. Leaving through the bottom-left corner, none of the left edge is.
        let entry = if let Diagonal = self {
            Right(0, 0, SIDE)
        } else {
            self
        };
        let exit = if let Diagonal = exit {
            Left(0, 0, 0)
        } else {
            exit
        };
        match (entry, exit) {
            (Right(.., a), Left(.., b)) | (Top(.., a), Bottom(.., b)) => {
                doubled_trapezoid_area(a, b)
            }
            // Only the bottom-right corner inside.
            (Right(.., a), Bottom(.., b)) => doubled_triangle_area(a, b),
            // All but the top-left corner inside.
            (Top(.., a), Left(.., b)) => DOUBLED_FULL - doubled_triangle_area(SIDE - a, SIDE - b),
            _ => unreachable!(
                "the upper half enters through the top or right edge and leaves through the left or bottom"
            ),
        }
    }
}

/// A doubled area in 1/SIDE² as a coverage, rounded like `emit_cell`, and clamped below 255: a pixel the walk visits
/// is never full.
#[cfg_attr(not(feature = "no-inline"), inline(always))]
#[cfg_attr(feature = "no-inline", inline(never))]
fn coverage(doubled_area: i32) -> u8 {
    // FULL is 1 << 16.
    (((doubled_area >> 1) * 255 + FULL / 2) >> 16).min(254) as u8
}

/// The pixel the walk is on, relative to the pixel whose top-left corner is the center (a corner center), or that
/// contains it (a pixel center). Corners are measured from the center in half pixels, so they are integers either
/// way, and F = 4 * (distance^2 - radius^2) = dx^2 + dy^2 - diameter^2 at a corner.
#[derive(Clone, Copy)]
struct Cursor {
    point: Point,
    /// Per axis, 1 if the center is in the middle of a pixel, 0 if it is on a pixel edge. Usually both are equal; a
    /// circle in a box that isn't square can have one of each.
    center_offset: Point,
}

impl Cursor {
    /// The first pixel, in the top row, on the vertical axis or just left of it, and the crossing where the circle
    /// enters it: its right edge. The only place that multiplies.
    fn first(diameter: i32, center_offset: Point) -> (Self, Crossing) {
        let cursor = Self {
            point: Point::new(
                center_offset.x - 1,
                -((diameter as i16 + 1 - center_offset.y) / 2),
            ),
            center_offset,
        };
        let right = doubled(cursor.point.x + 1, center_offset.x);
        let top = doubled(cursor.point.y, center_offset.y);
        let top_right = right * right + top * top - diameter * diameter;
        let bottom_right = top_right + forward_difference(cursor.point.y, center_offset.y);
        (
            cursor,
            Crossing::Right(top_right, bottom_right, inside(bottom_right, top_right)),
        )
    }

    /// One pixel of the top-left quarter, where the circle runs down and to the left, so it leaves each pixel through
    /// its left edge or its bottom edge, and F at the pixel's bottom-left corner tells which. Moves to the next pixel
    /// and returns the crossing to enter it, or `None` when that pixel's top edge is at or below the center. A pixel
    /// entered from above is the innermost of its row.
    #[cfg_attr(not(feature = "no-inline"), inline(always))]
    #[cfg_attr(feature = "no-inline", inline(never))]
    fn step(&mut self, entry: Crossing) -> (Step, Option<Crossing>) {
        let (Point { x, y }, offset) = (self.point, self.center_offset);
        let (dx, dy) = (
            forward_difference(x, offset.x),
            forward_difference(y, offset.y),
        );
        let (bottom_left, innermost) = match entry {
            Crossing::Right(_, bottom_right, _) => (bottom_right - dx, false),
            Crossing::Top(top_left, ..) => (top_left + dy, true),
            Crossing::Diagonal => (dy - dx, true), // entered at the top-right corner, where F is 0
            Crossing::Left(..) | Crossing::Bottom(..) => unreachable!(),
        };
        let exit = if bottom_left < 0 {
            let top_left = bottom_left - dy;
            Crossing::Left(bottom_left, top_left, inside(bottom_left, top_left))
        } else if bottom_left == 0 && doubled(y + 1, offset.y) != 0 {
            // The bottom-left corner is exactly on the circle, and not on the horizontal axis. The pixels on the
            // other sides of the corner are covered completely or not at all, so go through the corner.
            // (On the horizontal axis the circle is vertical, and the next pixel down is cut.)
            Crossing::Diagonal
        } else {
            let bottom_right = bottom_left + dx;
            Crossing::Bottom(bottom_right, bottom_left, inside(bottom_right, bottom_left))
        };
        let point = self.point;
        self.point = exit.next_pixel(point);
        let next = (self.point.y < offset.y).then(|| exit.across());
        let coverage = coverage(entry.upper_area(exit));
        (
            Step {
                point,
                coverage,
                innermost,
            },
            next,
        )
    }
}

/// How much of an edge is inside, in 1/SIDE, from F at its inside end and at its outside end, rounded to nearest.
/// One division: unlike `round_div`, ties don't need to go to even, because an edge and its mirror image give the
/// same values in the same order.
#[cfg_attr(not(feature = "no-inline"), inline(always))]
#[cfg_attr(feature = "no-inline", inline(never))]
fn inside(f_inside: i32, f_outside: i32) -> i32 {
    let edge = f_inside - f_outside; // negative, like f_inside
    (f_inside * SIDE + (edge >> 1)) / edge
}

/// Corner coordinate `coord` as a distance from the center, in half pixels.
#[inline(always)]
fn doubled(coord: i16, center_offset: i16) -> i32 {
    2 * i32::from(coord) - i32::from(center_offset)
}

/// How much F grows when a corner moves from coordinate `coord` to `coord + 1`, along either axis. It does not depend
/// on the other coordinate. This is how the walk gets from one corner of a pixel to the next: one add.
#[inline(always)]
fn forward_difference(coord: i16, center_offset: i16) -> i32 {
    (doubled(coord, center_offset) + 1) << 2
}

/// One pixel of the walk.
#[derive(Clone, Copy)]
pub struct Step {
    pub point: Point,
    pub coverage: u8,
    /// The innermost pixel of its row: the pixels from the next one to its mirror image are inside.
    pub innermost: bool,
}

/// Walks the top-left quarter of the boundary of a circle `diameter` pixels across, from the top pixel next to the
/// vertical axis down to the row at the center, and calls `visit` for each pixel. Pixels are relative to the center,
/// as in `Cursor`. Coverage is 0 when the circle cuts a pixel too little to show.
pub fn walk_it(diameter: i32, center_offset: Point, mut visit: impl FnMut(Step)) {
    assert!(
        (1..=1 << 15).contains(&diameter),
        "diameter must be in 1..=32768"
    ); // keeps F in an i32

    if diameter == 1 && center_offset != Point::new(0, 0) {
        // No corner is inside the circle, so no edge is crossed.
        if center_offset == Point::new(1, 1) {
            // One pixel with the whole circle inside it.
            visit(Step {
                point: Point::new(0, 0),
                coverage: coverage(2 * SINGLE_PIXEL_AREA),
                innermost: true,
            });
        }
        // shortcut: a circle centered on the middle of a pixel edge is dropped, like ChordWalk does; emit π/8 on the
        // two pixels if it matters.
        return;
    }

    let (mut cursor, entry) = Cursor::first(diameter, center_offset);
    let mut next = Some(entry);
    while let Some(entry) = next {
        let step;
        (step, next) = cursor.step(entry);
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
    let center_offset = Point::new((disk.center_x & 1) as i16, (disk.center_y & 1) as i16);
    let (center_x, center_y) = ((disk.center_x >> 1) as i16, (disk.center_y >> 1) as i16);
    let no_clip = (i32::MIN, i32::MAX);

    walk_it(disk.radius, center_offset, |step| {
        let (x, y) = (center_x + step.point.x, center_y + step.point.y);
        let mirror_x = disk.mirror(x.into());
        let mirror_y = (disk.center_y - i32::from(y) - 1) as i16; // like `Disk::mirror`, vertically
        let mut row = |y: i16| {
            if step.innermost {
                span(i32::from(x) + 1, mirror_x, y, no_clip, &mut emit);
            }
            if step.coverage > 0 {
                emit(Coverage::Boundary(Point::new(x, y), step.coverage));
                if mirror_x != i32::from(x) {
                    emit(Coverage::Boundary(
                        Point::new(mirror_x as i16, y),
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
        let offsets = [(0, 0), (1, 1), (0, 1), (1, 0)].map(|(x, y)| Point::new(x, y));
        for diameter in 1..=200 {
            for center_offset in offsets {
                if diameter == 1 && center_offset != Point::new(0, 0) {
                    continue;
                }
                // F at the top-left corner of pixel (x, y), from scratch.
                let f = |x: i16, y: i16| {
                    let dx = 2 * i32::from(x) - i32::from(center_offset.x);
                    let dy = 2 * i32::from(y) - i32::from(center_offset.y);
                    dx * dx + dy * dy - diameter * diameter
                };
                let mut points = Vec::new();
                walk_it(diameter, center_offset, |step| points.push(step.point));
                let context = format!("diameter {diameter}, center offset {center_offset:?}");

                for &Point { x, y } in &points {
                    let corners = [f(x, y), f(x + 1, y), f(x, y + 1), f(x + 1, y + 1)];
                    assert!(
                        corners.iter().any(|&f| f < 0) && corners.iter().any(|&f| f >= 0),
                        "{context}: pixel ({x}, {y}) is not cut"
                    );
                }
                for pair in points.windows(2) {
                    let (dx, dy) = (pair[1].x - pair[0].x, pair[1].y - pair[0].y);
                    assert!(
                        (dy == 0 && dx.abs() == 1) || (dy == 1 && dx.abs() <= 1),
                        "{context}: bad move from {:?} to {:?}",
                        pair[0],
                        pair[1]
                    );
                }
                let (first, last) = (points[0], points[points.len() - 1]);
                assert_eq!(first.x, center_offset.x - 1, "{context}: first pixel");
                assert_eq!(last.y, center_offset.y - 1, "{context}: last pixel");
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
                assert!(coverage > 0 && coverage < 255);
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
                assert!(
                    got == Some(expected) || (expected == 255 && got == Some(254)),
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
