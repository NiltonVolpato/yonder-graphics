//! Circles by marching squares: a pixel's coverage is the area of the polygon cut from
//! it where the circle's implicit function crosses zero on its four edges, taking the
//! function as linear along each edge. Only cells near the edge are visited.
//!
//! A cell is the square between four lattice points (pixel corners). Cell `(x, y)` is
//! pixel `(x, y)`.

use yonder_geometry::screen::{BoundingBox, Point};
use yonder_geometry::shapes::Circle;

use crate::{Coverage, Filled, Rasterizable, Stroked};

impl Rasterizable for Filled<'_, Circle> {
    fn rasterize(&self, clip: BoundingBox) -> impl Iterator<Item = Coverage> {
        rasterize(Disk::grown(self.0, 0), None, clip)
    }
}

impl Rasterizable for Stroked<'_, Circle> {
    fn rasterize(&self, clip: BoundingBox) -> impl Iterator<Item = Coverage> {
        let width = i64::from(self.width);
        let inner = Disk::grown(self.shape, -width);
        rasterize(
            Disk::grown(self.shape, width),
            (inner.radius > 0).then_some(inner),
            clip,
        )
    }
}

/// A disk with center `(x / w, y / w)` and radius `radius / w`, `w > 0`.
///
/// `x`, `y`, `w` are the usual names for homogeneous coordinates; `w` is the common
/// denominator that keeps half-pixel centers and radii integral.
#[derive(Clone, Copy)]
struct Disk {
    x: i64,
    y: i64,
    w: i64,
    radius: i64,
}

impl Disk {
    /// The circle grown by `stroke_width / 2` pixels, or shrunk when it is negative.
    /// Coordinates are doubled when needed so half-pixel radii stay integers.
    fn grown(circle: &Circle, stroke_width: i64) -> Self {
        let [x, y, w, radius] = circle.homogeneous().map(i64::from);
        if stroke_width == 0 {
            Self { x, y, w, radius }
        } else {
            Self {
                x: 2 * x,
                y: 2 * y,
                w: 2 * w,
                radius: 2 * radius + stroke_width * w,
            }
        }
    }

    /// The circle's implicit function at the homogeneous point `(point_x / point_w,
    /// point_y / point_w)`: negative inside, zero on the circle, positive outside.
    ///
    /// It is `distance² − radius²` (the power of the point), scaled by `(w · point_w)²` so
    /// it stays an integer.
    fn implicit_value(&self, point_x: i64, point_y: i64, point_w: i64) -> i64 {
        let dx = self.w * point_x - self.x * point_w;
        let dy = self.w * point_y - self.y * point_w;
        dx * dx + dy * dy - self.radius * self.radius * point_w * point_w
    }

    /// Rows the disk reaches into, half-open.
    fn rows(&self) -> (i64, i64) {
        (
            (self.y - self.radius).div_euclid(self.w),
            ceil_div(self.y + self.radius, self.w),
        )
    }

    fn reaches_row(&self, row: i64) -> bool {
        self.y - self.radius < self.w * (row + 1) && self.y + self.radius > self.w * row
    }

    fn center_in_row(&self, row: i64) -> bool {
        self.w * row < self.y && self.y < self.w * (row + 1)
    }
}

/// Tracks the lattice points inside the disk on consecutive horizontal lattice lines.
/// Each line starts from the previous one, so the walk costs O(radius) in total.
struct Chords {
    disk: Disk,
    left: i64,
    right: i64,
}

impl Chords {
    /// Starts on `line` with the one square root per shape.
    fn new(disk: Disk, line: i64) -> Self {
        let offset_y = disk.w * line - disk.y;
        let half_chord = (disk.radius * disk.radius - offset_y * offset_y)
            .max(0)
            .unsigned_abs()
            .isqrt() as i64;
        Self {
            disk,
            left: ceil_div(disk.x - half_chord, disk.w),
            right: (disk.x + half_chord).div_euclid(disk.w),
        }
    }

    /// The inside points `left..=right` on `line`, empty when `left > right`. Lines go
    /// in order.
    fn inside_on(&mut self, line: i64) -> (i64, i64) {
        let Disk { x, w, .. } = self.disk;
        let offset_y = w * line - self.disk.y;
        let half_chord_squared = self.disk.radius * self.disk.radius - offset_y * offset_y;
        if half_chord_squared < 0 {
            return (1, 0);
        }
        let inside = |point_x: i64| (w * point_x - x) * (w * point_x - x) <= half_chord_squared;
        // Both predicates flip once along the line, so each walk only moves toward the edge.
        let past_left = |point_x: i64| w * point_x >= x || inside(point_x);
        let before_right = |point_x: i64| w * point_x <= x || inside(point_x);
        while !past_left(self.left) {
            self.left += 1;
        }
        while past_left(self.left - 1) {
            self.left -= 1;
        }
        while !before_right(self.right) {
            self.right -= 1;
        }
        while before_right(self.right + 1) {
            self.right += 1;
        }
        (self.left, self.right)
    }
}

/// Cells of one row as half-open x ranges: `touched` may be covered, `full` is.
#[derive(Clone, Copy)]
struct Profile {
    touched: (i64, i64),
    full: (i64, i64),
}

const EMPTY: Profile = Profile {
    touched: (0, 0),
    full: (0, 0),
};

/// The cells of `row`, from the inside points on its top and bottom lattice lines.
fn profile(disk: &Disk, row: i64, top: (i64, i64), bottom: (i64, i64)) -> Profile {
    let has_points = |points: (i64, i64)| points.0 <= points.1;
    let (left, right) = match (has_points(top), has_points(bottom)) {
        (true, true) => (top.0.min(bottom.0), top.1.max(bottom.1)),
        (true, false) => top,
        (false, true) => bottom,
        (false, false) if disk.reaches_row(row) => {
            // A cap, or a disk smaller than a pixel: narrower than one pixel, so it's in the
            // one or two cells around the center column.
            let (doubled_x, doubled_w) = (2 * disk.x, 2 * disk.w);
            let touched = (
                (doubled_x - disk.w).div_euclid(doubled_w),
                (doubled_x + disk.w).div_euclid(doubled_w) + 1,
            );
            return Profile {
                touched,
                full: (0, 0),
            };
        }
        (false, false) => return EMPTY,
    };
    let full = (top.0.max(bottom.0), top.1.min(bottom.1));
    let full = if has_points(top) && has_points(bottom) && full.0 < full.1 {
        full
    } else {
        (0, 0)
    };
    // A cell with any inside corner is touched. In the row with the widest point, the
    // edge can also bulge into one more cell on each side.
    let margin = if disk.center_in_row(row) { 2 } else { 1 };
    Profile {
        touched: (left - margin, right + margin),
        full,
    }
}

fn rasterize(
    outer: Disk,
    inner: Option<Disk>,
    clip: BoundingBox,
) -> impl Iterator<Item = Coverage> {
    let (first, end) = outer.rows();
    let rows = first.max(clip.top_left.y.into())..end.min(clip.bottom_right.y.into());
    let clip_x = (i64::from(clip.top_left.x), i64::from(clip.bottom_right.x));
    let mut chords = (
        Chords::new(outer, rows.start),
        inner.map(|disk| Chords::new(disk, rows.start)),
    );
    let mut top = (
        chords.0.inside_on(rows.start),
        chords.1.as_mut().map(|inner| inner.inside_on(rows.start)),
    );
    rows.flat_map(move |y| {
        let bottom = (
            chords.0.inside_on(y + 1),
            chords.1.as_mut().map(|inner| inner.inside_on(y + 1)),
        );
        let outer_profile = profile(&outer, y, top.0, bottom.0);
        let inner_profile = match (inner, top.1, bottom.1) {
            (Some(disk), Some(top), Some(bottom)) => Some((disk, profile(&disk, y, top, bottom))),
            _ => None,
        };
        top = bottom;
        row(outer, outer_profile, inner_profile, y, clip_x)
    })
}

/// Splits a row at every range boundary. Within a segment, every cell is in the same
/// ranges, so it is one span, one hole, or a run of edge cells.
fn row(
    outer: Disk,
    outer_profile: Profile,
    inner: Option<(Disk, Profile)>,
    y: i64,
    clip_x: (i64, i64),
) -> impl Iterator<Item = Coverage> {
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
    let within = |range: (i64, i64), x: i64| range.0 <= x && x < range.1;
    (0..7usize)
        .filter(move |&index| cuts[index] < cuts[index + 1])
        .flat_map(move |index| {
            let (segment_start, segment_end) = (cuts[index], cuts[index + 1]);
            let outer_full = within(outer_profile.full, segment_start);
            let inner_touched = within(inner_profile.touched, segment_start);
            if within(inner_profile.full, segment_start) {
                Segment::Done
            } else if outer_full && !inner_touched {
                Segment::Span(Some(Coverage::Interior(cell_box(
                    segment_start,
                    segment_end,
                    y,
                ))))
            } else {
                Segment::Cells {
                    y,
                    x: segment_start,
                    end: segment_end,
                    outer: (!outer_full).then(|| Cells::new(outer, segment_start, y)),
                    inner: inner
                        .filter(|_| inner_touched)
                        .map(|(disk, _)| Cells::new(disk, segment_start, y)),
                }
            }
        })
}

enum Segment {
    Done,
    Span(Option<Coverage>),
    Cells {
        y: i64,
        x: i64,
        end: i64,
        outer: Option<Cells>,
        inner: Option<Cells>,
    },
}

impl Iterator for Segment {
    type Item = Coverage;

    fn next(&mut self) -> Option<Coverage> {
        match self {
            Segment::Done => None,
            Segment::Span(span) => span.take(),
            Segment::Cells {
                y,
                x,
                end,
                outer,
                inner,
            } => {
                while *x < *end {
                    let outer_area = outer.as_mut().map_or(FULL, Cells::area);
                    let inner_area = inner.as_mut().map_or(0, Cells::area);
                    let coverage =
                        (((outer_area - inner_area).max(0) * 255 + FULL / 2) / FULL) as u8;
                    let position = Point::new(saturating_i16(*x), saturating_i16(*y));
                    *x += 1;
                    match coverage {
                        0 => {}
                        255 => return Some(Coverage::Interior(cell_box(*x - 1, *x, *y))),
                        partial => return Some(Coverage::Boundary(position, partial)),
                    }
                }
                None
            }
        }
    }
}

/// One pixel's area in 1/65536 pixel.
const FULL: i64 = 1 << 16;

/// Walks the cells of one row left to right, sharing corners between neighbors.
struct Cells {
    disk: Disk,
    x: i64,
    y: i64,
    /// The implicit function at the current cell's top-left and bottom-left corners.
    left_values: [i64; 2],
}

impl Cells {
    fn new(disk: Disk, x: i64, y: i64) -> Self {
        Self {
            disk,
            x,
            y,
            left_values: [
                disk.implicit_value(x, y, 1),
                disk.implicit_value(x, y + 1, 1),
            ],
        }
    }

    fn area(&mut self) -> i64 {
        let (disk, x, y) = (self.disk, self.x, self.y);
        let right_values = [
            disk.implicit_value(x + 1, y, 1),
            disk.implicit_value(x + 1, y + 1, 1),
        ];
        let corner_values = [
            self.left_values[0],
            right_values[0],
            right_values[1],
            self.left_values[1],
        ];
        self.left_values = right_values;
        self.x += 1;
        if corner_values.iter().all(|&value| value > 0) {
            // No corner inside: only a cap or a tiny disk reaches here. Look at the four
            // half-pixel cells instead.
            let value_at = |i: i64, j: i64| disk.implicit_value(2 * x + i, 2 * y + j, 2);
            let quarter_area = |i: i64, j: i64| {
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
fn polygon_area(corner_values: [i64; 4]) -> i64 {
    /// The cell's side in fixed-point units.
    const SIDE: i64 = 256;
    const CORNERS: [(i64, i64); 4] = [(0, 0), (SIDE, 0), (SIDE, SIDE), (0, SIDE)];
    let inside = |corner: usize| corner_values[corner] <= 0;
    let mut vertices = [(0, 0); 8];
    let mut count = 0;
    for corner in 0..4 {
        let next = (corner + 1) % 4;
        if inside(corner) {
            vertices[count] = CORNERS[corner];
            count += 1;
        }
        if inside(corner) != inside(next) {
            // Where the edge crosses zero, as a fraction of SIDE from `corner`.
            let crossing = round_div(
                corner_values[corner] * SIDE,
                corner_values[corner] - corner_values[next],
            );
            let ((x0, y0), (x1, y1)) = (CORNERS[corner], CORNERS[next]);
            vertices[count] = (
                x0 + (x1 - x0) * crossing / SIDE,
                y0 + (y1 - y0) * crossing / SIDE,
            );
            count += 1;
        }
    }
    // Shoelace formula.
    let doubled_area: i64 = (0..count)
        .map(|k| {
            let ((x0, y0), (x1, y1)) = (vertices[k], vertices[(k + 1) % count]);
            x0 * y1 - x1 * y0
        })
        .sum();
    doubled_area.abs() / 2
}

/// `a / b` rounded to nearest, ties to even, so mirrored edges round the same way.
fn round_div(a: i64, b: i64) -> i64 {
    let (a, b) = if b < 0 { (-a, -b) } else { (a, b) };
    let (quotient, remainder) = (a.div_euclid(b), a.rem_euclid(b));
    if 2 * remainder > b || (2 * remainder == b && quotient % 2 != 0) {
        quotient + 1
    } else {
        quotient
    }
}

fn ceil_div(a: i64, b: i64) -> i64 {
    -(-a).div_euclid(b)
}

fn saturating_i16(value: i64) -> i16 {
    value.clamp(i16::MIN.into(), i16::MAX.into()) as i16
}

fn cell_box(x0: i64, x1: i64, y: i64) -> BoundingBox {
    BoundingBox::new(
        Point::new(saturating_i16(x0), saturating_i16(y)),
        Point::new(saturating_i16(x1), saturating_i16(y + 1)),
    )
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
    fn pixels(items: impl Iterator<Item = Coverage>) -> HashMap<(i16, i16), u8> {
        let mut out = HashMap::new();
        let mut put = |x, y, coverage| {
            assert!(
                out.insert((x, y), coverage).is_none(),
                "pixel ({x}, {y}) twice"
            )
        };
        for item in items {
            match item {
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
            }
        }
        out
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
            (Circle::new(PointT2::new(10, 7, 3), 6), 6.0),
        ]
        .into()
    }

    #[test]
    fn fill_area_matches_disk() {
        for (circle, radius) in circles() {
            let covered = pixels(Filled(&circle).rasterize(screen()));
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
                    Stroked {
                        shape: &circle,
                        width,
                    }
                    .rasterize(screen()),
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
        let covered = pixels(Filled(&circle).rasterize(screen()));
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
            let full = pixels(Filled(&circle).rasterize(screen()));
            let clip = BoundingBox::new(Point::new(-3, -2), Point::new(4, 5));
            let clipped = pixels(Filled(&circle).rasterize(clip));
            let expected: HashMap<_, _> = full
                .into_iter()
                .filter(|&((x, y), _)| (-3..4).contains(&x) && (-2..5).contains(&y))
                .collect();
            assert_eq!(clipped, expected);
        }
    }

    #[test]
    fn tiny_circle_is_not_lost() {
        // Diameter 1 on a pixel center: no lattice point is inside.
        let circle = Circle::in_box(BoundingBox::new(Point::new(0, 0), Point::new(1, 1)));
        let covered = pixels(Filled(&circle).rasterize(screen()));
        assert!(area(&covered) > 0.5, "{covered:?}");
    }
}
