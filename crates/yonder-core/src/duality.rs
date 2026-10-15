//! Projective duality in $\mathbb{T}^2$: points and lines are algebraic duals.
//!
//! Joining two points yields a directed line. Meeting two lines yields a point.
//! Parallel lines naturally intersect at an ideal point at infinity ($w = 0$).

use crate::coord::Coord3;
use crate::gcd::normalize_triple_i64_to_i32;

/// A directed line in oriented projective 2-space $\mathbb{T}^2$.
///
/// Represented by coefficients $[a, b, c]$ such that:
/// $$a \cdot x + b \cdot y + c \cdot w = 0$$
///
/// For a finite point with $w = 1$, the directed distance function is $a \cdot x + b \cdot y + c$.
/// Points with value $> 0$ lie to the left of the line's directed heading;
/// points with value $< 0$ lie to the right.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(align(16))]
pub struct Line3 {
    pub v: [i32; 3],
}

impl Line3 {
    /// Creates a directed line with coefficients `[a, b, c]`.
    #[inline(always)]
    pub const fn new(a: i32, b: i32, c: i32) -> Self {
        Self { v: [a, b, c] }
    }

    /// Returns $a$.
    #[inline(always)]
    pub const fn a(&self) -> i32 {
        self.v[0]
    }

    /// Returns $b$.
    #[inline(always)]
    pub const fn b(&self) -> i32 {
        self.v[1]
    }

    /// Returns $c$.
    #[inline(always)]
    pub const fn c(&self) -> i32 {
        self.v[2]
    }

    /// Evaluates the directed distance function for a homogeneous point `p`:
    /// $$a \cdot p_x + b \cdot p_y + c \cdot p_w$$
    #[inline]
    pub fn evaluate_coord(&self, p: Coord3) -> i64 {
        self.v[0] as i64 * p.x() as i64
            + self.v[1] as i64 * p.y() as i64
            + self.v[2] as i64 * p.w() as i64
    }

    /// Evaluates the directed distance function for a finite Euclidean pixel $(x, y)$ ($w = 1$):
    /// $$a \cdot x + b \cdot y + c$$
    #[inline(always)]
    pub fn evaluate_pixel(&self, x: i32, y: i32) -> i64 {
        self.v[0] as i64 * x as i64 + self.v[1] as i64 * y as i64 + self.v[2] as i64
    }

    /// Inverts the direction of the line.
    #[inline(always)]
    pub const fn reversed(self) -> Self {
        Self::new(-self.v[0], -self.v[1], -self.v[2])
    }
}

/// Dual Join operation: connects two points $\mathbf{p}_1$ and $\mathbf{p}_2$
/// with a directed line $\mathbf{l} = \mathbf{p}_1 \times \mathbf{p}_2$.
///
/// The line is directed from $\mathbf{p}_1$ toward $\mathbf{p}_2$.
#[inline]
pub fn join(p1: Coord3, p2: Coord3) -> Line3 {
    let x1 = p1.x() as i64;
    let y1 = p1.y() as i64;
    let w1 = p1.w() as i64;

    let x2 = p2.x() as i64;
    let y2 = p2.y() as i64;
    let w2 = p2.w() as i64;

    let a = y1 * w2 - w1 * y2;
    let b = w1 * x2 - x1 * w2;
    let c = x1 * y2 - y1 * x2;

    let (na, nb, nc) = normalize_triple_i64_to_i32(a, b, c);
    Line3::new(na, nb, nc)
}

/// Dual Meet operation: intersects two directed lines $\mathbf{l}_1$ and $\mathbf{l}_2$
/// to produce their intersection point $\mathbf{p} = \mathbf{l}_1 \times \mathbf{l}_2$.
///
/// If the lines are parallel in Euclidean space, $w$ evaluates to zero, naturally
/// yielding a directed point at infinity with zero special branches or panics.
#[inline]
pub fn meet(l1: Line3, l2: Line3) -> Coord3 {
    let a1 = l1.a() as i64;
    let b1 = l1.b() as i64;
    let c1 = l1.c() as i64;

    let a2 = l2.a() as i64;
    let b2 = l2.b() as i64;
    let c2 = l2.c() as i64;

    let x = b1 * c2 - c1 * b2;
    let y = c1 * a2 - a1 * c2;
    let w = a1 * b2 - b1 * a2;

    let (nx, ny, nw) = normalize_triple_i64_to_i32(x, y, w);
    Coord3::new(nx, ny, nw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_join_and_evaluate() {
        // Line from (0, 0) to (10, 0) -> Directed horizontal line along x-axis
        let p1 = Coord3::point(0, 0);
        let p2 = Coord3::point(10, 0);
        let line = join(p1, p2);

        // Point (5, 5) is above/left of line -> evaluate > 0
        assert!(line.evaluate_pixel(5, 5) > 0);
        // Point (5, -5) is below/right of line -> evaluate < 0
        assert!(line.evaluate_pixel(5, -5) < 0);
        // Point (5, 0) is on line -> evaluate == 0
        assert_eq!(line.evaluate_pixel(5, 0), 0);
    }

    #[test]
    fn test_meet_intersecting_lines() {
        // Horizontal line y = 5
        let l1 = Line3::new(0, 1, -5);
        // Vertical line x = 10
        let l2 = Line3::new(1, 0, -10);

        let pt = meet(l1, l2);
        assert!(pt.is_finite());
        assert_eq!(pt.to_euclidean(), Some((10, 5)));
    }

    #[test]
    fn test_meet_parallel_lines_yields_infinity() {
        // Two horizontal parallel lines: y = 0 and y = 10
        let l1 = Line3::new(0, 1, 0);
        let l2 = Line3::new(0, 1, -10);

        let pt = meet(l1, l2);
        // Parallel lines intersect at infinity (w == 0)
        assert!(pt.is_infinite());
        assert_eq!(pt.w(), 0);
        assert_ne!((pt.x(), pt.y()), (0, 0));
    }

    #[test]
    fn test_line_evaluate_coord_and_reversed() {
        let line = Line3::new(1, -1, 0);
        let p = Coord3::point(5, 5);
        assert_eq!(line.evaluate_coord(p), 0);

        let rev = line.reversed();
        assert_eq!(rev.a(), -1);
        assert_eq!(rev.b(), 1);
        assert_eq!(rev.c(), 0);
    }
}
