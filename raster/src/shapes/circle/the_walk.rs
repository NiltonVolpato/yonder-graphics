//! Circle rasterizer with anti-aliasing support.

use yonder_geometry::{screen::Point, shapes::Circle};

/// A crossing: the edge of a pixel where the circle crosses it, and F at both ends of that edge.
/// TODO: must also include the midpoint, which should be computed on creation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Crossing {
    Top(i32, i32),
    Right(i32, i32),
    Bottom(i32, i32),
    Left(i32, i32),

    /// Through a corner that lies exactly on the circle, to the pixel on the other side of that corner.
    Diagonal,
}

impl Crossing {
    /// The same crossing seen from the pixel on the other side of the edge.
    pub fn across(self) -> Self {
        match self {
            Crossing::Top(f0, f1) => Crossing::Bottom(f1, f0),
            Crossing::Right(f0, f1) => Crossing::Left(f1, f0),
            Crossing::Bottom(f0, f1) => Crossing::Top(f1, f0),
            Crossing::Left(f0, f1) => Crossing::Right(f1, f0),
            Crossing::Diagonal => Crossing::Diagonal,
        }
    }

    /// The pixel on the other side of this crossing, seen from pixel (x, y).
    /// A diagonal crossing leads one pixel down and `diagonal_dx` pixels sideways.
    pub fn next_pixel(self, x: i32, y: i32, diagonal_dx: i32) -> (i32, i32) {
        match self {
            Crossing::Top(..) => (x, y - 1),
            Crossing::Right(..) => (x + 1, y),
            Crossing::Bottom(..) => (x, y + 1),
            Crossing::Left(..) => (x - 1, y),
            Crossing::Diagonal => (x + diagonal_dx, y + 1),
        }
    }
}

/// A pixel of the walk. It keeps the doubled center so that it can compute how F changes from one side to the
/// other, where F = 4 * (distance^2 - radius^2) at a corner.
#[derive(Clone, Copy)]
struct Cursor {
    diameter: i16,
    current: Point, // Scaled by a factor of 2.
}

impl Cursor {
    fn new(diameter: i16) -> Self {
        Self {
            diameter,
            current: Point::new(0, 0),
        }
    }

    fn first(diameter: i16) -> Self {
        let center_offset = diameter % 2; // 1 if the center is the center of a pixel, 0 if it is a pixel corner
        let x0 = center_offset - 1; //   the column of the first and last pixel: on the vertical axis, or just left of it
        let y0 = -(diameter / 2);
        Self {
            diameter,
            current: Point::new(x0, y0),
        }
    }

    // F evaluated at the grid in between pixels. By convention the top-left corner
    // of pixel `(x, y)` is at coordinates `(x, y)`. This is computed with multiplications,
    // but only the first and last pixels use it.
    fn evaluate(&self, x: i32, y: i32) -> i32 {
        let dx2 = 2 * x; // the distances from the center, doubled
        let dy2 = 2 * y;
        dx2 * dx2 + dy2 * dy2 - self.diameter as i32 * self.diameter as i32
    }

    fn forward_difference(&self, coord: i32) -> i32 {
        (2 * coord + 1 - self.diameter as i32 % 2) << 2
    }
}

/// One pixel of the walk.
#[derive(Clone, Copy, Debug)]
pub struct Step {
    pub point: Point,
    pub coverage: u8,
}

/// Walks the left half of the boundary of a circle that touches all four sides of a `diameter` x `diameter` box,
/// from the top pixel next to the vertical axis down to the bottom pixel next to it, and calls `visit` for each pixel.
/// Pixel (x, y) is the unit square with corners at the integer points (x, y) to (x + 1, y + 1), and y grows downward.
/// The center of the circle is the center of a pixel when the diameter is odd, and a pixel corner when it is even.
///
/// The walk keeps only the current pixel and the crossing where the circle entered it. A crossing holds F at both
/// ends of its edge: 4 * (distance^2 - radius^2) at that corner, where the distance is measured from the center
/// of the circle. It is negative inside the circle, positive outside, and 0 on the circle.
pub fn walk_it(circle: Circle, mut visit: impl FnMut(Step)) {
    assert!(circle.radius() <= (1 << 14), "radius must be <= 16384");

    let diameter = circle.radius() as i16 * 2;
    let mut cursor = Cursor::first(diameter);

    if diameter == 1 {
        // One pixel with the whole circle inside it and no corner inside the circle: no edge is crossed.
        visit(Step {
            point: cursor.current,
            coverage: 64,
        });
        return;
    }

    // How much F grows when a corner moves from coordinate `coord` to `coord + 1`, along either axis. It does not depend
    // on the other coordinate. This is how the walk gets from one corner of a pixel to the next: one add.
    let forward_difference = |coord: i32| (2 * coord + 1 - center_offset) << 2;

    // The circle enters the first pixel and leaves the last pixel through their right edges, which are
    // on the vertical axis, or next to it.
    let mut next_entry = Some(Crossing::Right(
        cursor.evaluate(x0 + 1, y0),
        cursor.evaluate(x0 + 1, y0 + 1),
    ));

    // Upper half. The circle runs down and to the left, so it leaves each pixel through its left edge or
    // its bottom edge, and F at the pixel's bottom-left corner tells which.
    while let Some(entry) = next_entry.take() {
        let bottom_left = match entry {
            Crossing::Right(_, bottom_right) => bottom_right - forward_difference(x),
            Crossing::Top(top_left, _) => top_left + forward_difference(y),
            Crossing::Diagonal => forward_difference(y) - forward_difference(x), // entered at the top-right corner, where F is 0
            Crossing::Left(..) | Crossing::Bottom(..) => unreachable!(),
        };
        let exit = if bottom_left < 0 {
            Crossing::Left(bottom_left, bottom_left - forward_difference(y))
        } else if bottom_left == 0 && y + 1 < 0 {
            // The bottom-left corner is exactly on the circle, and not on the horizontal axis. The pixels on the
            // other sides of the corner are covered completely or not at all, so go through the corner.
            // (On the horizontal axis the circle is vertical, and the next pixel down is cut.)
            Crossing::Diagonal
        } else {
            Crossing::Bottom(bottom_left + forward_difference(x), bottom_left)
        };
        let (coverage, next_entry) = cursor.step_upper(entry);
        visit(Step {
            point: Point { x, y },
            coverage: coverage,
        });
    }

    // Lower half. The circle runs down and to the right, so it leaves each pixel through its bottom edge or
    // its right edge, and F at the pixel's bottom-right corner tells which.
    while let Some(entry) = next_entry.take() {
        let bottom_right = match entry {
            Crossing::Left(bottom_left, _) => bottom_left + forward_difference(x),
            Crossing::Top(_, top_right) => top_right + forward_difference(y),
            Crossing::Diagonal => forward_difference(x) + forward_difference(y), // entered at the top-left corner, where F is 0
            Crossing::Right(..) | Crossing::Bottom(..) => unreachable!(),
        };
        let exit = if bottom_right < 0 {
            Crossing::Bottom(bottom_right, bottom_right - forward_difference(x))
        } else if bottom_right == 0 {
            Crossing::Diagonal // as above
        } else {
            Crossing::Right(bottom_right - forward_difference(y), bottom_right)
        };
        let (coverage, next_entry) = cursor.step_lower(entry);
        visit(Step {
            point: Point { x, y },
            coverage: coverage,
        });
    }

    visit(Step {
        point: Point { x, y },
        coverage: 0,
    });
    // entry: Some(entry),
    // exit: Some(Crossing::Right(corner_f(x + 1, y), corner_f(x + 1, y + 1))),
}
