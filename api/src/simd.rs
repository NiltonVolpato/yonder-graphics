//! ESP32-S3 PIE (128-bit SIMD) fill and blend for RGB565, ported from smart-dial. Call [`enable_pie`] once at
//! start-up: without it the vector instructions trap.

use crate::color::Rgb565;

core::arch::global_asm!(include_str!("asm/simd_fill_color.S"));
core::arch::global_asm!(include_str!("asm/s3_simd_alphablend_color_le.S"));

unsafe extern "C" {
    /// Fills `count` pixels at `dst`, 16-byte aligned, with `*value`.
    fn simd_fill_color(dst: *mut u16, value: *const u16, count: usize) -> i32;

    /// Blends `color` with `alpha` (0..=32) over `count` pixels, a multiple of 8, from `background` into `dst`, both
    /// 16-byte aligned.
    fn s3_alpha_blend_color_le(
        color: u16,
        background: *const u16,
        dst: *mut u16,
        count: u32,
        alpha: u8,
        masks: *const u16,
    );
}

/// The channel masks the blend reads in order: blue, green, red shifted down 5 bits, red.
static COLOR_MASKS: [u16; 4] = [0x001F, 0x07E0, 0x07C0, 0xF800];

/// Eight pixels, aligned as the vector loads and stores need.
#[repr(C, align(16))]
struct Block([Rgb565; 8]);

/// Enables coprocessor 3, PIE, on the current core.
pub fn enable_pie() {
    // SAFETY: sets bit 3 of CPENABLE, nothing else.
    unsafe {
        core::arch::asm!(
            "rsr.cpenable {tmp}",
            "movi.n {mask}, 8",
            "or {tmp}, {tmp}, {mask}",
            "wsr.cpenable {tmp}",
            tmp = out(reg) _,
            mask = out(reg) _,
        );
    }
}

/// Fills `pixels` with `color`.
pub(crate) fn fill(pixels: &mut [Rgb565], color: Rgb565) {
    // SAFETY: a block is 8 pixels, nothing more.
    let (head, blocks, tail) = unsafe { pixels.align_to_mut::<Block>() };
    head.fill(color);
    tail.fill(color);
    if !blocks.is_empty() {
        // SAFETY: the blocks are 16-byte aligned and hold `8 * len` pixels.
        unsafe { simd_fill_color(blocks.as_mut_ptr().cast(), &color.0, 8 * blocks.len()) };
    }
}

/// Blends `color` with `alpha` (0..=32) over `pixels`. `scalar` blends one pixel the same way, for the pixels before
/// and after the aligned blocks.
pub(crate) fn blend(
    pixels: &mut [Rgb565],
    color: Rgb565,
    alpha: u32,
    scalar: impl Fn(&mut Rgb565),
) {
    // SAFETY: as in `fill`.
    let (head, blocks, tail) = unsafe { pixels.align_to_mut::<Block>() };
    head.iter_mut().chain(tail).for_each(scalar);
    if !blocks.is_empty() {
        let (count, blocks) = (8 * blocks.len() as u32, blocks.as_mut_ptr().cast::<u16>());
        // SAFETY: as in `fill`, blending in place.
        unsafe {
            s3_alpha_blend_color_le(
                color.0,
                blocks,
                blocks,
                count,
                alpha as u8,
                COLOR_MASKS.as_ptr(),
            )
        };
    }
}
