//! 16-bit hash accumulators for spatial tile damage tracking.
//!
//! Designed for fast execution on embedded 32-bit registers (zero division, zero branches)
//! while ensuring non-commutativity so drawing order changes trigger damage.

/// Accumulates a 16-bit token into a running 16-bit tile hash.
///
/// Uses an integer bit-rotation and prime multiplicative step:
/// Non-commutative: $f(f(h, A), B) \ne f(f(h, B), A)$.
#[inline(always)]
pub const fn hash_accumulate(current: u16, token: u16) -> u16 {
    // 0x9E37 is the 16-bit golden ratio prime constant (2^16 / phi)
    let mixed = current.wrapping_add(token).rotate_left(5);
    mixed ^ token.wrapping_mul(0x9E37)
}

/// Generates a 16-bit hash token from a primitive ID, integer coordinates, and a color payload.
#[inline]
pub const fn make_primitive_token(primitive_id: u8, x: i32, y: i32, color_bits: u16) -> u16 {
    let mut h = 0x811Cu16; // FNV offset basis 16-bit
    h = hash_accumulate(h, primitive_id as u16);
    h = hash_accumulate(h, (x as u16) ^ ((x >> 16) as u16));
    h = hash_accumulate(h, (y as u16) ^ ((y >> 16) as u16));
    hash_accumulate(h, color_bits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_non_commutative() {
        let h0 = 0;
        let h_ab = hash_accumulate(hash_accumulate(h0, 0x1234), 0x5678);
        let h_ba = hash_accumulate(hash_accumulate(h0, 0x5678), 0x1234);

        assert_ne!(h_ab, h_ba, "Hash accumulation must be non-commutative");
    }

    #[test]
    fn test_primitive_token_sensitivity() {
        let t1 = make_primitive_token(1, 10, 20, 0xF800);
        let t2 = make_primitive_token(1, 11, 20, 0xF800);
        let t3 = make_primitive_token(1, 10, 20, 0x07E0);
        let t4 = make_primitive_token(2, 10, 20, 0xF800);

        assert_ne!(t1, t2);
        assert_ne!(t1, t3);
        assert_ne!(t1, t4);
    }
}
