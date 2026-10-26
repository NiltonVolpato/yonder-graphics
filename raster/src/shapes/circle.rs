//! Circles by marching squares: a pixel's coverage is the area of the polygon cut from
//! it where the circle's implicit function crosses zero on its four edges, taking the
//! function as linear along each edge. Only cells near the edge are visited.
//!
//! A cell is the square between four lattice points (pixel corners). Cell `(x, y)` is
//! pixel `(x, y)`. A lattice line is the horizontal line of corners between two rows.
//!
//! Distances are in half pixels, so that circle centers (lattice points or pixel centers)
//! and stroke radii are all integers. Positions are `i16` screen coordinates. The
//! implicit function is `i32`: it is only evaluated near the edge, where it stays small.

use core::ops::RangeInclusive;

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

    /// The circle's implicit function at `(x / 2, y / 2)`: negative inside, zero on the
    /// circle, positive outside. It is `distance² − radius²`, in half pixels squared.
    ///
    /// Only called near the edge, where the result fits `i32`; the squares need `i64`.
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

    fn reaches_row(&self, row: i16) -> bool {
        let top = 2 * i32::from(row);
        self.center_y - self.radius < top + 2 && self.center_y + self.radius > top
    }

    fn center_in_row(&self, row: i16) -> bool {
        let top = 2 * i32::from(row);
        top < self.center_y && self.center_y < top + 2
    }
}

/// Tracks the lattice points inside the disk on consecutive lattice lines, top to bottom.
///
/// Like Zingl's circle algorithm, it keeps the implicit function at the chord's left end
/// and updates it with additions as the end moves: one step changes `offset²` by
/// `4 · offset + 4` (offsets are in half pixels, so a step is 2). The one square root
/// finds the first line's end. The right end is the left one mirrored around the center.
struct Chords {
    disk: Disk,
    line: i16,
    /// The chord belongs to the circle, which may reach past the `i16` screen range.
    left: i32,
    /// `2 · left − center_x` and `2 · line − center_y`.
    offset_x: i32,
    offset_y: i32,
    /// The implicit function at `(left, line)`.
    value: i32,
}

impl Chords {
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

    /// The lattice points inside the disk on `line`: the current line or the next one.
    fn inside_on(&mut self, line: i16) -> RangeInclusive<i16> {
        debug_assert!(line == self.line || line == self.line + 1);
        if line != self.line {
            self.value += 4 * self.offset_y + 4;
            self.offset_y += 2;
            self.line = line;
        }
        // False for outside points left of the center, true from the chord's left end on.
        // It flips once along the line, so the left end only moves toward the flip.
        let past_left = |offset_x: i32, value: i32| offset_x >= 0 || value <= 0;
        while !past_left(self.offset_x, self.value) {
            self.value += 4 * self.offset_x + 4;
            self.offset_x += 2;
            self.left += 1;
        }
        loop {
            let value = self.value - 4 * self.offset_x + 4;
            if !past_left(self.offset_x - 2, value) {
                break;
            }
            self.value = value;
            self.offset_x -= 2;
            self.left -= 1;
        }
        if self.value <= 0 {
            let right = self.disk.center_x - self.left;
            screen(self.left)..=screen(right)
        } else {
            // No lattice point inside: an empty range.
            RangeInclusive::new(1, 0)
        }
    }
}

/// Cells of one row as half-open x ranges: `touched` may be covered, `full` is.
#[derive(Clone, Copy)]
struct Profile {
    touched: (i16, i16),
    full: (i16, i16),
}

const EMPTY: Profile = Profile {
    touched: (0, 0),
    full: (0, 0),
};

/// The cells of `row`, from the inside points on its top and bottom lattice lines.
fn profile(
    disk: &Disk,
    row: i16,
    top: &RangeInclusive<i16>,
    bottom: &RangeInclusive<i16>,
) -> Profile {
    let (left, right) = match (top.is_empty(), bottom.is_empty()) {
        (false, false) => (
            *top.start().min(bottom.start()),
            *top.end().max(bottom.end()),
        ),
        (false, true) => (*top.start(), *top.end()),
        (true, false) => (*bottom.start(), *bottom.end()),
        (true, true) if disk.reaches_row(row) => {
            // A cap, or a disk smaller than a pixel: narrower than one pixel, so it's in the
            // one or two cells around the center column.
            let touched = (
                (disk.center_x - 1).div_euclid(2) as i16,
                ((disk.center_x + 1).div_euclid(2) + 1) as i16,
            );
            return Profile {
                touched,
                full: (0, 0),
            };
        }
        (true, true) => return EMPTY,
    };
    let full = if top.is_empty() || bottom.is_empty() {
        (0, 0)
    } else {
        let full = (
            *top.start().max(bottom.start()),
            *top.end().min(bottom.end()),
        );
        if full.0 < full.1 { full } else { (0, 0) }
    };
    // A cell with any inside corner is touched. In the row with the widest point, the
    // edge can also bulge into one more cell on each side.
    let margin = if disk.center_in_row(row) { 2 } else { 1 };
    Profile {
        touched: (left.saturating_sub(margin), right.saturating_add(margin)),
        full,
    }
}

fn rasterize(outer: Disk, inner: Option<Disk>, clip: BoundingBox, emit: &mut impl FnMut(Coverage)) {
    let (first, end) = outer.rows();
    let first = first.max(clip.top_left.y.into()) as i16;
    let end = end.min(clip.bottom_right.y.into()).max(first.into()) as i16;
    // Only start the walk when a row is visible: it must begin near the disk.
    if first >= end {
        return;
    }
    let clip_x = (clip.top_left.x, clip.bottom_right.x);
    let mut outer_chords = Chords::new(outer, first);
    let mut inner_chords = inner.map(|disk| Chords::new(disk, first));
    let mut outer_top = outer_chords.inside_on(first);
    let mut inner_top = inner_chords.as_mut().map(|chords| chords.inside_on(first));
    for y in first..end {
        let outer_bottom = outer_chords.inside_on(y + 1);
        let inner_bottom = inner_chords.as_mut().map(|chords| chords.inside_on(y + 1));
        let outer_profile = profile(&outer, y, &outer_top, &outer_bottom);
        let inner_profile = match (inner, &inner_top, &inner_bottom) {
            (Some(disk), Some(top), Some(bottom)) => Some((disk, profile(&disk, y, top, bottom))),
            _ => None,
        };
        row(outer, outer_profile, inner_profile, y, clip_x, emit);
        outer_top = outer_bottom;
        inner_top = inner_bottom;
    }
}

/// Splits a row at every range boundary. Within a segment, every cell is in the same
/// ranges, so it is one span, one hole, or a run of edge cells.
fn row(
    outer: Disk,
    outer_profile: Profile,
    inner: Option<(Disk, Profile)>,
    y: i16,
    clip_x: (i16, i16),
    emit: &mut impl FnMut(Coverage),
) {
    let inner_profile = inner.map_or(EMPTY, |(_, profile)| profile);
    let start = outer_profile.touched.0.max(clip_x.0);
    let end = outer_profile.touched.1.min(clip_x.1).max(start);
    let mut cuts = [
        outer_profile.touched.0,
        outer_profile.touched.1,
        outer_profile.full.0,
        outer_profile.full.1,
        inner_profile.touched.0,
        inner_profile.touched.1,
        inner_profile.full.0,
        inner_profile.full.1,
    ]
    .map(|cut| cut.clamp(start, end));
    cuts.sort_unstable();
    let within = |range: (i16, i16), x: i16| range.0 <= x && x < range.1;
    for index in 0..7 {
        let (segment_start, segment_end) = (cuts[index], cuts[index + 1]);
        if segment_start >= segment_end || within(inner_profile.full, segment_start) {
            continue;
        }
        let outer_full = within(outer_profile.full, segment_start);
        let inner_touched = within(inner_profile.touched, segment_start);
        let inner_disk = inner.filter(|_| inner_touched).map(|(disk, _)| disk);
        // One loop per case, so the per-cell loop has no branches on which edges are present.
        match (outer_full, inner_disk) {
            (true, None) => emit(Coverage::Interior(cell_box(segment_start, segment_end, y))),
            (false, None) => {
                let mut outer_cells = Cells::new(outer, segment_start, y);
                for x in segment_start..segment_end {
                    emit_cell(x, y, outer_cells.area(), emit);
                }
            }
            (true, Some(inner_disk)) => {
                let mut inner_cells = Cells::new(inner_disk, segment_start, y);
                for x in segment_start..segment_end {
                    emit_cell(x, y, FULL - inner_cells.area(), emit);
                }
            }
            (false, Some(inner_disk)) => {
                let mut outer_cells = Cells::new(outer, segment_start, y);
                let mut inner_cells = Cells::new(inner_disk, segment_start, y);
                for x in segment_start..segment_end {
                    let area = outer_cells.area() - inner_cells.area();
                    emit_cell(x, y, area, emit);
                }
            }
        }
    }
}

/// Emits one cell with `area` in 1/65536 pixel, skipping empty cells.
#[inline(always)]
fn emit_cell(x: i16, y: i16, area: i32, emit: &mut impl FnMut(Coverage)) {
    match ((area.max(0) * 255 + FULL / 2) / FULL) as u8 {
        0 => {}
        255 => emit(Coverage::Interior(cell_box(x, x + 1, y))),
        partial => emit(Coverage::Boundary(Point::new(x, y), partial)),
    }
}

/// One pixel's area in 1/65536 pixel.
const FULL: i32 = 1 << 16;

/// Walks the cells of one row left to right. Neighbors share corners, and the implicit
/// function moves to the next corners by additions, as in `Chords`.
struct Cells {
    disk: Disk,
    x: i16,
    y: i16,
    /// `2 · x − center_x`.
    offset_x: i32,
    /// The implicit function at the current cell's top-left and bottom-left corners.
    left_values: [i32; 2],
}

impl Cells {
    fn new(disk: Disk, x: i16, y: i16) -> Self {
        let (corner_x, corner_y) = (2 * i32::from(x), 2 * i32::from(y));
        Self {
            disk,
            x,
            y,
            offset_x: corner_x - disk.center_x,
            left_values: [
                disk.implicit_value(corner_x, corner_y),
                disk.implicit_value(corner_x, corner_y + 2),
            ],
        }
    }

    fn area(&mut self) -> i32 {
        let step = 4 * self.offset_x + 4;
        let right_values = [self.left_values[0] + step, self.left_values[1] + step];
        let corner_values = [
            self.left_values[0],
            right_values[0],
            right_values[1],
            self.left_values[1],
        ];
        let (corner_x, corner_y) = (2 * i32::from(self.x), 2 * i32::from(self.y));
        self.left_values = right_values;
        self.offset_x += 2;
        self.x += 1;
        if corner_values.iter().all(|&value| value > 0) {
            // No corner inside: only a cap or a tiny disk reaches here. Look at the four
            // half-pixel cells instead.
            let value_at = |i: i32, j: i32| self.disk.implicit_value(corner_x + i, corner_y + j);
            let quarter_area = |i: i32, j: i32| {
                polygon_area([
                    value_at(i, j),
                    value_at(i + 1, j),
                    value_at(i + 1, j + 1),
                    value_at(i, j + 1),
                ])
            };
            (quarter_area(0, 0) + quarter_area(1, 0) + quarter_area(0, 1) + quarter_area(1, 1)) / 4
        } else {
            polygon_area(corner_values)
        }
    }
}

/// Area, in 1/65536 of the cell, where the implicit function is ≤ 0, taking it as linear
/// along each edge. `corner_values` are the function at the corners top-left, top-right,
/// bottom-right, bottom-left.
///
/// The corners inside pick the shape: a triangle at one corner, the cell minus such a
/// triangle, a trapezoid, or the cell minus two opposite triangles. Each area is computed
/// doubled, from the crossings, so it stays an integer: the same value as the shoelace
/// formula on the polygon.
fn polygon_area(corner_values: [i32; 4]) -> i32 {
    /// The cell's side in fixed-point units.
    const SIDE: i32 = 256;
    const DOUBLED_CELL: i32 = 2 * SIDE * SIDE;
    // Where edge `corner` → `corner + 1` crosses zero, as a distance from `corner`.
    let crossing = |corner: usize| {
        let next = (corner + 1) % 4;
        round_div(
            corner_values[corner] * SIDE,
            corner_values[corner] - corner_values[next],
        )
    };
    // Doubled area of the triangle cut off at `corner`: its legs run along the corner's
    // two edges, out to the crossings.
    let corner_triangle = |corner: usize| {
        let previous = (corner + 3) % 4;
        crossing(corner) * (SIDE - crossing(previous))
    };
    // Doubled area of the trapezoid where `corner` and the next corner are inside.
    let trapezoid = |corner: usize| {
        let (next, previous) = ((corner + 1) % 4, (corner + 3) % 4);
        SIDE * (crossing(next) + SIDE - crossing(previous))
    };
    let inside = corner_values.map(|value| value <= 0);
    let doubled_area = match inside {
        [false, false, false, false] => 0,
        [true, true, true, true] => DOUBLED_CELL,
        [true, false, false, false] => corner_triangle(0),
        [false, true, false, false] => corner_triangle(1),
        [false, false, true, false] => corner_triangle(2),
        [false, false, false, true] => corner_triangle(3),
        [false, true, true, true] => DOUBLED_CELL - corner_triangle(0),
        [true, false, true, true] => DOUBLED_CELL - corner_triangle(1),
        [true, true, false, true] => DOUBLED_CELL - corner_triangle(2),
        [true, true, true, false] => DOUBLED_CELL - corner_triangle(3),
        [true, true, false, false] => trapezoid(0),
        [false, true, true, false] => trapezoid(1),
        [false, false, true, true] => trapezoid(2),
        [true, false, false, true] => trapezoid(3),
        // Opposite corners inside: the inside joins them across the cell.
        [true, false, true, false] => DOUBLED_CELL - corner_triangle(1) - corner_triangle(3),
        [false, true, false, true] => DOUBLED_CELL - corner_triangle(0) - corner_triangle(2),
    };
    doubled_area / 2
}

/// `a / b` rounded to nearest, ties to even, so mirrored edges round the same way.
/// `a / b` is between 0 and 256: `a` and `b` have the same sign.
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

/// Clamps to the `i16` screen range. Anything clamped is far outside every clip.
fn screen(x: i32) -> i16 {
    x.clamp(i16::MIN.into(), i16::MAX.into()) as i16
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
