//! Exact geometric orientation predicates in oriented projective space $\mathbb{T}^2$.
//!
//! Evaluated with exact 128-bit integer arithmetic to prevent topological
//! inconsistency, cracks between adjacent edges, or floating-point rounding errors.

use core::cmp::Ordering;

use crate::coord::Coord3;

/// Geometric orientation of an ordered triple of points $(\mathbf{p}, \mathbf{q}, \mathbf{r})$
/// in oriented projective space $\mathbb{T}^2$.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Orientation {
    /// Points form a counter-clockwise turn.
    CounterClockwise,
    /// Points form a clockwise turn.
    Clockwise,
    /// Points are collinear (fall on the exact same projective line).
    Collinear,
}

impl Orientation {
    /// Returns true if the turn is counter-clockwise.
    #[inline(always)]
    pub const fn is_ccw(&self) -> bool {
        matches!(self, Self::CounterClockwise)
    }

    /// Returns true if the turn is clockwise.
    #[inline(always)]
    pub const fn is_cw(&self) -> bool {
        matches!(self, Self::Clockwise)
    }

    /// Returns true if the points are collinear.
    #[inline(always)]
    pub const fn is_collinear(&self) -> bool {
        matches!(self, Self::Collinear)
    }
}

/// Computes the exact orientation predicate $\Delta(\mathbf{p}, \mathbf{q}, \mathbf{r})$
/// via the $3 \times 3$ determinant:
///
/// $$\det \begin{bmatrix} p_x & q_x & r_x \\ p_y & q_y & r_y \\ p_w & q_w & r_w \end{bmatrix}$$
///
/// Evaluated with 128-bit integer precision. Guarantees complete topological consistency
/// across shared edges.
#[inline]
pub fn orientation(p: Coord3, q: Coord3, r: Coord3) -> Orientation {
    let px = p.x() as i128;
    let py = p.y() as i128;
    let pw = p.w() as i128;

    let qx = q.x() as i128;
    let qy = q.y() as i128;
    let qw = q.w() as i128;

    let rx = r.x() as i128;
    let ry = r.y() as i128;
    let rw = r.w() as i128;

    let det = px * (qy * rw - qw * ry) - py * (qx * rw - qw * rx) + pw * (qx * ry - qy * rx);

    match det.cmp(&0) {
        Ordering::Greater => Orientation::CounterClockwise,
        Ordering::Less => Orientation::Clockwise,
        Ordering::Equal => Orientation::Collinear,
    }
}

/// Returns the signed 128-bit determinant value for exact calculations.
#[inline]
pub fn orientation_det(p: Coord3, q: Coord3, r: Coord3) -> i128 {
    let px = p.x() as i128;
    let py = p.y() as i128;
    let pw = p.w() as i128;

    let qx = q.x() as i128;
    let qy = q.y() as i128;
    let qw = q.w() as i128;

    let rx = r.x() as i128;
    let ry = r.y() as i128;
    let rw = r.w() as i128;

    px * (qy * rw - qw * ry) - py * (qx * rw - qw * rx) + pw * (qx * ry - qy * rx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orientation_basic() {
        let p = Coord3::point(0, 0);
        let q = Coord3::point(10, 0);
        let r_ccw = Coord3::point(5, 5);
        let r_cw = Coord3::point(5, -5);
        let r_collinear = Coord3::point(20, 0);

        let o_ccw = orientation(p, q, r_ccw);
        let o_cw = orientation(p, q, r_cw);
        let o_col = orientation(p, q, r_collinear);

        assert_eq!(o_ccw, Orientation::CounterClockwise);
        assert!(o_ccw.is_ccw());
        assert!(!o_ccw.is_cw());
        assert!(!o_ccw.is_collinear());

        assert_eq!(o_cw, Orientation::Clockwise);
        assert!(o_cw.is_cw());
        assert!(!o_cw.is_ccw());

        assert_eq!(o_col, Orientation::Collinear);
        assert!(o_col.is_collinear());

        assert!(orientation_det(p, q, r_ccw) > 0);
        assert!(orientation_det(p, q, r_cw) < 0);
        assert_eq!(orientation_det(p, q, r_collinear), 0);
    }
}
