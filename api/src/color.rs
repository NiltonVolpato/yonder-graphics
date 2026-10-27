use core::hash::Hash;

pub trait Color: Copy + PartialEq + Hash {
    const BLACK: Self;
    const WHITE: Self;
    const RED: Self;
    const GREEN: Self;
    const BLUE: Self;
    const YELLOW: Self;
    const CYAN: Self;
    const MAGENTA: Self;

    fn with_alpha(self, alpha: u8) -> WithAlpha<Self> {
        WithAlpha { color: self, alpha }
    }

    /// Blends `src` over every pixel in `dst`. Work that depends only on `src` should
    /// be done once per call, not once per pixel.
    fn blend_slice(dst: &mut [Self], src: WithAlpha<Self>);

    /// Blends `src` over one pixel.
    #[inline]
    fn blend(dst: &mut Self, src: WithAlpha<Self>) {
        Self::blend_slice(core::slice::from_mut(dst), src);
    }
}

/// A color plus the opacity to blend it with (255 = opaque). Screens never store alpha.
#[derive(Clone, Copy, PartialEq, Hash)]
pub struct WithAlpha<C> {
    pub color: C,
    pub alpha: u8,
}

/// Defines every named color once, as 8-bit RGB, for a type with a `const fn from_rgb8`.
macro_rules! named_colors {
    () => {
        const BLACK: Self = Self::from_rgb8(0, 0, 0);
        const WHITE: Self = Self::from_rgb8(255, 255, 255);
        const RED: Self = Self::from_rgb8(255, 0, 0);
        const GREEN: Self = Self::from_rgb8(0, 255, 0);
        const BLUE: Self = Self::from_rgb8(0, 0, 255);
        const YELLOW: Self = Self::from_rgb8(255, 255, 0);
        const CYAN: Self = Self::from_rgb8(0, 255, 255);
        const MAGENTA: Self = Self::from_rgb8(255, 0, 255);
    };
}

/// 16-bit color: 5 bits red, 6 bits green, 5 bits blue.
#[derive(Clone, Copy, PartialEq, Hash)]
#[repr(transparent)] // the SIMD routines read it as a `u16`
pub struct Rgb565(pub(crate) u16);

impl Rgb565 {
    /// Takes channels at native depth: `r` and `b` in 0..32, `g` in 0..64.
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self(((r as u16 & 0x1f) << 11) | ((g as u16 & 0x3f) << 5) | (b as u16 & 0x1f))
    }

    pub const fn from_rgb8(r: u8, g: u8, b: u8) -> Self {
        Self::new(r >> 3, g >> 2, b >> 3)
    }

    pub const fn r(self) -> u8 {
        (self.0 >> 11) as u8
    }

    pub const fn g(self) -> u8 {
        ((self.0 >> 5) & 0x3f) as u8
    }

    pub const fn b(self) -> u8 {
        (self.0 & 0x1f) as u8
    }

    /// The channels spread apart (G high, R and B low), so one multiply blends all three.
    const fn spread(self) -> u32 {
        (self.0 as u32 | (self.0 as u32) << 16) & SPREAD_MASK
    }

    /// Blends `src`, spread and multiplied by its alpha `a` (0..=32), over this pixel.
    #[inline(always)]
    fn mix(&mut self, src: u32, a: u32) {
        let mixed = ((src + self.spread() * (32 - a)) >> 5) & SPREAD_MASK;
        self.0 = (mixed | (mixed >> 16)) as u16;
    }
}

const SPREAD_MASK: u32 = 0x07e0_f81f;

impl Color for Rgb565 {
    named_colors!();

    fn blend_slice(dst: &mut [Self], src: WithAlpha<Self>) {
        match src.alpha {
            0 => {}
            #[cfg(feature = "esp32s3-simd")]
            u8::MAX => crate::simd::fill(dst, src.color),
            #[cfg(not(feature = "esp32s3-simd"))]
            u8::MAX => dst.fill(src.color),
            alpha => {
                let a = (u32::from(alpha) + 4) >> 3; // 0..=32
                let src_times_a = src.color.spread() * a;
                let blend = |d: &mut Rgb565| d.mix(src_times_a, a);
                #[cfg(feature = "esp32s3-simd")]
                crate::simd::blend(dst, src.color, a, blend);
                #[cfg(not(feature = "esp32s3-simd"))]
                dst.iter_mut().for_each(blend);
            }
        }
    }

    #[inline]
    fn blend(dst: &mut Self, src: WithAlpha<Self>) {
        match src.alpha {
            0 => {}
            u8::MAX => *dst = src.color,
            alpha => {
                let a = (u32::from(alpha) + 4) >> 3; // 0..=32
                dst.mix(src.color.spread() * a, a);
            }
        }
    }
}

/// 24-bit color, 8 bits per channel.
#[derive(Clone, Copy, PartialEq, Hash)]
pub struct Rgb888 {
    r: u8,
    g: u8,
    b: u8,
}

impl Rgb888 {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    const fn from_rgb8(r: u8, g: u8, b: u8) -> Self {
        Self::new(r, g, b)
    }

    pub const fn r(self) -> u8 {
        self.r
    }

    pub const fn g(self) -> u8 {
        self.g
    }

    pub const fn b(self) -> u8 {
        self.b
    }
}

impl Color for Rgb888 {
    named_colors!();

    fn blend_slice(dst: &mut [Self], src: WithAlpha<Self>) {
        match src.alpha {
            0 => {}
            u8::MAX => dst.fill(src.color),
            alpha => {
                let a = u16::from(alpha);
                let (r, g, b) = (
                    src.color.r as u16 * a,
                    src.color.g as u16 * a,
                    src.color.b as u16 * a,
                );
                let mix = |s: u16, d: u8| div255(s + u16::from(d) * (255 - a));
                for d in dst {
                    *d = Self::new(mix(r, d.r), mix(g, d.g), mix(b, d.b));
                }
            }
        }
    }
}

impl From<Rgb888> for Rgb565 {
    fn from(c: Rgb888) -> Self {
        Self::from_rgb8(c.r, c.g, c.b)
    }
}

impl From<Rgb565> for Rgb888 {
    fn from(c: Rgb565) -> Self {
        // Repeat the high bits in the low bits so full intensity maps to 255.
        let (r, g, b) = (c.r(), c.g(), c.b());
        Self::new(
            (r << 3) | (r >> 2),
            (g << 2) | (g >> 4),
            (b << 3) | (b >> 2),
        )
    }
}

/// `x / 255`, rounded, for `x <= 255 * 255`.
pub(crate) const fn div255(x: u16) -> u8 {
    let x = x as u32 + 128;
    ((x + (x >> 8)) >> 8) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn div255_matches_rounded_division() {
        for x in 0..=255 * 255u32 {
            assert_eq!(u32::from(div255(x as u16)), (x + 127) / 255);
        }
    }

    #[test]
    fn conversions_keep_named_colors() {
        assert!(Rgb565::from(Rgb888::YELLOW) == Rgb565::YELLOW);
        assert!(Rgb888::from(Rgb565::WHITE) == Rgb888::WHITE);
        assert!(Rgb888::from(Rgb565::BLACK) == Rgb888::BLACK);
    }

    #[test]
    fn blend_slice_mixes_by_alpha() {
        let mut px = [Rgb565::BLACK; 3];
        Rgb565::blend_slice(&mut px, Rgb565::WHITE.with_alpha(128));
        assert!(px.iter().all(|p| (p.r(), p.g(), p.b()) == (15, 31, 15)));
        Rgb565::blend_slice(&mut px, Rgb565::RED.with_alpha(0));
        assert!(px[0].r() == 15);

        let mut px = [Rgb888::new(0, 100, 255)];
        Rgb888::blend_slice(&mut px, Rgb888::WHITE.with_alpha(64));
        assert!(px[0] == Rgb888::new(64, 139, 255));
    }
}
