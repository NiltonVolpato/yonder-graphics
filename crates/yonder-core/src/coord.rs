//! Homogeneous coordinates in oriented projective spaces $\mathbb{T}^2$ and $\mathbb{T}^3$.
//!
//! Based on Jorge Stolfi's spherical model of oriented projective geometry.
//! Unlike standard real projective spaces $\mathbb{P}^2$ and $\mathbb{P}^3$ which identify
//! $(x, y, w) \equiv (-x, -y, -w)$, oriented projective space preserves signs,
//! treating the two antipodal directions as distinct front and rear universes.

use core::cmp::Ordering;
use crate::gcd::{normalize_quad_i32, normalize_triple_i32};

/// A homogeneous coordinate triple `[x, y, w]` in oriented projective 2-space $\mathbb{T}^2$.
///
/// Invariants:
/// - $w > 0$: Represents a finite point at Euclidean position $(x/w, y/w)$ in the front universe.
/// - $w < 0$: Represents an antipodal point in the rear universe.
/// - $w = 0$: Represents a directed point at infinity in the direction of vector $(x, y)$.
/// - $x = y = w = 0$: Undefined singularity.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(align(16))]
pub struct Coord3 {
    pub v: [i32; 3],
}

impl Coord3 {
    /// Creates a new homogeneous triple `[x, y, w]`.
    #[inline(always)]
    pub const fn new(x: i32, y: i32, w: i32) -> Self {
        Self { v: [x, y, w] }
    }

    /// Creates a finite Euclidean point with weight $w = 1$.
    #[inline(always)]
    pub const fn point(x: i32, y: i32) -> Self {
        Self::new(x, y, 1)
    }

    /// Creates a finite subpixel point with explicit fractional scale $w = \text{scale}$.
    #[inline(always)]
    pub const fn subpixel(x: i32, y: i32, scale: i32) -> Self {
        Self::new(x, y, scale)
    }

    /// Creates a directed point at infinity ($w = 0$) along direction `(dx, dy)`.
    #[inline(always)]
    pub const fn infinity(dx: i32, dy: i32) -> Self {
        Self::new(dx, dy, 0)
    }

    /// Returns the $x$ coordinate.
    #[inline(always)]
    pub const fn x(&self) -> i32 {
        self.v[0]
    }

    /// Returns the $y$ coordinate.
    #[inline(always)]
    pub const fn y(&self) -> i32 {
        self.v[1]
    }

    /// Returns the homogeneous weight $w$.
    #[inline(always)]
    pub const fn w(&self) -> i32 {
        self.v[2]
    }

    /// Returns true if this point is finite (not at infinity, i.e. $w \ne 0$).
    #[inline(always)]
    pub const fn is_finite(&self) -> bool {
        self.v[2] != 0
    }

    /// Returns true if this point is in the front universe ($w > 0$).
    #[inline(always)]
    pub const fn is_front(&self) -> bool {
        self.v[2] > 0
    }

    /// Returns true if this point is at infinity on the horizon ($w = 0$).
    #[inline(always)]
    pub const fn is_infinite(&self) -> bool {
        self.v[2] == 0
    }

    /// Returns true if this point is antipodal in the rear universe ($w < 0$).
    #[inline(always)]
    pub const fn is_antipodal(&self) -> bool {
        self.v[2] < 0
    }

    /// Returns the canonical front-universe representation with $w \ge 0$.
    ///
    /// If $w < 0$, returns the antipodal point $(-x, -y, -w)$.
    #[inline(always)]
    pub const fn to_front(self) -> Self {
        if self.v[2] < 0 {
            self.antipodal()
        } else {
            self
        }
    }

    /// Normalizes the coordinates in-place by dividing out their greatest common positive divisor.
    ///
    /// Preserves orientation while minimizing coordinate bit width.
    #[inline]
    pub fn normalize(&mut self) -> &mut Self {
        normalize_triple_i32(&mut self.v);
        self
    }

    /// Returns a normalized copy with common positive factors divided out.
    #[inline]
    pub fn normalized(mut self) -> Self {
        self.normalize();
        self
    }

    /// Returns the antipodal point $(-x, -y, -w)$.
    ///
    /// Note: While the Euclidean projection $(x/w, y/w)$ is identical, the orientation
    /// is inverted from the front universe to the rear universe.
    #[inline(always)]
    pub const fn antipodal(self) -> Self {
        Self::new(-self.v[0], -self.v[1], -self.v[2])
    }

    /// Converts to Euclidean integer coordinates $(x/w, y/w)$ by truncation.
    /// Returns `None` if the point is at infinity ($w = 0$).
    #[inline]
    pub fn to_euclidean(&self) -> Option<(i32, i32)> {
        if self.v[2] == 0 {
            None
        } else {
            Some((self.v[0] / self.v[2], self.v[1] / self.v[2]))
        }
    }

    /// Converts to scaled Euclidean integer coordinates with subpixel precision.
    ///
    /// Computes $(x \cdot \text{scale} / w, y \cdot \text{scale} / w)$.
    /// Returns `None` if $w = 0$.
    #[inline]
    pub fn to_euclidean_scaled(&self, scale: i32) -> Option<(i32, i32)> {
        if self.v[2] == 0 {
            None
        } else {
            let num_x = self.v[0] as i64 * scale as i64;
            let num_y = self.v[1] as i64 * scale as i64;
            let den = self.v[2] as i64;
            Some(((num_x / den) as i32, (num_y / den) as i32))
        }
    }
}

/// A homogeneous coordinate quad `[X, Y, Z, W]` in oriented projective 3-space $\mathbb{T}^3$.
///
/// Designed with a 16-byte alignment to map directly into 128-bit SIMD vector registers
/// (ARM NEON, x86 SSE/AVX, RISC-V Vector).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(align(16))]
pub struct Coord4 {
    pub v: [i32; 4],
}

impl Coord4 {
    /// Creates a new homogeneous quad `[X, Y, Z, W]`.
    #[inline(always)]
    pub const fn new(x: i32, y: i32, z: i32, w: i32) -> Self {
        Self { v: [x, y, z, w] }
    }

    /// Creates a finite 3D Euclidean point with $W = 1$.
    #[inline(always)]
    pub const fn point(x: i32, y: i32, z: i32) -> Self {
        Self::new(x, y, z, 1)
    }

    /// Returns $X$.
    #[inline(always)]
    pub const fn x(&self) -> i32 {
        self.v[0]
    }

    /// Returns $Y$.
    #[inline(always)]
    pub const fn y(&self) -> i32 {
        self.v[1]
    }

    /// Returns $Z$.
    #[inline(always)]
    pub const fn z(&self) -> i32 {
        self.v[2]
    }

    /// Returns the projective weight $W$.
    #[inline(always)]
    pub const fn w(&self) -> i32 {
        self.v[3]
    }

    /// Normalizes coordinates in-place by dividing out their greatest common positive divisor.
    #[inline]
    pub fn normalize(&mut self) -> &mut Self {
        normalize_quad_i32(&mut self.v);
        self
    }

    /// Returns a normalized copy.
    #[inline]
    pub fn normalized(mut self) -> Self {
        self.normalize();
        self
    }

    /// Compares projective depths without division for painter's algorithm sorting.
    ///
    /// Evaluates $Z_A / W_A > Z_B / W_B$ via cross-multiplication:
    /// $(Z_A \cdot W_B) \text{ vs } (Z_B \cdot W_A)$ with sign correction for negative $W$.
    #[inline]
    pub fn compare_depth(&self, other: &Self) -> Ordering {
        let za = self.v[2] as i64;
        let wa = self.v[3] as i64;
        let zb = other.v[2] as i64;
        let wb = other.v[3] as i64;

        let left = za * wb;
        let right = zb * wa;

        // If either weight is negative, invert relationship to preserve oriented depth
        let sign = wa.signum() * wb.signum();
        if sign < 0 {
            right.cmp(&left)
        } else {
            left.cmp(&right)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coord3_properties() {
        let p = Coord3::point(10, 20);
        assert!(p.is_finite());
        assert!(!p.is_infinite());
        assert!(!p.is_antipodal());
        assert_eq!(p.to_euclidean(), Some((10, 20)));

        let inf = Coord3::infinity(1, 0);
        assert!(inf.is_infinite());
        assert_eq!(inf.to_euclidean(), None);

        let antipodal = p.antipodal();
        assert!(antipodal.is_antipodal());
        assert_eq!(antipodal.to_euclidean(), Some((10, 20)));
    }

    #[test]
    fn test_coord3_subpixel_and_scaled() {
        let p = Coord3::subpixel(15, 25, 2);
        assert_eq!(p.to_euclidean(), Some((7, 12)));
        assert_eq!(p.to_euclidean_scaled(4), Some((30, 50)));

        let inf = Coord3::infinity(1, 1);
        assert_eq!(inf.to_euclidean_scaled(4), None);
    }

    #[test]
    fn test_coord4_depth_compare() {
        // Point A: Z = 10, W = 1 (Depth 10)
        // Point B: Z = 20, W = 1 (Depth 20)
        let a = Coord4::point(1, 2, 10);
        assert_eq!(a.x(), 1);
        assert_eq!(a.y(), 2);
        assert_eq!(a.z(), 10);
        assert_eq!(a.w(), 1);

        let b = Coord4::point(0, 0, 20);
        assert_eq!(a.compare_depth(&b), Ordering::Less);
        assert_eq!(b.compare_depth(&a), Ordering::Greater);

        // Point C: Z = 30, W = 2 (Depth 15)
        let c = Coord4::new(0, 0, 30, 2);
        assert_eq!(a.compare_depth(&c), Ordering::Less); // 10 < 15
        assert_eq!(b.compare_depth(&c), Ordering::Greater); // 20 > 15

        // Negative W (rear universe orientation check)
        let d = Coord4::new(0, 0, 10, -1); // depth -10
        assert_eq!(d.compare_depth(&a), Ordering::Less);

        // Normalized copy
        let norm_c = c.normalized();
        assert_eq!(norm_c.w(), 1); // gcd(0, 0, 30, 2) is 2 -> w becomes 1
        assert_eq!(norm_c.z(), 15);
    }
}
