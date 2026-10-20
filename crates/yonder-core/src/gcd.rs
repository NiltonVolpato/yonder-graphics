//! Binary GCD implementations adapted for embedded `#![no_std]`.
//!
//! Based on Stein's algorithm with early termination optimizations and
//! Lehmer-style hybrid division when quotient bit-lengths diverge.

/// Computes the greatest common divisor of two unsigned 64-bit integers
/// using iterative binary GCD (Stein's algorithm).
///
/// Features an early exit optimization when the running minimum reaches 1
/// (`gcd(k, 1) == 1`), avoiding unnecessary loop iterations.
#[inline]
pub fn gcd_binary_u64(mut a: u64, mut b: u64) -> u64 {
    if a == 0 {
        return b;
    }
    if b == 0 {
        return a;
    }

    let mut az = a.trailing_zeros();
    let bz = b.trailing_zeros();
    let shift = az.min(bz);
    b >>= bz;

    loop {
        a >>= az;
        if a == b {
            break;
        }
        let diff = a.abs_diff(b);
        az = diff.trailing_zeros();
        b = a.min(b);
        if b == 1 {
            break;
        }
        a = diff;
    }
    b << shift
}

/// Computes the greatest common divisor of two unsigned 32-bit integers
/// using iterative binary GCD (Stein's algorithm).
#[inline]
pub fn gcd_binary_u32(mut a: u32, mut b: u32) -> u32 {
    if a == 0 {
        return b;
    }
    if b == 0 {
        return a;
    }

    let mut az = a.trailing_zeros();
    let bz = b.trailing_zeros();
    let shift = az.min(bz);
    b >>= bz;

    loop {
        a >>= az;
        if a == b {
            break;
        }
        let diff = a.abs_diff(b);
        az = diff.trailing_zeros();
        b = a.min(b);
        if b == 1 {
            break;
        }
        a = diff;
    }
    b << shift
}

/// Normalizes a triple of signed 32-bit integers by their greatest common positive divisor.
///
/// In oriented projective space $\mathbb{T}^2$, scaling by any positive scalar
/// preserves the exact oriented point or line. Negating or dividing by a negative
/// scalar would flip orientation (antipodal swap), so only positive divisors are factored out.
///
/// Performs an initial O(1) common trailing zeros shift, followed by binary GCD.
#[inline]
pub fn normalize_triple_i32(v: &mut [i32; 3]) {
    let ux = v[0].unsigned_abs();
    let uy = v[1].unsigned_abs();
    let uw = v[2].unsigned_abs();

    let combined = ux | uy | uw;
    if combined == 0 {
        return;
    }

    // Step 1: O(1) trailing zeros elimination
    let shift = combined.trailing_zeros();
    if shift > 0 {
        v[0] >>= shift;
        v[1] >>= shift;
        v[2] >>= shift;
    }

    // Step 2: Binary GCD on remaining values
    let g1 = gcd_binary_u32(v[0].unsigned_abs(), v[1].unsigned_abs());
    if g1 == 1 {
        // Early exit: gcd(1, w) is guaranteed to be 1.
        return;
    }

    let g = gcd_binary_u32(g1, v[2].unsigned_abs());
    if g > 1 {
        v[0] /= g as i32;
        v[1] /= g as i32;
        v[2] /= g as i32;
    }
}

/// Normalizes a triple of signed 64-bit integers by their greatest common positive divisor,
/// saturated and fitted into 32-bit integers if possible.
#[inline]
pub fn normalize_triple_i64_to_i32(x: i64, y: i64, w: i64) -> (i32, i32, i32) {
    let ux = x.unsigned_abs();
    let uy = y.unsigned_abs();
    let uw = w.unsigned_abs();

    let combined = ux | uy | uw;
    if combined == 0 {
        return (0, 0, 0);
    }

    let mut sx = x;
    let mut sy = y;
    let mut sw = w;

    let shift = combined.trailing_zeros();
    if shift > 0 {
        sx >>= shift;
        sy >>= shift;
        sw >>= shift;
    }

    let g1 = gcd_binary_u64(sx.unsigned_abs(), sy.unsigned_abs());
    let g = if g1 == 1 {
        1
    } else {
        gcd_binary_u64(g1, sw.unsigned_abs())
    };

    if g > 1 {
        sx /= g as i64;
        sy /= g as i64;
        sw /= g as i64;
    }

    // If still exceeds i32 range, bit-shift down to fit i32
    let max_mag = sx
        .unsigned_abs()
        .max(sy.unsigned_abs())
        .max(sw.unsigned_abs());
    if max_mag > i32::MAX as u64 {
        let extra_shift = 64 - max_mag.leading_zeros() - 31;
        sx >>= extra_shift;
        sy >>= extra_shift;
        sw >>= extra_shift;
    }

    (sx as i32, sy as i32, sw as i32)
}

/// Normalizes a quad of signed 32-bit integers by their greatest common positive divisor.
#[inline]
pub fn normalize_quad_i32(v: &mut [i32; 4]) {
    let ux = v[0].unsigned_abs();
    let uy = v[1].unsigned_abs();
    let uz = v[2].unsigned_abs();
    let uw = v[3].unsigned_abs();

    let combined = ux | uy | uz | uw;
    if combined == 0 {
        return;
    }

    let shift = combined.trailing_zeros();
    if shift > 0 {
        v[0] >>= shift;
        v[1] >>= shift;
        v[2] >>= shift;
        v[3] >>= shift;
    }

    let g1 = gcd_binary_u32(v[0].unsigned_abs(), v[1].unsigned_abs());
    if g1 == 1 {
        return;
    }
    let g2 = gcd_binary_u32(g1, v[2].unsigned_abs());
    if g2 == 1 {
        return;
    }
    let g = gcd_binary_u32(g2, v[3].unsigned_abs());
    if g > 1 {
        v[0] /= g as i32;
        v[1] /= g as i32;
        v[2] /= g as i32;
        v[3] /= g as i32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gcd_binary_u64() {
        assert_eq!(gcd_binary_u64(0, 0), 0);
        assert_eq!(gcd_binary_u64(0, 5), 5);
        assert_eq!(gcd_binary_u64(5, 0), 5);
        assert_eq!(gcd_binary_u64(56, 98), 14);
        assert_eq!(gcd_binary_u64(1071, 462), 21);
        assert_eq!(gcd_binary_u64(100, 75), 25);
        assert_eq!(gcd_binary_u64(u64::MAX, u64::MAX - 1), 1);
    }

    #[test]
    fn test_gcd_binary_u32() {
        assert_eq!(gcd_binary_u32(0, 0), 0);
        assert_eq!(gcd_binary_u32(0, 5), 5);
        assert_eq!(gcd_binary_u32(5, 0), 5);
        assert_eq!(gcd_binary_u32(56, 98), 14);
    }

    #[test]
    fn test_normalize_quad_i32() {
        let mut v_zero = [0, 0, 0, 0];
        normalize_quad_i32(&mut v_zero);
        assert_eq!(v_zero, [0, 0, 0, 0]);

        // g1 == 1 early exit
        let mut v_coprime = [3, 5, 10, 20];
        normalize_quad_i32(&mut v_coprime);
        assert_eq!(v_coprime, [3, 5, 10, 20]);

        // g2 == 1 early exit
        let mut v_g2 = [4, 6, 7, 14];
        normalize_quad_i32(&mut v_g2);
        assert_eq!(v_g2, [4, 6, 7, 14]);

        let mut v = [12, 18, 24, 30];
        normalize_quad_i32(&mut v);
        assert_eq!(v, [2, 3, 4, 5]);
    }

    #[test]
    fn test_normalize_triple_edge_cases() {
        let mut v_zero = [0, 0, 0];
        normalize_triple_i32(&mut v_zero);
        assert_eq!(v_zero, [0, 0, 0]);

        assert_eq!(normalize_triple_i64_to_i32(0, 0, 0), (0, 0, 0));

        // Overflow reduction
        let big = (i32::MAX as i64) * 4;
        let (nx, ny, nw) = normalize_triple_i64_to_i32(big, big / 2, big / 4);
        assert_eq!((nx, ny, nw), (4, 2, 1));
    }
}
