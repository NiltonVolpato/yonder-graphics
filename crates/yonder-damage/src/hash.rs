//! 16-bit hash accumulators for spatial tile damage tracking.
//!
//! Designed for fast execution on embedded 32-bit registers (zero division, zero branches)
//! while ensuring non-commutativity so drawing order changes trigger damage.

/// Tool identifier for solid rectangle blending (`DamageTarget::blend_solid` when $\alpha = 255$).
pub(crate) const TOOL_ID_SOLID_RECT: u8 = 2;

/// Tool identifier for styled shape or alpha rectangle rasterization (`DamageTarget::blend_solid`).
pub(crate) const TOOL_ID_SHAPE: u8 = 3;

/// Tool identifier for individual pixel iterator blending (`DamageTarget::blend_iter`).
pub(crate) const TOOL_ID_PIXEL_ITER: u8 = 4;

/// Tool identifier for contiguous pixel buffer blending (`DamageTarget::blend_contiguous`).
pub(crate) const TOOL_ID_CONTIGUOUS: u8 = 5;

/// Accumulates a 16-bit token into a running 16-bit tile hash.
///
/// Uses an integer bit-rotation and prime multiplicative step:
/// Non-commutative: $f(f(h, A), B) \ne f(f(h, B), A)$.
#[inline(always)]
pub(crate) const fn hash_accumulate(current: u16, token: u16) -> u16 {
    // 0x9E37 is the 16-bit golden ratio prime constant (2^16 / phi)
    let mixed = current.wrapping_add(token).rotate_left(5);
    mixed ^ token.wrapping_mul(0x9E37)
}

/// Produces a deterministic 16-bit hash for a full-tile solid opaque color overwrite.
///
/// Any full-tile overwrite resets the tile's hash to this value, discarding prior rendering history.
#[inline(always)]
pub const fn hash_solid(color: u16) -> u16 {
    let mixed = (color ^ 0x811C).wrapping_mul(0x9E37);
    mixed ^ (mixed >> 7)
}

/// Zero-allocation, tool-aware shape token builder.
///
/// Builds a 16-bit token representing a tool's drawing parameters (e.g. tool ID,
/// stroke width, angle, color) for partial-tile accumulation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileHasher(u16);

impl TileHasher {
    /// Creates a new `TileHasher` initialized with a unique tool identifier.
    #[inline(always)]
    pub const fn new(tool_id: u8) -> Self {
        Self(0x811Cu16 ^ (tool_id as u16))
    }

    /// Folds an 8-bit unsigned integer (e.g. subpixel alpha/coverage) into the token.
    #[inline(always)]
    pub const fn write_u8(self, val: u8) -> Self {
        Self(hash_accumulate(self.0, val as u16))
    }

    /// Folds a 16-bit unsigned integer (e.g. RGB565 color or stroke width) into the token.
    #[inline(always)]
    pub const fn write_u16(self, val: u16) -> Self {
        Self(hash_accumulate(self.0, val))
    }

    /// Folds a 32-bit signed integer (e.g. coordinate or angle) into the token.
    #[inline(always)]
    pub const fn write_i32(self, val: i32) -> Self {
        let low = val as u16;
        let high = (val >> 16) as u16;
        Self(hash_accumulate(hash_accumulate(self.0, low), high))
    }

    /// Folds a 32-bit unsigned integer (e.g. 24/32-bit color or raw storage) into the token.
    #[inline(always)]
    pub const fn write_u32(self, val: u32) -> Self {
        let low = val as u16;
        let high = (val >> 16) as u16;
        Self(hash_accumulate(hash_accumulate(self.0, low), high))
    }

    /// Completes hashing and returns the final 16-bit token.
    #[inline(always)]
    pub const fn finish(self) -> u16 {
        self.0
    }
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
    fn test_tile_hasher_sensitivity() {
        let t1 = TileHasher::new(1)
            .write_i32(10)
            .write_i32(20)
            .write_u16(0xF800)
            .finish();
        let t2 = TileHasher::new(1)
            .write_i32(11)
            .write_i32(20)
            .write_u16(0xF800)
            .finish();
        let t3 = TileHasher::new(1)
            .write_i32(10)
            .write_i32(20)
            .write_u16(0x07E0)
            .finish();
        let t4 = TileHasher::new(2)
            .write_i32(10)
            .write_i32(20)
            .write_u16(0xF800)
            .finish();

        assert_ne!(t1, t2);
        assert_ne!(t1, t3);
        assert_ne!(t1, t4);
    }

    #[test]
    fn test_hash_solid_determinism_and_distribution() {
        let h_white1 = hash_solid(0xFFFF);
        let h_white2 = hash_solid(0xFFFF);
        let h_black = hash_solid(0x0000);
        let h_red = hash_solid(0xF800);

        assert_eq!(h_white1, h_white2);
        assert_ne!(h_white1, h_black);
        assert_ne!(h_white1, h_red);
        assert_ne!(h_black, h_red);
    }
}
