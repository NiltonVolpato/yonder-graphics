//! Circles by marching squares. A pixel's coverage is the area of the polygon cut from it
//! where the circle's implicit function `F = distance² − radius²` crosses zero on its
//! four edges, taking `F` as linear along each edge. `F` is evaluated at lattice points
//! (pixel corners), so two neighboring pixels share the crossing on their common edge.
//!
//! The walk follows the circle down the lattice lines, like Zingl's circle algorithm: it
//! keeps each line's leftmost inside point and `F` there, updated by additions. Only the
//! edge cells left of the center are computed; the right side is their mirror.
//!
//! Lattice line `y` is the horizontal line of corners above pixel row `y`. Cell `(x, y)`
//! is pixel `(x, y)`, with corners on lines `y` and `y + 1`.
//!
//! Distances are in half pixels, so circle centers (lattice points or pixel centers) and
//! stroke radii are integers. A step of one pixel changes an offset by 2 and `F` by
//! `4 · offset + 4`. Positions are `i16` screen coordinates. `F` is `i32`: it is only
//! evaluated near the edge, where it stays small.

use yonder_geometry::screen::{BoundingBox, Point};
use yonder_geometry::shapes::Circle;

use crate::{Coverage, Filled, Rasterizable, Stroked};

impl Rasterizable for Filled<'_, Circle> {
    fn rasterize(&self, clip: BoundingBox, mut emit: impl FnMut(Coverage)) {
        rasterize(Disk::grown(self.0, 0), None, clip, &mut emit);
    }
}

impl Rasterizable for Stroked<'_, Circle> {
    fn rasterize(&self, clip: BoundingBox, mut emit: impl FnMut(Coverage)) {
        let width = i32::from(self.width);
        let inner = Disk::grown(self.shape, -width);
        rasterize(
            Disk::grown(self.shape, width),
            (inner.radius > 0).then_some(inner),
            clip,
            &mut emit,
        );
    }
}

/// One pixel's area in 1/65536 pixel.
const FULL: i32 = 1 << 16;

/// A cell's side in fixed-point units: crossings are in 1/SIDE, areas in 1/SIDE².
const SIDE: i32 = 256;

/// A disk in half pixels: center `(center_x / 2, center_y / 2)`, radius `radius / 2`.
#[derive(Clone, Copy)]
struct Disk {
    center_x: i32,
    center_y: i32,
    radius: i32,
}

impl Disk {
    /// The circle grown by `stroke_width / 2` pixels, or shrunk when it is negative.
    fn grown(circle: &Circle, stroke_width: i32) -> Self {
        // `w` is 1 for a lattice-point center and 2 for a pixel center (see `Circle::new`).
        let [x, y, w, radius] = circle.homogeneous();
        let to_half_pixels = 2 / w;
        Self {
            center_x: x * to_half_pixels,
            center_y: y * to_half_pixels,
            radius: radius * to_half_pixels + stroke_width,
        }
    }

    /// `F` at `(x / 2, y / 2)`, in half pixels squared. Only for starting the walk: the
    /// squares need `i64`, and the result fits `i32` near the edge.
    fn implicit_value(&self, x: i32, y: i32) -> i32 {
        let dx = i64::from(x - self.center_x);
        let dy = i64::from(y - self.center_y);
        let radius = i64::from(self.radius);
        let value = dx * dx + dy * dy - radius * radius;
        debug_assert!(
            i32::try_from(value).is_ok(),
            "implicit value far from the edge"
        );
        value as i32
    }

    /// Rows the disk reaches into, half-open.
    fn rows(&self) -> (i32, i32) {
        (
            (self.center_y - self.radius).div_euclid(2),
            ceil_div(self.center_y + self.radius, 2),
        )
    }

    /// The mirror of cell `x` around the center.
    fn mirror(&self, x: i32) -> i32 {
        self.center_x - x - 1
    }

    /// The pixel of a disk one pixel across around a pixel center. All four corners of
    /// that pixel are outside, so corner sampling can't see it. It is the only such disk.
    fn single_pixel(&self) -> Option<(i32, i32)> {
        (self.radius == 1 && self.center_x % 2 != 0 && self.center_y % 2 != 0)
            .then(|| (self.center_x.div_euclid(2), self.center_y.div_euclid(2)))
    }
}

/// The area of a disk one pixel across, π/4 of a pixel.
const SINGLE_PIXEL_AREA: i32 = 51_472;

/// Walks the circle down the lattice lines. On each line it stops at the chord's left
/// end, the leftmost inside point, and keeps `F` there. Moving one point sideways or one
/// line down changes `F` by its partial derivative, so the walk only adds.
struct ChordWalk {
    disk: Disk,
    line: i16,
    /// The current point: the chord's left end, or the center when no point is inside.
    left: i32,
    /// `2 · left − center_x` and `2 · line − center_y`.
    offset_x: i32,
    offset_y: i32,
    /// `F` at `(left, line)`.
    value: i32,
}

impl ChordWalk {
    /// Starts on `line`. The one square root per shape finds the chord's half width there.
    fn new(disk: Disk, line: i16) -> Self {
        let offset_y = 2 * i32::from(line) - disk.center_y;
        let radius = i64::from(disk.radius);
        let half_chord_squared = radius * radius - i64::from(offset_y) * i64::from(offset_y);
        let half_chord = half_chord_squared.max(0).unsigned_abs().isqrt() as i32;
        let left = ceil_div(disk.center_x - half_chord, 2);
        Self {
            disk,
            line,
            left,
            offset_x: 2 * left - disk.center_x,
            offset_y,
            value: disk.implicit_value(2 * left, 2 * i32::from(line)),
        }
    }

    /// Moves to `line`, the current line or the next one down, and returns it.
    fn line(&mut self, line: i16) -> Line {
        debug_assert!(line == self.line || line == self.line + 1);
        if line != self.line {
            // One line down: F(x, y + 1) = F(x, y) + 4 · offset_y + 4.
            self.value += 4 * self.offset_y + 4;
            self.offset_y += 2;
            self.line = line;
        }
        // Below the center the chord narrows: step right while the point is outside, but
        // not past the center. F(x + 1, y) = F(x, y) + 4 · offset_x + 4.
        while self.offset_x < 0 && self.value > 0 {
            self.value += 4 * self.offset_x + 4;
            self.offset_x += 2;
            self.left += 1;
        }
        // Above the center it widens: step left while the next point out is inside.
        // F(x − 1, y) = F(x, y) − 4 · offset_x + 4.
        loop {
            let outward = self.value - 4 * self.offset_x + 4;
            if outward > 0 {
                break;
            }
            self.value = outward;
            self.offset_x -= 2;
            self.left -= 1;
        }
        let empty = self.value > 0;
        Line {
            first_inside: if empty { i32::MAX } else { self.left },
            last_inside: self.disk.center_x - self.left,
            left: self.left,
            offset_x: self.offset_x,
            offset_y: self.offset_y,
            value: self.value,
            // Between the outside point `left − 1` and `left`.
            crossing: if empty {
                0
            } else {
                crossing(self.value - 4 * self.offset_x + 4, self.value)
            },
        }
    }
}

/// A lattice line of one disk, as its row's edge cells need it.
#[derive(Clone, Copy)]
struct Line {
    /// The inside points, `first_inside..=last_inside`. `first_inside` is `i32::MAX`
    /// when none is.
    first_inside: i32,
    last_inside: i32,
    /// Where the walk stopped, with its offsets and `F` there: the start for stepping
    /// `F` along the line.
    left: i32,
    offset_x: i32,
    offset_y: i32,
    value: i32,
    /// Where the line crosses zero between `first_inside − 1` and `first_inside`, as a
    /// distance from `first_inside − 1`. Shared by the rows above and below the line.
    crossing: i32,
}

impl Line {
    #[inline(always)]
    fn inside(&self, x: i32) -> bool {
        self.first_inside <= x && x <= self.last_inside
    }
}

fn rasterize(outer: Disk, inner: Option<Disk>, clip: BoundingBox, emit: &mut impl FnMut(Coverage)) {
    let clip_x = (i32::from(clip.top_left.x), i32::from(clip.bottom_right.x));
    if let Some((x, y)) = outer.single_pixel() {
        if i32::from(clip.top_left.y) <= y && y < i32::from(clip.bottom_right.y) {
            emit_cell(x, y as i16, SINGLE_PIXEL_AREA, clip_x, emit);
        }
        return;
    }
    // A ring whose hole is one pixel across: that pixel loses the hole's area.
    let hole = inner.and_then(|disk| disk.single_pixel());
    let inner = inner.filter(|_| hole.is_none());

    let (first, end) = outer.rows();
    let first = first.max(clip.top_left.y.into()) as i16;
    let end = end.min(clip.bottom_right.y.into()).max(first.into()) as i16;
    // Only start the walk when a row is visible: it must begin near the disk.
    if first >= end {
        return;
    }
    let mut outer_walk = ChordWalk::new(outer, first);
    let mut inner_walk = inner.map(|disk| ChordWalk::new(disk, first));
    let mut outer_top = outer_walk.line(first);
    let mut inner_top = inner_walk.as_mut().map(|walk| walk.line(first));
    for y in first..end {
        let outer_bottom = outer_walk.line(y + 1);
        let inner_bottom = inner_walk.as_mut().map(|walk| walk.line(y + 1));
        let inner_lines = match (inner, &inner_top, &inner_bottom) {
            (Some(disk), Some(top), Some(bottom)) => Some((disk, top, bottom)),
            _ => None,
        };
        let hole = hole.filter(|&(_, hole_y)| hole_y == i32::from(y));
        row(
            (outer, &outer_top, &outer_bottom),
            inner_lines,
            y,
            clip_x,
            hole,
            emit,
        );
        outer_top = outer_bottom;
        inner_top = inner_bottom;
    }
}

/// Emits row `y`: the left edge cells and their mirrors, and the full spans between them.
/// For a ring, a cell both edges cross is covered by the outer area minus the inner one.
fn row(
    (disk, top, bottom): (Disk, &Line, &Line),
    inner: Option<(Disk, &Line, &Line)>,
    y: i16,
    clip_x: (i32, i32),
    hole: Option<(i32, i32)>,
    emit: &mut impl FnMut(Coverage),
) {
    let Some(mut outer) = EdgeCells::new(disk, top, bottom) else {
        return;
    };
    let inner = inner.and_then(|(disk, top, bottom)| EdgeCells::new(disk, top, bottom));
    let cell = |x: i32, area: i32, emit: &mut _| {
        emit_cell(x, y, area, clip_x, emit);
        if disk.mirror(x) != x {
            emit_cell(disk.mirror(x), y, area, clip_x, emit);
        }
    };
    let Some(mut inner) = inner else {
        for x in outer.first..=outer.last {
            cell(x, outer.next_area(), emit);
        }
        let (first, end) = (outer.last + 1, disk.mirror(outer.last));
        match hole {
            Some((hole_x, _)) if first <= hole_x && hole_x < end => {
                span(first, hole_x, y, clip_x, emit);
                emit_cell(hole_x, y, FULL - SINGLE_PIXEL_AREA, clip_x, emit);
                span(hole_x + 1, end, y, clip_x, emit);
            }
            _ => span(first, end, y, clip_x, emit),
        }
        return;
    };
    let mut x = outer.first;
    while x <= outer.last && x < inner.first {
        cell(x, outer.next_area(), emit);
        x += 1;
    }
    if x < inner.first {
        span(x, inner.first, y, clip_x, emit);
        span(
            disk.mirror(inner.first - 1),
            disk.mirror(x - 1),
            y,
            clip_x,
            emit,
        );
        x = inner.first;
    }
    // Past the inner edge cells is the hole.
    while x <= inner.last {
        let outer_area = if x <= outer.last {
            outer.next_area()
        } else {
            FULL
        };
        cell(x, outer_area - inner.next_area(), emit);
        x += 1;
    }
}

/// The edge cells of one disk in one row, left of the center: the cells from where either
/// line's inside starts to where both have started. They are walked left to right.
struct EdgeCells<'a> {
    top: &'a Line,
    bottom: &'a Line,
    first: i32,
    last: i32,
    /// `F` one line down minus `F` here, at every column: the step in `y`.
    down: i32,
    /// The next cell, its left column's `2 · x − center_x`, and `F` there on the top line.
    x: i32,
    offset_x: i32,
    top_value: i32,
    /// Where the next cell's left edge crosses zero, as a distance from its top.
    left_crossing: i32,
}

impl<'a> EdgeCells<'a> {
    /// `None` when neither line has inside points: then the disk doesn't reach the row.
    fn new(disk: Disk, top: &'a Line, bottom: &'a Line) -> Option<Self> {
        let first_inside = top.first_inside.min(bottom.first_inside);
        if first_inside == i32::MAX {
            return None;
        }
        let first = first_inside - 1;
        // The last cell left of the center, or on it when the center is a pixel center.
        let center = (disk.center_x - 1).div_euclid(2);
        let last = (top.first_inside.max(bottom.first_inside) - 1).min(center);
        // `F` on the top line at column `first`, stepped from where the walk stopped.
        let steps = first - top.left;
        let mut cells = Self {
            top,
            bottom,
            first,
            last,
            x: first,
            down: 4 * top.offset_y + 4,
            offset_x: top.offset_x + 2 * steps,
            top_value: top.value + 4 * steps * (top.offset_x + steps),
            left_crossing: 0,
        };
        cells.left_crossing = cells.vertical_crossing(first, cells.top_value);
        Some(cells)
    }

    /// Where column `x` crosses zero between the two lines, as a distance from the top,
    /// given `F` at its top. Zero when it doesn't cross.
    #[inline(always)]
    fn vertical_crossing(&self, x: i32, top_value: i32) -> i32 {
        if self.top.inside(x) == self.bottom.inside(x) {
            return 0;
        }
        crossing(top_value, top_value + self.down)
    }

    /// The inside area of the next cell, in 1/65536 pixel.
    #[inline(always)]
    fn next_area(&mut self) -> i32 {
        let x = self.x;
        self.x += 1;
        // One column right: F(x + 1, y) = F(x, y) + 4 · offset_x + 4.
        self.top_value += 4 * self.offset_x + 4;
        self.offset_x += 2;
        let right_crossing = self.vertical_crossing(x + 1, self.top_value);
        let inside = [
            self.top.inside(x),
            self.top.inside(x + 1),
            self.bottom.inside(x + 1),
            self.bottom.inside(x),
        ];
        // Each from its edge's first corner, going around the cell. A line's crossing is
        // measured from its left point, a column's from its top.
        let crossings = [
            self.top.crossing,
            right_crossing,
            SIDE - self.bottom.crossing,
            SIDE - self.left_crossing,
        ];
        self.left_crossing = right_crossing;
        cell_area(inside, crossings)
    }
}

/// Where `F` crosses zero between a corner where it is `from` and one where it is `to`
/// (of opposite signs), as a distance from the first corner in 1/SIDE.
#[inline(always)]
fn crossing(from: i32, to: i32) -> i32 {
    round_div(from * SIDE, from - to)
}

/// The inside area of a cell, in 1/65536 pixel. `inside` says which corners are inside,
/// and `crossings` where each edge crosses zero, as a distance from the edge's first
/// corner, for corners top-left, top-right, bottom-right, bottom-left. Only crossings of
/// edges with one corner inside are read.
#[inline(always)]
fn cell_area(inside: [bool; 4], crossings: [i32; 4]) -> i32 {
    // The triangle at `corner`: its legs run along the corner's two edges to the crossings.
    let triangle = |corner: usize| {
        doubled_triangle_area(crossings[corner], SIDE - crossings[(corner + 3) % 4])
    };
    // The trapezoid where `corner` and the next corner are inside: its parallel sides run
    // from each of them along its other edge to the crossing.
    let trapezoid = |corner: usize| {
        doubled_trapezoid_area(
            SIDE - crossings[(corner + 3) % 4],
            crossings[(corner + 1) % 4],
        )
    };
    const DOUBLED_FULL: i32 = 2 * FULL;
    let doubled_area = match inside {
        [false, false, false, false] => 0,
        [true, true, true, true] => DOUBLED_FULL,
        [true, false, false, false] => triangle(0),
        [false, true, false, false] => triangle(1),
        [false, false, true, false] => triangle(2),
        [false, false, false, true] => triangle(3),
        [false, true, true, true] => DOUBLED_FULL - triangle(0),
        [true, false, true, true] => DOUBLED_FULL - triangle(1),
        [true, true, false, true] => DOUBLED_FULL - triangle(2),
        [true, true, true, false] => DOUBLED_FULL - triangle(3),
        [true, true, false, false] => trapezoid(0),
        [false, true, true, false] => trapezoid(1),
        [false, false, true, true] => trapezoid(2),
        [true, false, false, true] => trapezoid(3),
        // Two opposite corners inside needs a disk smaller than a pixel.
        [true, false, true, false] | [false, true, false, true] => {
            unreachable!("only a disk smaller than a pixel has opposite corners inside")
        }
    };
    doubled_area / 2
}

/// Twice the area of a right triangle with legs `leg` and `other_leg`, in 1/SIDE².
/// Doubled, it stays an integer: the same value as the shoelace formula.
fn doubled_triangle_area(leg: i32, other_leg: i32) -> i32 {
    leg * other_leg
}

/// Twice the area of a trapezoid one cell wide, with parallel sides `side` and
/// `other_side`, in 1/SIDE².
fn doubled_trapezoid_area(side: i32, other_side: i32) -> i32 {
    SIDE * (side + other_side)
}

/// Emits cell `x` with `area`, in 1/65536 pixel, when it is visible and not empty.
#[inline(always)]
fn emit_cell(x: i32, y: i16, area: i32, clip_x: (i32, i32), emit: &mut impl FnMut(Coverage)) {
    if x < clip_x.0 || x >= clip_x.1 {
        return;
    }
    match ((area.max(0) * 255 + FULL / 2) / FULL) as u8 {
        0 => {}
        255 => emit(Coverage::Interior(cell_box(x as i16, x as i16 + 1, y))),
        partial => emit(Coverage::Boundary(Point::new(x as i16, y), partial)),
    }
}

/// Emits the full cells `first..end`, clipped.
fn span(first: i32, end: i32, y: i16, clip_x: (i32, i32), emit: &mut impl FnMut(Coverage)) {
    let first = first.max(clip_x.0);
    let end = end.min(clip_x.1);
    if first < end {
        emit(Coverage::Interior(cell_box(first as i16, end as i16, y)));
    }
}

/// `a / b` rounded to nearest, ties to even, so mirrored edges round the same way.
/// `a / b` is between 0 and SIDE: `a` and `b` have the same sign.
#[inline(always)]
fn round_div(a: i32, b: i32) -> i32 {
    let (a, b) = if b < 0 { (-a, -b) } else { (a, b) };
    let quotient = a / b;
    let remainder = a - quotient * b;
    if 2 * remainder > b || (2 * remainder == b && quotient % 2 != 0) {
        quotient + 1
    } else {
        quotient
    }
}

fn ceil_div(a: i32, b: i32) -> i32 {
    -(-a).div_euclid(b)
}

fn cell_box(x0: i16, x1: i16, y: i16) -> BoundingBox {
    BoundingBox::new(Point::new(x0, y), Point::new(x1, y + 1))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::vec::Vec;

    use yonder_geometry::projective::PointT2;

    use super::*;

    fn screen() -> BoundingBox {
        BoundingBox::new(Point::new(-100, -100), Point::new(100, 100))
    }

    /// Coverage per pixel, checking that no pixel is produced twice.
    fn pixels(shape: &impl Rasterizable, clip: BoundingBox) -> HashMap<(i16, i16), u8> {
        let mut out = HashMap::new();
        let mut put = |x, y, coverage| {
            assert!(
                out.insert((x, y), coverage).is_none(),
                "pixel ({x}, {y}) twice"
            )
        };
        shape.rasterize(clip, |item| match item {
            Coverage::Boundary(position, coverage) => {
                assert!(coverage > 0 && coverage < 255);
                put(position.x, position.y, coverage);
            }
            Coverage::Partial(..) => unreachable!("circles emit no uniform partial areas"),
            Coverage::Interior(bounds) => {
                for y in bounds.top_left.y..bounds.bottom_right.y {
                    for x in bounds.top_left.x..bounds.bottom_right.x {
                        put(x, y, 255);
                    }
                }
            }
        });
        out
    }

    fn count(shape: &impl Rasterizable, clip: BoundingBox) -> usize {
        let mut count = 0;
        shape.rasterize(clip, |_| count += 1);
        count
    }

    fn area(covered: &HashMap<(i16, i16), u8>) -> f64 {
        covered
            .values()
            .map(|&coverage| f64::from(coverage) / 255.0)
            .sum()
    }

    fn circles() -> Vec<(Circle, f64)> {
        let lattice = |x, y, radius| {
            (
                Circle::new(PointT2::new(x, y, 1), radius),
                f64::from(radius),
            )
        };
        let centered = |x, y, radius| {
            (
                Circle::new(PointT2::pixel_center(Point::new(x, y)), radius),
                f64::from(radius),
            )
        };
        let boxed = |x0, y0, side: i16| {
            (
                Circle::in_box(BoundingBox::new(
                    Point::new(x0, y0),
                    Point::new(x0 + side, y0 + side),
                )),
                f64::from(side) / 2.0,
            )
        };
        [
            lattice(0, 0, 5),
            lattice(3, -7, 1),
            lattice(-10, 4, 40),
            centered(0, 0, 5),
            centered(2, 3, 1),
            centered(-9, 11, 23),
            boxed(0, 0, 7),
            boxed(-5, 2, 8),
            boxed(1, 1, 3),
        ]
        .into()
    }

    #[test]
    fn fill_area_matches_disk() {
        for (circle, radius) in circles() {
            let covered = pixels(&Filled(&circle), screen());
            let expected = core::f64::consts::PI * radius * radius;
            // Linear crossings sit slightly inside the convex edge: about 1 px² short in total,
            // whatever the radius.
            let error = area(&covered) - expected;
            assert!(
                (-1.25..0.05).contains(&error),
                "radius = {radius}: {error:+.3}"
            );
        }
    }

    #[test]
    fn stroke_area_matches_ring() {
        for (circle, radius) in circles() {
            for width in [1u16, 2, 3, 6] {
                let covered = pixels(
                    &Stroked {
                        shape: &circle,
                        width,
                    },
                    screen(),
                );
                let (outer, inner) = (
                    radius + f64::from(width) / 2.0,
                    (radius - f64::from(width) / 2.0).max(0.0),
                );
                let expected = core::f64::consts::PI * (outer * outer - inner * inner);
                // The outer and inner edges lose about the same, unless the inner one is tiny.
                let error = area(&covered) - expected;
                let range = if inner >= 1.0 {
                    -0.25..0.25
                } else {
                    -1.25..0.25
                };
                assert!(
                    range.contains(&error),
                    "radius = {radius}, width = {width}: {error:+.3}"
                );
            }
        }
    }

    #[test]
    fn centered_circle_is_symmetric() {
        let circle = Circle::new(PointT2::pixel_center(Point::new(0, 0)), 6);
        let covered = pixels(&Filled(&circle), screen());
        for (&(x, y), &coverage) in &covered {
            for mirror in [(-x, y), (x, -y), (y, x)] {
                assert_eq!(
                    covered.get(&mirror).copied().unwrap_or(0),
                    coverage,
                    "({x}, {y}) vs {mirror:?}"
                );
            }
        }
    }

    #[test]
    fn clipping_keeps_the_same_pixels() {
        for (circle, _) in circles() {
            let full = pixels(&Filled(&circle), screen());
            let clip = BoundingBox::new(Point::new(-3, -2), Point::new(4, 5));
            let clipped = pixels(&Filled(&circle), clip);
            let expected: HashMap<_, _> = full
                .into_iter()
                .filter(|&((x, y), _)| (-3..4).contains(&x) && (-2..5).contains(&y))
                .collect();
            assert_eq!(clipped, expected);
        }
    }

    #[test]
    fn largest_circles_stay_in_range() {
        // Debug builds check every addition: the walk and the implicit values must fit at
        // the documented limit of 14-bit coordinates.
        const LIMIT: i16 = (1 << 14) - 1;
        let circles = [
            Circle::new(PointT2::new(0, 0, 1), LIMIT),
            Circle::at_pixel_center(Point::new(LIMIT - 1, -LIMIT), LIMIT),
        ];
        let clips = [
            BoundingBox::new(Point::new(-LIMIT, -LIMIT), Point::new(LIMIT, -LIMIT + 3)),
            BoundingBox::new(Point::new(-LIMIT, -2), Point::new(LIMIT, 2)),
            BoundingBox::new(Point::new(LIMIT - 3, -LIMIT), Point::new(LIMIT, LIMIT)),
        ];
        for circle in &circles {
            for clip in clips {
                assert!(count(&Filled(circle), clip) > 0);
                let ring = Stroked {
                    shape: circle,
                    width: 9,
                };
                assert!(
                    count(&ring, clip) > 0,
                    "{:?} {clip:?}",
                    circle.homogeneous()
                );
            }
        }
    }

    #[test]
    fn tiny_circle_is_not_lost() {
        // Diameter 1 on a pixel center: no lattice point is inside.
        let circle = Circle::in_box(BoundingBox::new(Point::new(0, 0), Point::new(1, 1)));
        let covered = pixels(&Filled(&circle), screen());
        assert!(area(&covered) > 0.5, "{covered:?}");
    }
}
