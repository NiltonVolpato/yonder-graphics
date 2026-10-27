//! The walk: a circle rasterizer that visits only the pixels the circle cuts.
//!
//! It follows the left half of the boundary from the top pixel next to the vertical axis down to the bottom one.
//! Each pixel is entered through one crossing and left through another, and its coverage is the area between them.
//! The right half is the mirror image, and the pixels between the two halves are filled with spans.

use core::ops::ControlFlow::{self, Break, Continue};

use yonder_geometry::{screen::Point, shapes::Circle};

use super::{
    DOUBLED_FULL, Disk, FULL, SIDE, SINGLE_PIXEL_AREA, crossing, doubled_trapezoid_area,
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
    /// A diagonal crossing leads one pixel down and `diagonal_dx` pixels sideways.
    pub fn next_pixel(self, pixel: Point, diagonal_dx: i16) -> Point {
        let Point { x, y } = pixel;
        match self {
            Crossing::Top(..) => Point::new(x, y - 1),
            Crossing::Right(..) => Point::new(x + 1, y),
            Crossing::Bottom(..) => Point::new(x, y + 1),
            Crossing::Left(..) => Point::new(x - 1, y),
            Crossing::Diagonal => Point::new(x + diagonal_dx, y + 1),
        }
    }

    /// Twice the inside area of an upper half pixel entered at `self` and left at `exit`, in 1/SIDE².
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

    /// Twice the inside area of a lower half pixel entered at `self` and left at `exit`, in 1/SIDE².
    fn lower_area(self, exit: Crossing) -> i32 {
        use Crossing::{Bottom, Diagonal, Left, Right, Top};
        // Entering through the top-left corner, all the top edge is inside. Leaving through the bottom-right corner,
        // none of the bottom edge is.
        let entry = if let Diagonal = self {
            Top(0, 0, SIDE)
        } else {
            self
        };
        let exit = if let Diagonal = exit {
            Bottom(0, 0, 0)
        } else {
            exit
        };
        match (entry, exit) {
            (Left(.., a), Right(.., b)) | (Top(.., a), Bottom(.., b)) => {
                doubled_trapezoid_area(a, b)
            }
            // Only the top-right corner inside.
            (Top(.., a), Right(.., b)) => doubled_triangle_area(a, b),
            // All but the bottom-left corner inside.
            (Left(.., a), Bottom(.., b)) => {
                DOUBLED_FULL - doubled_triangle_area(SIDE - a, SIDE - b)
            }
            _ => unreachable!(
                "the lower half enters through the top or left edge and leaves through the bottom or right"
            ),
        }
    }
}

/// A doubled area in 1/SIDE² as a coverage, rounded like `emit_cell`, and clamped below 255: a pixel the walk visits
/// is never full.
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
    center_offset: i16, // 1 if the center is the center of a pixel, 0 if it is a pixel corner
}

impl Cursor {
    /// The first pixel, in the top row, on the vertical axis or just left of it, and the crossing where the circle
    /// enters it: its right edge. The only place that multiplies.
    fn first(diameter: i32, center_offset: i16) -> (Self, Crossing) {
        let cursor = Self {
            point: Point::new(
                center_offset - 1,
                -((diameter as i16 + 1 - center_offset) / 2),
            ),
            center_offset,
        };
        let right = cursor.doubled(cursor.point.x + 1);
        let top = cursor.doubled(cursor.point.y);
        let top_right = right * right + top * top - diameter * diameter;
        let bottom_right = top_right + cursor.forward_difference(cursor.point.y);
        (
            cursor,
            Crossing::Right(top_right, bottom_right, crossing(bottom_right, top_right)),
        )
    }

    /// Corner coordinate `coord` as a distance from the center, in half pixels.
    fn doubled(&self, coord: i16) -> i32 {
        2 * i32::from(coord) - i32::from(self.center_offset)
    }

    /// How much F grows when a corner moves from coordinate `coord` to `coord + 1`, along either axis. It does not
    /// depend on the other coordinate. This is how the walk gets from one corner of a pixel to the next: one add.
    fn forward_difference(&self, coord: i16) -> i32 {
        (self.doubled(coord) + 1) << 2
    }

    /// Upper half. The circle runs down and to the left, so it leaves each pixel through its left edge or its bottom
    /// edge, and F at the pixel's bottom-left corner tells which. Moves to the next pixel, and breaks when that pixel's
    /// top edge is at or below the center.
    fn step_upper(&mut self, entry: Crossing) -> (u8, ControlFlow<Crossing, Crossing>) {
        let Point { x, y } = self.point;
        let (dx, dy) = (self.forward_difference(x), self.forward_difference(y));
        let bottom_left = match entry {
            Crossing::Right(_, bottom_right, _) => bottom_right - dx,
            Crossing::Top(top_left, ..) => top_left + dy,
            Crossing::Diagonal => dy - dx, // entered at the top-right corner, where F is 0
            Crossing::Left(..) | Crossing::Bottom(..) => unreachable!(),
        };
        let exit = if bottom_left < 0 {
            let top_left = bottom_left - dy;
            Crossing::Left(bottom_left, top_left, crossing(bottom_left, top_left))
        } else if bottom_left == 0 && self.doubled(y + 1) != 0 {
            // The bottom-left corner is exactly on the circle, and not on the horizontal axis. The pixels on the
            // other sides of the corner are covered completely or not at all, so go through the corner.
            // (On the horizontal axis the circle is vertical, and the next pixel down is cut.)
            Crossing::Diagonal
        } else {
            let bottom_right = bottom_left + dx;
            Crossing::Bottom(
                bottom_right,
                bottom_left,
                crossing(bottom_right, bottom_left),
            )
        };
        self.point = exit.next_pixel(self.point, -1);
        let next = exit.across();
        let flow = if self.point.y >= self.center_offset {
            Break(next)
        } else {
            Continue(next)
        };
        (coverage(entry.upper_area(exit)), flow)
    }

    /// Lower half. The circle runs down and to the right, so it leaves each pixel through its bottom edge or its
    /// right edge, and F at the pixel's bottom-right corner tells which. Breaks after the last pixel, which is next
    /// to the vertical axis and leaves through its right edge.
    fn step_lower(&mut self, entry: Crossing) -> (u8, ControlFlow<(), Crossing>) {
        let Point { x, y } = self.point;
        let (dx, dy) = (self.forward_difference(x), self.forward_difference(y));
        let bottom_right = match entry {
            Crossing::Left(bottom_left, ..) => bottom_left + dx,
            Crossing::Top(_, top_right, _) => top_right + dy,
            Crossing::Diagonal => dx + dy, // entered at the top-left corner, where F is 0
            Crossing::Right(..) | Crossing::Bottom(..) => unreachable!(),
        };
        let exit = if bottom_right < 0 {
            let bottom_left = bottom_right - dx;
            Crossing::Bottom(
                bottom_right,
                bottom_left,
                crossing(bottom_right, bottom_left),
            )
        } else {
            let top_right = bottom_right - dy;
            let right = Crossing::Right(top_right, bottom_right, crossing(top_right, bottom_right));
            if x >= self.center_offset - 1 {
                // The last pixel. Checked first: its bottom-right corner may be on the circle.
                return (coverage(entry.lower_area(right)), Break(()));
            }
            if bottom_right == 0 {
                Crossing::Diagonal
            } else {
                right
            } // as in the upper half
        };
        self.point = exit.next_pixel(self.point, 1);
        (coverage(entry.lower_area(exit)), Continue(exit.across()))
    }
}

/// One pixel of the walk.
#[derive(Clone, Copy)]
pub struct Step {
    pub point: Point,
    pub coverage: u8,
}

/// Walks the left half of the boundary of a circle `diameter` pixels across, from the top pixel next to the vertical
/// axis down to the bottom pixel next to it, and calls `visit` for each pixel. Pixels are relative to the center, as
/// in `Cursor`. Coverage is 0 when the circle cuts a pixel too little to show.
pub fn walk_it(diameter: i32, center_offset: i16, mut visit: impl FnMut(Step)) {
    assert!(
        (1..=1 << 15).contains(&diameter),
        "diameter must be in 1..=32768"
    ); // keeps F in an i32

    if diameter == 1 && center_offset == 1 {
        // One pixel with the whole circle inside it and no corner inside the circle: no edge is crossed.
        visit(Step {
            point: Point::new(0, 0),
            coverage: coverage(2 * SINGLE_PIXEL_AREA),
        });
        return;
    }

    let (mut cursor, mut entry) = Cursor::first(diameter, center_offset);
    entry = loop {
        let point = cursor.point;
        let (coverage, flow) = cursor.step_upper(entry);
        visit(Step { point, coverage });
        match flow {
            Continue(next) => entry = next,
            Break(next) => break next,
        }
    };
    loop {
        let point = cursor.point;
        let (coverage, flow) = cursor.step_lower(entry);
        visit(Step { point, coverage });
        match flow {
            Continue(next) => entry = next,
            Break(()) => return,
        }
    }
}

/// Fills the disk of `circle`, a row at a time: the pixels the walk visits, their mirror images, and the full span
/// between them. Not clipped yet.
pub fn fill(circle: &Circle, mut emit: impl FnMut(Coverage)) {
    let disk = Disk::grown(circle, 0); // in half pixels, so its radius is the diameter
    if disk.radius < 1 {
        return;
    }
    let center_offset = (disk.center_x & 1) as i16;
    assert_eq!(
        disk.center_y & 1,
        disk.center_x & 1,
        "the walk needs the same center offset on both axes"
    );
    let (center_x, center_y) = ((disk.center_x >> 1) as i16, (disk.center_y >> 1) as i16);
    let no_clip = (i32::MIN, i32::MAX);

    let mut row: Option<(i16, i32)> = None; // the current row, and its innermost pixel the walk visited
    walk_it(disk.radius, center_offset, |Step { point, coverage }| {
        let (x, y) = (center_x + point.x, center_y + point.y);
        let inner = match row {
            Some((row_y, inner)) if row_y == y => inner.max(x.into()),
            _ => {
                if let Some((row_y, inner)) = row {
                    span(inner + 1, disk.mirror(inner), row_y, no_clip, &mut emit);
                }
                x.into()
            }
        };
        row = Some((y, inner));
        if coverage > 0 {
            emit(Coverage::Boundary(Point::new(x, y), coverage));
            let mirror = disk.mirror(x.into()) as i16;
            if mirror != x {
                emit(Coverage::Boundary(Point::new(mirror, y), coverage));
            }
        }
    });
    if let Some((y, inner)) = row {
        span(inner + 1, disk.mirror(inner), y, no_clip, &mut emit);
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
        for diameter in 1..=200 {
            for center_offset in 0..=1 {
                if (diameter, center_offset) == (1, 1) {
                    continue;
                }
                // F at the top-left corner of pixel (x, y), from scratch.
                let f = |x: i16, y: i16| {
                    let dx = 2 * i32::from(x) - i32::from(center_offset);
                    let dy = 2 * i32::from(y) - i32::from(center_offset);
                    dx * dx + dy * dy - diameter * diameter
                };
                let mut points = Vec::new();
                walk_it(diameter, center_offset, |step| points.push(step.point));
                let context = format!("diameter {diameter}, center offset {center_offset}");

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
                assert_eq!(first.x, center_offset - 1, "{context}: first pixel");
                assert_eq!(
                    last,
                    Point::new(center_offset - 1, center_offset - 1 - first.y),
                    "{context}: last pixel"
                );
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
            .chain((1..=200).map(|side| {
                Circle::in_box(BoundingBox::new(
                    Point::new(-7, 3),
                    Point::new(side - 7, side + 3),
                ))
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
