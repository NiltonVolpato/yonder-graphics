//! Projective maps of $\mathbb{T}_2$ and $\mathbb{T}_3$ into themselves ($\mathcal{M}_2$ and $\mathcal{M}_3$).
//!
//! Following Jorge Stolfi's definition of projective maps (Section 8, Definition 1):
//! A linear map $M$ induces a projective map $[\!\[M\]\!]$ of oriented projective space $\mathbb{T}_\nu$
//! to itself ($\mathcal{M}_\nu$).
//!
//! Unlike classical unoriented projective groups ($\mathrm{PGL}$) where $M$ and $-M$ are identified,
//! Stolfi's oriented projective maps distinguish universe orientations:
//! - An inducing matrix with $\det(M) > 0$ is **orientation-preserving** ($\mathbb{T}_\nu \to \mathbb{T}_\nu$).
//! - An inducing matrix with $\det(M) < 0$ is **orientation-reversing** ($\mathbb{T}_\nu \to \neg\mathbb{T}_\nu$).
//!
//! All matrix compositions (`multiply`) are implemented as `const fn` to allow compile-time
//! evaluation, constant folding, and complete dead-code elimination of zero terms by LLVM.

use crate::coord::{Coord3, Coord4};
use crate::gcd::normalize_triple_i64_to_i32;

/// A $3 \times 3$ integer matrix inducing a projective map of $\mathbb{T}_2$ to itself ($\mathcal{M}_2$).
///
/// Used for planar transformations (translation, rotation, scaling, and 2D projective warps).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Matrix3x3 {
    pub m: [[i32; 3]; 3],
}

impl Matrix3x3 {
    /// Creates a matrix from 2D row-major array.
    #[inline(always)]
    pub const fn from_rows(rows: [[i32; 3]; 3]) -> Self {
        Self { m: rows }
    }

    /// The $3 \times 3$ identity projective map.
    pub const IDENTITY: Self = Self::from_rows([[1, 0, 0], [0, 1, 0], [0, 0, 1]]);

    /// Creates an integer translation projective map for 2D offset `(tx, ty)`.
    #[inline]
    pub const fn translation(tx: i32, ty: i32) -> Self {
        Self::from_rows([[1, 0, tx], [0, 1, ty], [0, 0, 1]])
    }

    /// Creates an integer scaling projective map `(sx, sy)` with projective weight divisor `sw`.
    #[inline]
    pub const fn scale(sx: i32, sy: i32, sw: i32) -> Self {
        Self::from_rows([[sx, 0, 0], [0, sy, 0], [0, 0, sw]])
    }

    /// Evaluates the determinant of the inducing matrix.
    #[inline]
    pub const fn determinant(&self) -> i64 {
        let m = self.m;
        let a = m[0][0] as i64;
        let b = m[0][1] as i64;
        let c = m[0][2] as i64;
        let d = m[1][0] as i64;
        let e = m[1][1] as i64;
        let f = m[1][2] as i64;
        let g = m[2][0] as i64;
        let h = m[2][1] as i64;
        let k = m[2][2] as i64;

        a * (e * k - f * h) - b * (d * k - f * g) + c * (d * h - e * g)
    }

    /// Returns true if this map is orientation-preserving ($\det(M) > 0$).
    #[inline(always)]
    pub const fn is_orientation_preserving(&self) -> bool {
        self.determinant() > 0
    }

    /// Returns true if this map is orientation-reversing ($\det(M) < 0$).
    #[inline(always)]
    pub const fn is_orientation_reversing(&self) -> bool {
        self.determinant() < 0
    }

    /// Multiplies this matrix by a homogeneous coordinate triple `Coord3`.
    ///
    /// Evaluates $[x', y', w']^T = M \cdot [x, y, w]^T$ with 64-bit precision
    /// and normalizes the resulting coordinates to contain bit growth.
    #[inline]
    pub fn transform_coord(&self, p: Coord3) -> Coord3 {
        let x = p.x() as i64;
        let y = p.y() as i64;
        let w = p.w() as i64;

        let rx = self.m[0][0] as i64 * x + self.m[0][1] as i64 * y + self.m[0][2] as i64 * w;
        let ry = self.m[1][0] as i64 * x + self.m[1][1] as i64 * y + self.m[1][2] as i64 * w;
        let rw = self.m[2][0] as i64 * x + self.m[2][1] as i64 * y + self.m[2][2] as i64 * w;

        let (nx, ny, nw) = normalize_triple_i64_to_i32(rx, ry, rw);
        Coord3::new(nx, ny, nw)
    }

    /// Composes two projective maps: `self * other`.
    ///
    /// Marked as `const fn` to permit compile-time composition and constant folding.
    #[inline]
    pub const fn multiply(&self, other: &Self) -> Self {
        let mut result = [[0i32; 3]; 3];
        let mut r = 0;
        while r < 3 {
            let mut c = 0;
            while c < 3 {
                let mut sum = 0i64;
                let mut k = 0;
                while k < 3 {
                    sum += self.m[r][k] as i64 * other.m[k][c] as i64;
                    k += 1;
                }
                result[r][c] = sum as i32;
                c += 1;
            }
            r += 1;
        }
        Self::from_rows(result)
    }
}

/// A $4 \times 4$ integer matrix inducing a projective map of $\mathbb{T}_3$ to itself ($\mathcal{M}_3$).
///
/// Handles all 3D affine and projective transformations (Model, View, and Perspective Projection).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Matrix4x4 {
    pub m: [[i32; 4]; 4],
}

impl Matrix4x4 {
    /// Creates a 4x4 matrix from row-major array.
    #[inline(always)]
    pub const fn from_rows(rows: [[i32; 4]; 4]) -> Self {
        Self { m: rows }
    }

    /// The $4 \times 4$ identity projective map.
    pub const IDENTITY: Self =
        Self::from_rows([[1, 0, 0, 0], [0, 1, 0, 0], [0, 0, 1, 0], [0, 0, 0, 1]]);

    /// Creates an integer 3D translation projective map for offset `(tx, ty, tz)`.
    #[inline]
    pub const fn translation(tx: i32, ty: i32, tz: i32) -> Self {
        Self::from_rows([[1, 0, 0, tx], [0, 1, 0, ty], [0, 0, 1, tz], [0, 0, 0, 1]])
    }

    /// Creates an integer 3D scaling projective map with divisor `sw`.
    #[inline]
    pub const fn scale(sx: i32, sy: i32, sz: i32, sw: i32) -> Self {
        Self::from_rows([[sx, 0, 0, 0], [0, sy, 0, 0], [0, 0, sz, 0], [0, 0, 0, sw]])
    }

    /// Creates a standard 3D pinhole perspective projection map with focal length `f`,
    /// optical center `(cx, cy)`, and depth clipping range `[z_near, z_far]`.
    ///
    /// Preserves monotonic depth $Z'$ in the 3rd row for division-free painter's sorting:
    /// $$X' = f \cdot X + c_x \cdot Z$$
    /// $$Y' = f \cdot Y + c_y \cdot Z$$
    /// $$Z' = (z_{far} + z_{near}) \cdot Z - 2 \cdot z_{far} \cdot z_{near} \cdot W$$
    /// $$W' = Z$$
    #[inline]
    pub const fn perspective(f: i32, cx: i32, cy: i32, z_near: i32, z_far: i32) -> Self {
        let a = z_far + z_near;
        let b = -2 * z_far * z_near;
        Self::from_rows([[f, 0, cx, 0], [0, f, cy, 0], [0, 0, a, b], [0, 0, 1, 0]])
    }

    /// Composes two 3D projective maps: `self * other`.
    ///
    /// Marked as `const fn` to permit compile-time composition of Model, View, and Projection matrices.
    #[inline]
    pub const fn multiply(&self, other: &Self) -> Self {
        let mut result = [[0i32; 4]; 4];
        let mut r = 0;
        while r < 4 {
            let mut c = 0;
            while c < 4 {
                let mut sum = 0i64;
                let mut k = 0;
                while k < 4 {
                    sum += self.m[r][k] as i64 * other.m[k][c] as i64;
                    k += 1;
                }
                result[r][c] = sum as i32;
                c += 1;
            }
            r += 1;
        }
        Self::from_rows(result)
    }

    /// Multiplies this matrix by a 3D homogeneous coordinate quad `Coord4`.
    #[inline]
    pub fn transform_coord(&self, p: Coord4) -> Coord4 {
        let x = p.x() as i64;
        let y = p.y() as i64;
        let z = p.z() as i64;
        let w = p.w() as i64;

        let rx = self.m[0][0] as i64 * x
            + self.m[0][1] as i64 * y
            + self.m[0][2] as i64 * z
            + self.m[0][3] as i64 * w;
        let ry = self.m[1][0] as i64 * x
            + self.m[1][1] as i64 * y
            + self.m[1][2] as i64 * z
            + self.m[1][3] as i64 * w;
        let rz = self.m[2][0] as i64 * x
            + self.m[2][1] as i64 * y
            + self.m[2][2] as i64 * z
            + self.m[2][3] as i64 * w;
        let rw = self.m[3][0] as i64 * x
            + self.m[3][1] as i64 * y
            + self.m[3][2] as i64 * z
            + self.m[3][3] as i64 * w;

        let mut transformed = Coord4::new(rx as i32, ry as i32, rz as i32, rw as i32);
        transformed.normalize();
        transformed
    }

    /// Transforms a 3D homogeneous coordinate `Coord4` and projects it to a 2D screen `Coord3`,
    /// returning the screen coordinates and the transformed 3D clip coordinate `Coord4`
    /// containing the exact depth $Z'$ and weight $W'$ for depth comparison.
    ///
    /// Returns `(screen_coord3, clip_coord4)`.
    #[inline]
    pub fn project_to_screen(&self, p: Coord4) -> (Coord3, Coord4) {
        let transformed = self.transform_coord(p);
        let screen = Coord3::new(transformed.x(), transformed.y(), transformed.w()).normalized();
        (screen, transformed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matrix3x3_const_multiply_and_determinant() {
        const T1: Matrix3x3 = Matrix3x3::translation(10, -5);
        const T2: Matrix3x3 = Matrix3x3::scale(2, 2, 1);
        const COMPOSED: Matrix3x3 = T1.multiply(&T2);

        assert!(COMPOSED.is_orientation_preserving());
        assert_eq!(COMPOSED.determinant(), 4);

        let pt = Coord3::point(5, 5);
        let moved = COMPOSED.transform_coord(pt);
        // T2 scales (5, 5) -> (10, 10), T1 translates -> (20, 5)
        assert_eq!(moved.to_euclidean(), Some((20, 5)));
    }

    #[test]
    fn test_matrix3x3_reversing_and_identity() {
        assert_eq!(Matrix3x3::IDENTITY.determinant(), 1);
        let reflect = Matrix3x3::scale(-1, 1, 1);
        assert!(reflect.is_orientation_reversing());
        assert!(!reflect.is_orientation_preserving());
        assert_eq!(reflect.determinant(), -1);
    }

    #[test]
    fn test_matrix4x4_operations() {
        let identity = Matrix4x4::IDENTITY;
        let pt = Coord4::point(1, 2, 3);
        assert_eq!(identity.transform_coord(pt), pt);

        let t = Matrix4x4::translation(10, 20, 30);
        let s = Matrix4x4::scale(2, 2, 2, 1);
        let composed = t.multiply(&s);

        let transformed = composed.transform_coord(pt);
        // Scale (1, 2, 3) -> (2, 4, 6), Translate -> (12, 24, 36)
        assert_eq!(transformed.x(), 12);
        assert_eq!(transformed.y(), 24);
        assert_eq!(transformed.z(), 36);
        assert_eq!(transformed.w(), 1);
    }

    #[test]
    fn test_matrix4x4_perspective_projection_and_depth() {
        const PROJ: Matrix4x4 = Matrix4x4::perspective(200, 160, 120, 10, 1000);

        let v_near = Coord4::point(100, 0, 100);
        let v_far = Coord4::point(100, 0, 400);

        let (screen_near, clip_near) = PROJ.project_to_screen(v_near);
        let (screen_far, clip_far) = PROJ.project_to_screen(v_far);

        // Division-free depth comparison on clip coordinates
        assert_eq!(
            clip_near.compare_depth(&clip_far),
            core::cmp::Ordering::Less
        );
        assert_eq!(
            clip_far.compare_depth(&clip_near),
            core::cmp::Ordering::Greater
        );

        let (x_near, _) = screen_near.to_euclidean().unwrap();
        let (x_far, _) = screen_far.to_euclidean().unwrap();

        assert_eq!(x_near, 360);
        assert_eq!(x_far, 210);
    }
}
