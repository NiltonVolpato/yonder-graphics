//! Circles by marching squares: a pixel's coverage is the area of the polygon cut from
//! it where the circle's implicit function crosses zero on its four edges, taking the
//! function as linear along each edge. Only cells near the edge are visited.

use yonder_geometry::screen::{BoundingBox, Point};
use yonder_geometry::shapes::Circle;

use crate::{Coverage, Filled, Rasterizable, Stroked};

impl Rasterizable for Filled<'_, Circle> {
    fn rasterize(&self, clip: BoundingBox) -> impl Iterator<Item = Coverage> {
        rasterize(Disk::of(self.0, 0), None, clip)
    }
}

impl Rasterizable for Stroked<'_, Circle> {
    fn rasterize(&self, clip: BoundingBox) -> impl Iterator<Item = Coverage> {
        let width = i64::from(self.width);
        let inner = Disk::of(self.shape, -width);
        rasterize(
            Disk::of(self.shape, width),
            (inner.r > 0).then_some(inner),
            clip,
        )
    }
}

/// `q(p) = (w·px − x·pw)² + (w·py − y·pw)² − r²·pw²`, which is ≤ 0 inside.
/// Center `(x / w, y / w)`, radius `r / w`, `w > 0`.
#[derive(Clone, Copy)]
struct Disk {
    x: i64,
    y: i64,
    w: i64,
    r: i64,
}

impl Disk {
    /// The circle grown by `half_widths / 2` pixels. Coordinates are doubled when needed so
    /// half-pixel radii stay integers.
    fn of(circle: &Circle, half_widths: i64) -> Self {
        let [x, y, w, r] = circle.homogeneous().map(i64::from);
        if half_widths == 0 {
            Self { x, y, w, r }
        } else {
            Self {
                x: 2 * x,
                y: 2 * y,
                w: 2 * w,
                r: 2 * r + half_widths * w,
            }
        }
    }

    fn q(&self, px: i64, py: i64, pw: i64) -> i64 {
        let (u, v) = (self.w * px - self.x * pw, self.w * py - self.y * pw);
        u * u + v * v - self.r * self.r * pw * pw
    }

    /// Rows the disk reaches into, half-open.
    fn rows(&self) -> (i64, i64) {
        (
            (self.y - self.r).div_euclid(self.w),
            ceil_div(self.y + self.r, self.w),
        )
    }

    fn reaches_row(&self, row: i64) -> bool {
        self.y - self.r < self.w * (row + 1) && self.y + self.r > self.w * row
    }

    fn center_in_row(&self, row: i64) -> bool {
        self.w * row < self.y && self.y < self.w * (row + 1)
    }
}

/// Tracks the lattice points inside the disk on consecutive lattice lines. Each line
/// starts from the previous one, so the walk costs O(radius) in total.
struct Chords {
    disk: Disk,
    l: i64,
    r: i64,
}

impl Chords {
    /// Starts on `line` with the one square root per shape.
    fn new(disk: Disk, line: i64) -> Self {
        let v = disk.w * line - disk.y;
        let s = (disk.r * disk.r - v * v).max(0).unsigned_abs().isqrt() as i64;
        Self {
            disk,
            l: ceil_div(disk.x - s, disk.w),
            r: (disk.x + s).div_euclid(disk.w),
        }
    }

    /// The inside points `l..=r` on `line`, empty when `l > r`. Lines go in order.
    fn at(&mut self, line: i64) -> (i64, i64) {
        let Disk { x, w, .. } = self.disk;
        let v = w * line - self.disk.y;
        let d = self.disk.r * self.disk.r - v * v;
        if d < 0 {
            return (1, 0);
        }
        let inside = |px: i64| (w * px - x) * (w * px - x) <= d;
        // Both predicates flip once along the line, so each walk only moves toward the edge.
        let past_left = |px: i64| w * px >= x || inside(px);
        let before_right = |px: i64| w * px <= x || inside(px);
        while !past_left(self.l) {
            self.l += 1;
        }
        while past_left(self.l - 1) {
            self.l -= 1;
        }
        while !before_right(self.r) {
            self.r -= 1;
        }
        while before_right(self.r + 1) {
            self.r += 1;
        }
        (self.l, self.r)
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
    let some = |c: (i64, i64)| c.0 <= c.1;
    let (l, r) = match (some(top), some(bottom)) {
        (true, true) => (top.0.min(bottom.0), top.1.max(bottom.1)),
        (true, false) => top,
        (false, true) => bottom,
        (false, false) if disk.reaches_row(row) => {
            // A cap, or a disk smaller than a pixel: narrower than one pixel, so it's in the
            // one or two cells around the center column.
            let (x2, w2) = (2 * disk.x, 2 * disk.w);
            let touched = (
                (x2 - disk.w).div_euclid(w2),
                (x2 + disk.w).div_euclid(w2) + 1,
            );
            return Profile {
                touched,
                full: (0, 0),
            };
        }
        (false, false) => return EMPTY,
    };
    let full = (top.0.max(bottom.0), top.1.min(bottom.1));
    let full = if some(top) && some(bottom) && full.0 < full.1 {
        full
    } else {
        (0, 0)
    };
    // A cell with any inside corner is touched. In the row with the widest point, the
    // edge can also bulge into one more cell on each side.
    let pad = if disk.center_in_row(row) { 2 } else { 1 };
    Profile {
        touched: (l - pad, r + pad),
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
        inner.map(|d| Chords::new(d, rows.start)),
    );
    let mut top = (
        chords.0.at(rows.start),
        chords.1.as_mut().map(|c| c.at(rows.start)),
    );
    rows.flat_map(move |y| {
        let bottom = (chords.0.at(y + 1), chords.1.as_mut().map(|c| c.at(y + 1)));
        let o = profile(&outer, y, top.0, bottom.0);
        let i = match (inner, top.1, bottom.1) {
            (Some(d), Some(t), Some(b)) => Some((d, profile(&d, y, t, b))),
            _ => None,
        };
        top = bottom;
        row(outer, o, i, y, clip_x)
    })
}

/// Splits a row at every range boundary. Within a segment, every cell is in the same
/// ranges, so it is one span, one hole, or a run of edge cells.
fn row(
    outer: Disk,
    o: Profile,
    inner: Option<(Disk, Profile)>,
    y: i64,
    clip_x: (i64, i64),
) -> impl Iterator<Item = Coverage> {
    let i = inner.map_or(EMPTY, |(_, p)| p);
    let (lo, hi) = (
        o.touched.0.max(clip_x.0),
        o.touched.1.min(clip_x.1).max(o.touched.0.max(clip_x.0)),
    );
    let mut cuts = [
        o.touched.0,
        o.touched.1,
        o.full.0,
        o.full.1,
        i.touched.0,
        i.touched.1,
        i.full.0,
        i.full.1,
    ]
    .map(|c| c.clamp(lo, hi));
    cuts.sort_unstable();
    let within = |r: (i64, i64), x: i64| r.0 <= x && x < r.1;
    (0..7usize)
        .filter(move |&k| cuts[k] < cuts[k + 1])
        .flat_map(move |k| {
            let (a, b) = (cuts[k], cuts[k + 1]);
            let (full_o, touched_i) = (within(o.full, a), within(i.touched, a));
            if within(i.full, a) {
                Segment::Done
            } else if full_o && !touched_i {
                Segment::Span(Some(Coverage::Interior(cell_box(a, b, y))))
            } else {
                Segment::Cells {
                    y,
                    x: a,
                    end: b,
                    outer: (!full_o).then(|| Cells::new(outer, a, y)),
                    inner: inner
                        .filter(|_| touched_i)
                        .map(|(d, _)| Cells::new(d, a, y)),
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
                    let area_o = outer.as_mut().map_or(FULL, Cells::area);
                    let area_i = inner.as_mut().map_or(0, Cells::area);
                    let coverage = (((area_o - area_i).max(0) * 255 + FULL / 2) / FULL) as u8;
                    let at = Point::new(px(*x), px(*y));
                    *x += 1;
                    match coverage {
                        0 => {}
                        255 => return Some(Coverage::Interior(cell_box(*x - 1, *x, *y))),
                        c => return Some(Coverage::Boundary(at, c)),
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
    left: [i64; 2],
}

impl Cells {
    fn new(disk: Disk, x: i64, y: i64) -> Self {
        Self {
            disk,
            x,
            y,
            left: [disk.q(x, y, 1), disk.q(x, y + 1, 1)],
        }
    }

    fn area(&mut self) -> i64 {
        let (d, x, y) = (self.disk, self.x, self.y);
        let right = [d.q(x + 1, y, 1), d.q(x + 1, y + 1, 1)];
        let corners = [self.left[0], right[0], right[1], self.left[1]];
        self.left = right;
        self.x += 1;
        if corners.iter().all(|&q| q > 0) {
            // No corner inside: only a cap or a tiny disk reaches here. Look at the four
            // half-pixel cells instead.
            let q = |i: i64, j: i64| d.q(2 * x + i, 2 * y + j, 2);
            let quarter =
                |i: i64, j: i64| polygon_area([q(i, j), q(i + 1, j), q(i + 1, j + 1), q(i, j + 1)]);
            (quarter(0, 0) + quarter(1, 0) + quarter(0, 1) + quarter(1, 1)) / 4
        } else {
            polygon_area(corners)
        }
    }
}

/// Area, in 1/65536 of the cell, where `q ≤ 0`, with `q` linear along each edge.
/// `q` is given at the corners top-left, top-right, bottom-right, bottom-left.
fn polygon_area(q: [i64; 4]) -> i64 {
    const S: i64 = 256;
    const CORNERS: [(i64, i64); 4] = [(0, 0), (S, 0), (S, S), (0, S)];
    let mut points = [(0, 0); 8];
    let mut n = 0;
    for i in 0..4 {
        let j = (i + 1) % 4;
        if q[i] <= 0 {
            points[n] = CORNERS[i];
            n += 1;
        }
        if (q[i] <= 0) != (q[j] <= 0) {
            let t = round_div(q[i] * S, q[i] - q[j]);
            let ((x0, y0), (x1, y1)) = (CORNERS[i], CORNERS[j]);
            points[n] = (x0 + (x1 - x0) * t / S, y0 + (y1 - y0) * t / S);
            n += 1;
        }
    }
    let twice: i64 = (0..n)
        .map(|k| {
            let ((x0, y0), (x1, y1)) = (points[k], points[(k + 1) % n]);
            x0 * y1 - x1 * y0
        })
        .sum();
    twice.abs() / 2
}

/// `a / b` rounded to nearest, ties to even, so mirrored edges round the same way.
fn round_div(a: i64, b: i64) -> i64 {
    let (a, b) = if b < 0 { (-a, -b) } else { (a, b) };
    let (q, r) = (a.div_euclid(b), a.rem_euclid(b));
    if 2 * r > b || (2 * r == b && q % 2 != 0) {
        q + 1
    } else {
        q
    }
}

fn ceil_div(a: i64, b: i64) -> i64 {
    -(-a).div_euclid(b)
}

fn px(v: i64) -> i16 {
    v.clamp(i16::MIN.into(), i16::MAX.into()) as i16
}

fn cell_box(x0: i64, x1: i64, y: i64) -> BoundingBox {
    BoundingBox::new(Point::new(px(x0), px(y)), Point::new(px(x1), px(y + 1)))
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
        let mut put = |x, y, c| assert!(out.insert((x, y), c).is_none(), "pixel ({x}, {y}) twice");
        for item in items {
            match item {
                Coverage::Boundary(p, c) => {
                    assert!(c > 0 && c < 255);
                    put(p.x, p.y, c);
                }
                Coverage::Partial(..) => unreachable!("circles emit no uniform partial areas"),
                Coverage::Interior(b) => {
                    for y in b.top_left.y..b.bottom_right.y {
                        for x in b.top_left.x..b.bottom_right.x {
                            put(x, y, 255);
                        }
                    }
                }
            }
        }
        out
    }

    fn area(p: &HashMap<(i16, i16), u8>) -> f64 {
        p.values().map(|&c| f64::from(c) / 255.0).sum()
    }

    fn circles() -> Vec<(Circle, f64)> {
        let lattice = |x, y, r| (Circle::new(PointT2::new(x, y, 1), r), f64::from(r));
        let centered = |x, y, r| {
            (
                Circle::new(PointT2::pixel_center(Point::new(x, y)), r),
                f64::from(r),
            )
        };
        let boxed = |x0, y0, s: i16| {
            (
                Circle::in_box(BoundingBox::new(
                    Point::new(x0, y0),
                    Point::new(x0 + s, y0 + s),
                )),
                f64::from(s) / 2.0,
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
        for (circle, r) in circles() {
            let p = pixels(Filled(&circle).rasterize(screen()));
            let expected = core::f64::consts::PI * r * r;
            // Linear crossings sit slightly inside the convex edge: about 1 px² short in total,
            // whatever the radius.
            let error = area(&p) - expected;
            assert!((-1.25..0.05).contains(&error), "r = {r}: {error:+.3}");
        }
    }

    #[test]
    fn stroke_area_matches_ring() {
        for (circle, r) in circles() {
            for width in [1u16, 2, 3, 6] {
                let p = pixels(
                    Stroked {
                        shape: &circle,
                        width,
                    }
                    .rasterize(screen()),
                );
                let (outer, inner) = (
                    r + f64::from(width) / 2.0,
                    (r - f64::from(width) / 2.0).max(0.0),
                );
                let expected = core::f64::consts::PI * (outer * outer - inner * inner);
                // The outer and inner edges lose about the same, unless the inner one is tiny.
                let error = area(&p) - expected;
                let range = if inner >= 1.0 {
                    -0.25..0.25
                } else {
                    -1.25..0.25
                };
                assert!(range.contains(&error), "r = {r}, w = {width}: {error:+.3}");
            }
        }
    }

    #[test]
    fn centered_circle_is_symmetric() {
        let circle = Circle::new(PointT2::pixel_center(Point::new(0, 0)), 6);
        let p = pixels(Filled(&circle).rasterize(screen()));
        for (&(x, y), &c) in &p {
            for mirror in [(-x, y), (x, -y), (y, x)] {
                assert_eq!(
                    p.get(&mirror).copied().unwrap_or(0),
                    c,
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
        let p = pixels(Filled(&circle).rasterize(screen()));
        assert!(area(&p) > 0.5, "{p:?}");
    }
}
