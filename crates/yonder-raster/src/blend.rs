//! Integer scalar RGB565 blending kernels.
//!
//! Designed for fast execution without hardware division or floating-point arithmetic.
//! Channels are blended with alpha in [0, 255] using shift-add approximations for / 255.

/// Blends a single 8-bit channel value with alpha factor $\alpha \in [0, 255]$.
///
/// Uses the exact division-free integer identity:
/// $$\lfloor \frac{V + 1 + \lfloor V / 256 \rfloor}{256} \rfloor = \lfloor \frac{V}{255} \rfloor$$
#[inline(always)]
pub const fn blend_channel(src: u32, dst: u32, alpha: u32) -> u32 {
    let val = src * alpha + dst * (255 - alpha);
    (val + 1 + (val >> 8)) >> 8
}

/// Blends a source RGB565 color over an existing destination RGB565 pixel with alpha $\in [0, 255]$.
#[inline]
pub fn blend_rgb565(dst: u16, src: u16, alpha: u8) -> u16 {
    if alpha == 255 {
        return src;
    }
    if alpha == 0 {
        return dst;
    }

    let a = alpha as u32;

    let r_src = ((src >> 11) & 0x1F) as u32;
    let g_src = ((src >> 5) & 0x3F) as u32;
    let b_src = (src & 0x1F) as u32;

    let r_dst = ((dst >> 11) & 0x1F) as u32;
    let g_dst = ((dst >> 5) & 0x3F) as u32;
    let b_dst = (dst & 0x1F) as u32;

    let r_out = blend_channel(r_src, r_dst, a);
    let g_out = blend_channel(g_src, g_dst, a);
    let b_out = blend_channel(b_src, b_dst, a);

    ((r_out as u16) << 11) | ((g_out as u16) << 5) | (b_out as u16)
}

/// Blends a solid source RGB565 color across a contiguous slice of destination pixels in local SRAM.
///
/// This function serves as the central compositing hot spot and is designed to be easily
/// replaced with packed ARM Cortex-M DSP (`SMLAD`/`PKHBT`) or vector SIMD routines.
#[inline]
pub fn blend_slice_rgb565(dst: &mut [u16], src: u16, alpha: u8) {
    if alpha == 0 {
        return;
    }
    if alpha == 255 {
        dst.fill(src);
        return;
    }

    for pixel in dst.iter_mut() {
        *pixel = blend_rgb565(*pixel, src, alpha);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blend_channel_endpoints() {
        assert_eq!(blend_channel(31, 0, 255), 31);
        assert_eq!(blend_channel(31, 0, 0), 0);
        assert_eq!(blend_channel(31, 31, 128), 31);
        assert_eq!(blend_channel(0, 31, 255), 0);
        assert_eq!(blend_channel(0, 31, 0), 31);
    }

    #[test]
    fn test_blend_rgb565_endpoints() {
        let red = 0xF800u16; // (31, 0, 0)
        let green = 0x07E0u16; // (0, 63, 0)

        // Alpha = 255 -> completely source
        assert_eq!(blend_rgb565(green, red, 255), red);
        // Alpha = 0 -> completely destination
        assert_eq!(blend_rgb565(green, red, 0), green);

        // Alpha = 128 -> midpoint blend
        let mid = blend_rgb565(green, red, 128);
        let r_mid = (mid >> 11) & 0x1F;
        let g_mid = (mid >> 5) & 0x3F;
        assert!((14..=17).contains(&r_mid));
        assert!((30..=33).contains(&g_mid));
    }

    #[test]
    fn test_blend_slice_rgb565() {
        let mut pixels = [0x07E0u16; 8];
        blend_slice_rgb565(&mut pixels, 0xF800, 255);
        assert_eq!(pixels, [0xF800u16; 8]);

        blend_slice_rgb565(&mut pixels, 0x0000, 0);
        assert_eq!(pixels, [0xF800u16; 8]);
    }
}
