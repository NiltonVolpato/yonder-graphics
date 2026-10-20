//! Integer scalar pixel blending kernels and generic `BlendablePixel` trait.
//!
//! Designed for fast execution without hardware division or floating-point arithmetic.
//! Channels are blended with alpha in [0, 255] using shift-add approximations for / 255.

use embedded_graphics_core::pixelcolor::{IntoStorage, PixelColor, Rgb565, Rgb888};

/// Trait associating a pixel color type with alpha blending routines over its raw storage.
pub trait BlendablePixel: PixelColor {
    /// Raw scalar word representation in memory (e.g. `u16` for RGB565, `u32` for XRGB8888).
    type Storage: Copy + PartialEq + core::fmt::Debug + 'static;

    /// Converts the pixel color into its raw storage word.
    fn into_storage(self) -> Self::Storage;

    /// Blends a source color over a destination pixel in storage representation.
    fn blend(dst: Self::Storage, src: Self, alpha: u8) -> Self::Storage;

    /// Blends a solid source color across a contiguous slice of destination storage words.
    fn blend_slice(dst: &mut [Self::Storage], src: Self, alpha: u8);
}

/// Blends a single 8-bit channel value with alpha factor $\alpha \in [0, 255]$.
///
/// Uses the exact division-free integer identity:
/// $$\lfloor \frac{V + 1 + \lfloor V / 256 \rfloor}{256} \rfloor = \lfloor \frac{V}{255} \rfloor$$
#[inline(always)]
pub(crate) const fn blend_channel(src: u32, dst: u32, alpha: u32) -> u32 {
    let val = src * alpha + dst * (255 - alpha);
    (val + 1 + (val >> 8)) >> 8
}

#[inline(always)]
fn blend_rgb565_inner(dst: u16, src: u16, a: u32) -> u16 {
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

/// Blends a source RGB565 color over an existing destination RGB565 pixel with alpha $\in [0, 255]$.
#[inline]
pub(crate) fn blend_rgb565(dst: u16, src: u16, alpha: u8) -> u16 {
    if alpha == 255 {
        return src;
    }
    if alpha == 0 {
        return dst;
    }
    blend_rgb565_inner(dst, src, alpha as u32)
}

/// Blends a solid source RGB565 color across a contiguous slice of destination pixels in local SRAM.
#[inline]
pub(crate) fn blend_slice_rgb565(dst: &mut [u16], src: u16, alpha: u8) {
    if alpha == 0 {
        return;
    }
    if alpha == 255 {
        dst.fill(src);
        return;
    }

    let a = alpha as u32;
    for pixel in dst.iter_mut() {
        *pixel = blend_rgb565_inner(*pixel, src, a);
    }
}

#[inline(always)]
fn blend_xrgb8888_inner(dst: u32, src: u32, a: u32) -> u32 {
    let r_src = (src >> 16) & 0xFF;
    let g_src = (src >> 8) & 0xFF;
    let b_src = src & 0xFF;

    let r_dst = (dst >> 16) & 0xFF;
    let g_dst = (dst >> 8) & 0xFF;
    let b_dst = dst & 0xFF;

    let r_out = blend_channel(r_src, r_dst, a);
    let g_out = blend_channel(g_src, g_dst, a);
    let b_out = blend_channel(b_src, b_dst, a);

    (r_out << 16) | (g_out << 8) | b_out
}

/// Blends a source 24/32-bit XRGB8888 color over a destination pixel with alpha $\in [0, 255]$.
#[inline]
pub(crate) fn blend_xrgb8888(dst: u32, src: u32, alpha: u8) -> u32 {
    if alpha == 255 {
        return src;
    }
    if alpha == 0 {
        return dst;
    }
    blend_xrgb8888_inner(dst, src, alpha as u32)
}

/// Blends a solid source 24/32-bit XRGB8888 color across a contiguous slice of destination words.
#[inline]
pub(crate) fn blend_slice_xrgb8888(dst: &mut [u32], src: u32, alpha: u8) {
    if alpha == 0 {
        return;
    }
    if alpha == 255 {
        dst.fill(src);
        return;
    }

    let a = alpha as u32;
    for pixel in dst.iter_mut() {
        *pixel = blend_xrgb8888_inner(*pixel, src, a);
    }
}

impl BlendablePixel for Rgb565 {
    type Storage = u16;

    #[inline(always)]
    fn into_storage(self) -> Self::Storage {
        IntoStorage::into_storage(self)
    }

    #[inline(always)]
    fn blend(dst: u16, src: Self, alpha: u8) -> u16 {
        blend_rgb565(dst, IntoStorage::into_storage(src), alpha)
    }

    #[inline(always)]
    fn blend_slice(dst: &mut [u16], src: Self, alpha: u8) {
        blend_slice_rgb565(dst, IntoStorage::into_storage(src), alpha);
    }
}

impl BlendablePixel for Rgb888 {
    type Storage = u32;

    #[inline(always)]
    fn into_storage(self) -> Self::Storage {
        IntoStorage::into_storage(self)
    }

    #[inline(always)]
    fn blend(dst: u32, src: Self, alpha: u8) -> u32 {
        blend_xrgb8888(dst, IntoStorage::into_storage(src), alpha)
    }

    #[inline(always)]
    fn blend_slice(dst: &mut [u32], src: Self, alpha: u8) {
        blend_slice_xrgb8888(dst, IntoStorage::into_storage(src), alpha);
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

    #[test]
    fn test_blend_xrgb8888_endpoints_and_isolation() {
        let red = 0x00FF0000u32;
        let blue = 0x000000FFu32;

        // Alpha = 255 -> completely source
        assert_eq!(blend_xrgb8888(blue, red, 255), red);
        // Alpha = 0 -> completely destination
        assert_eq!(blend_xrgb8888(blue, red, 0), blue);

        // Alpha = 128 -> midpoint blend
        let mid = blend_xrgb8888(blue, red, 128);
        let r = (mid >> 16) & 0xFF;
        let g = (mid >> 8) & 0xFF;
        let b = mid & 0xFF;
        assert_eq!(g, 0); // No green bleed
        assert!((127..=129).contains(&r));
        assert!((127..=129).contains(&b));
    }

    #[test]
    fn test_blend_slice_xrgb8888() {
        let mut pixels = [0x0000FF00u32; 8]; // Green
        let red = 0x00FF0000u32;

        // Alpha = 255 fills slice
        blend_slice_xrgb8888(&mut pixels, red, 255);
        assert_eq!(pixels, [red; 8]);

        // Alpha = 0 leaves slice untouched
        blend_slice_xrgb8888(&mut pixels, 0x00000000, 0);
        assert_eq!(pixels, [red; 8]);

        // Test via BlendablePixel trait
        let color = Rgb888::new(0, 0, 255);
        Rgb888::blend_slice(&mut pixels, color, 128);
        assert_ne!(pixels[0], red);
    }
}
