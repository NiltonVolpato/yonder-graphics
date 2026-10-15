//! Unified Conic Sections in oriented projective space $\mathbb{T}^2$.
//!
//! Circles, ellipses, parabolas, and hyperbolas share an exact $3 \times 3$ symmetric
//! matrix representation $\mathbf{C}$. The quadratic inside/outside form is evaluated
//! with 2nd-order forward differencing (addition-only) in the hot raster loop.

use crate::coord::Coord3;
use crate::duality::Line3;
use crate::gcd::normalize_triple_i64_to_i32;

/// A symmetric $3 \times 3$ integer matrix representing a conic section in $\mathbb{T}^2$:
///
/// $$\mathbf{C} = \begin{bmatrix} A & B & D \\ B & C & E \\ D & E & F \end{bmatrix}$$
///
/// A point $\mathbf{p} = [x, y, w]^T$ lies on the conic when $\mathbf{p}^T \mathbf{C} \mathbf{p} = 0$:
/// $$A x^2 + 2B xy + C y^2 + 2D xw + 2E yw + F w^2 = 0$$
///
/// For closed conics (circles, ellipses), points inside the boundary satisfy $\mathbf{p}^T \mathbf{C} \mathbf{p} \le 0$.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ConicMatrix {
    pub a: i64,
    pub b: i64,
    pub c: i64,
    pub d: i64,
    pub e: i64,
    pub f: i64,
}

impl ConicMatrix {
    /// Creates a generic symmetric conic matrix with coefficients:
    /// `a` ($x^2$), `b` ($xy$), `c` ($y^2$), `d` ($xw$), `e` ($yw$), `f` ($w^2$).
    #[inline(always)]
    pub const fn new(a: i64, b: i64, c: i64, d: i64, e: i64, f: i64) -> Self {
        Self { a, b, c, d, e, f }
    }

    /// Creates a circle with integer center $(c_x, c_y)$ and radius $r$:
    /// $$(x - c_x)^2 + (y - c_y)^2 \le r^2$$
    #[inline]
    pub fn circle(cx: i32, cy: i32, r: i32) -> Self {
        let cx = cx as i64;
        let cy = cy as i64;
        let r = r as i64;

        Self {
            a: 1,
            b: 0,
            c: 1,
            d: -cx,
            e: -cy,
            f: cx * cx + cy * cy - r * r,
        }
    }

    /// Creates an axis-aligned ellipse with center $(c_x, c_y)$ and radii $(r_x, r_y)$:
    /// $$\frac{(x - c_x)^2}{r_x^2} + \frac{(y - c_y)^2}{r_y^2} \le 1$$
    #[inline]
    pub fn ellipse(cx: i32, cy: i32, rx: i32, ry: i32) -> Self {
        let cx = cx as i64;
        let cy = cy as i64;
        let rx = rx as i64;
        let ry = ry as i64;

        let rx2 = rx * rx;
        let ry2 = ry * ry;

        Self {
            a: ry2,
            b: 0,
            c: rx2,
            d: -ry2 * cx,
            e: -rx2 * cy,
            f: ry2 * cx * cx + rx2 * cy * cy - rx2 * ry2,
        }
    }

    /// Evaluates the quadratic form $\mathbf{p}^T \mathbf{C} \mathbf{p}$ for a homogeneous point $\mathbf{p}$.
    #[inline]
    pub fn evaluate_coord(&self, p: Coord3) -> i64 {
        let x = p.x() as i64;
        let y = p.y() as i64;
        let w = p.w() as i64;

        self.a * x * x
            + 2 * self.b * x * y
            + self.c * y * y
            + 2 * self.d * x * w
            + 2 * self.e * y * w
            + self.f * w * w
    }

    /// Evaluates the quadratic form for a finite Euclidean pixel $(x, y)$ with $w = 1$.
    #[inline(always)]
    pub fn evaluate_pixel(&self, x: i32, y: i32) -> i64 {
        let x = x as i64;
        let y = y as i64;

        self.a * x * x
            + 2 * self.b * x * y
            + self.c * y * y
            + 2 * self.d * x
            + 2 * self.e * y
            + self.f
    }

    /// Returns true if the finite Euclidean point $(x, y)$ is inside or on the conic boundary.
    #[inline(always)]
    pub fn is_inside_pixel(&self, x: i32, y: i32) -> bool {
        self.evaluate_pixel(x, y) <= 0
    }

    /// Computes the polar/tangent line of point $\mathbf{p}$ with respect to the conic:
    /// $$\mathbf{l} = \mathbf{C} \mathbf{p}$$
    ///
    /// If $\mathbf{p}$ is on the conic boundary, $\mathbf{l}$ is the exact tangent line at $\mathbf{p}$.
    #[inline]
    pub fn tangent_at(&self, p: Coord3) -> Line3 {
        let x = p.x() as i64;
        let y = p.y() as i64;
        let w = p.w() as i64;

        let la = self.a * x + self.b * y + self.d * w;
        let lb = self.b * x + self.c * y + self.e * w;
        let lc = self.d * x + self.e * y + self.f * w;

        let (na, nb, nc) = normalize_triple_i64_to_i32(la, lb, lc);
        Line3::new(na, nb, nc)
    }

    /// Creates an incremental 2nd-order forward difference stepper for scanline $y$, starting at $x_0$.
    #[inline]
    pub fn scanline_stepper(&self, x0: i32, y: i32) -> ConicScanlineStepper {
        ConicScanlineStepper::new(self, x0, y)
    }
}

/// An incremental 2nd-order forward differencing stepper along a horizontal scanline.
///
/// Evaluates the quadratic polynomial $Q(x, y)$ with **two integer additions per pixel**
/// in the hot loop, completely eliminating multiplications and square roots:
/// $$Q(x + 1, y) = Q(x, y) + \Delta Q_x$$
/// $$\Delta Q_x(x + 1) = \Delta Q_x(x) + 2A$$
#[derive(Copy, Clone, Debug)]
pub struct ConicScanlineStepper {
    pub current_q: i64,
    pub delta_q: i64,
    pub delta2_q: i64,
}

impl ConicScanlineStepper {
    /// Initializes the stepper for scanline `y` at starting position `x0`.
    #[inline]
    pub fn new(conic: &ConicMatrix, x0: i32, y: i32) -> Self {
        let x = x0 as i64;
        let y = y as i64;

        // Q(x, y) = A x^2 + 2B xy + C y^2 + 2D x + 2E y + F
        let current_q = conic.a * x * x
            + 2 * conic.b * x * y
            + conic.c * y * y
            + 2 * conic.d * x
            + 2 * conic.e * y
            + conic.f;

        // First difference: Q(x+1, y) - Q(x, y) = 2A x + A + 2B y + 2D
        let delta_q = 2 * conic.a * x + conic.a + 2 * conic.b * y + 2 * conic.d;

        // Second difference: Delta Q(x+1) - Delta Q(x) = 2A
        let delta2_q = 2 * conic.a;

        Self {
            current_q,
            delta_q,
            delta2_q,
        }
    }

    /// Steps one pixel to the right and returns whether the previous position was inside ($Q \le 0$).
    #[inline(always)]
    pub fn step_inside(&mut self) -> bool {
        let inside = self.current_q <= 0;
        self.current_q += self.delta_q;
        self.delta_q += self.delta2_q;
        inside
    }

    /// Returns whether the current pixel is inside the conic boundary ($Q \le 0$).
    #[inline(always)]
    pub const fn is_inside(&self) -> bool {
        self.current_q <= 0
    }

    /// Advances the stepper by one pixel to the right ($x \to x + 1$).
    #[inline(always)]
    pub fn advance(&mut self) {
        self.current_q += self.delta_q;
        self.delta_q += self.delta2_q;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circle_evaluation() {
        // Circle centered at (10, 10) with radius 5
        let circle = ConicMatrix::circle(10, 10, 5);

        // Center is strictly inside (< 0)
        assert!(circle.evaluate_pixel(10, 10) < 0);
        assert!(circle.is_inside_pixel(10, 10));

        // Perimeter points are on boundary (== 0)
        assert_eq!(circle.evaluate_pixel(15, 10), 0);
        assert_eq!(circle.evaluate_pixel(5, 10), 0);
        assert_eq!(circle.evaluate_pixel(10, 15), 0);
        assert_eq!(circle.evaluate_pixel(10, 5), 0);
        assert!(circle.is_inside_pixel(15, 10));

        // External points are outside (> 0)
        assert!(circle.evaluate_pixel(16, 10) > 0);
        assert!(!circle.is_inside_pixel(16, 10));
    }

    #[test]
    fn test_conic_forward_differencing() {
        let circle = ConicMatrix::circle(10, 10, 5);
        let mut stepper = circle.scanline_stepper(0, 10);

        for x in 0..=20 {
            let direct = circle.evaluate_pixel(x, 10);
            assert_eq!(stepper.current_q, direct, "Mismatch at x = {x}");
            assert_eq!(stepper.is_inside(), direct <= 0);
            stepper.advance();
        }
    }

    #[test]
    fn test_conic_tangent() {
        let circle = ConicMatrix::circle(0, 0, 10);
        // Tangent at point (10, 0) should be vertical line x = 10
        let pt = Coord3::point(10, 0);
        let tangent = circle.tangent_at(pt);

        // Vertical line x = 10 has a != 0, b == 0
        assert_ne!(tangent.a(), 0);
        assert_eq!(tangent.b(), 0);
        // Evaluating on tangent line: point (10, 5) should have distance 0
        assert_eq!(tangent.evaluate_pixel(10, 5), 0);
    }

    #[test]
    fn test_conic_evaluate_coord_and_step_inside() {
        let conic = ConicMatrix::new(1, 0, 1, 0, 0, -100);
        let pt = Coord3::point(6, 8);
        assert_eq!(conic.evaluate_coord(pt), 0); // 6^2 + 8^2 - 100 = 0

        let mut stepper = conic.scanline_stepper(0, 0);
        assert!(stepper.step_inside()); // (0, 0) is inside (q = -100)
    }
}
