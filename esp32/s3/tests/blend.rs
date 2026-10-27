//! Checks the PIE fill and blend against the scalar formula, and times them, alone and drawing whole scenes in
//! 16-row bands. Run with `just bench`.

#![no_std]
#![no_main]

esp_bootloader_esp_idf::esp_app_desc!();

#[cfg(test)]
#[embedded_test::tests(executor = esp_rtos::embassy::Executor::new())]
mod tests {
    use core::hint::black_box;

    use esp_hal::clock::CpuClock;
    use esp_hal::time::Instant;
    use yonder_graphics::prelude::*;
    use yonder_graphics::surface::{Footprint, PixelBuffer, Surface, Viewport};

    const SIDE: i16 = 390;
    const BAND: i16 = 16;
    const ITERATIONS: u32 = 50;

    #[init]
    fn init() {
        let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
        let peripherals = esp_hal::init(config);

        let timg0 = esp_hal::timer::timg::TimerGroup::new(peripherals.TIMG0);
        let sw_interrupt =
            esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
        esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);

        rtt_target::rtt_init_defmt!();
        yonder_graphics::simd::enable_pie();
    }

    /// The scalar blend, channel by channel: what the SIMD blend must match.
    fn reference(d: Rgb565, s: Rgb565, alpha: u8) -> Rgb565 {
        let a = (u16::from(alpha) + 4) >> 3;
        let mix = |s: u8, d: u8| ((u16::from(s) * a + u16::from(d) * (32 - a)) >> 5) as u8;
        match alpha {
            0 => d,
            u8::MAX => s,
            _ => Rgb565::new(mix(s.r(), d.r()), mix(s.g(), d.g()), mix(s.b(), d.b())),
        }
    }

    /// A pseudo-random pixel.
    fn noise(i: u32) -> Rgb565 {
        let x = i.wrapping_mul(2_654_435_761) >> 16;
        Rgb565::new(x as u8, (x >> 5) as u8, (x >> 11) as u8)
    }

    #[test]
    fn simd_matches_scalar() {
        let mut pixels = [Rgb565::BLACK; 64];
        let color = Rgb565::new(21, 42, 9);
        for alpha in [0, 1, 7, 100, 128, 200, 250, 254, 255] {
            // Every start and length up to 40, so the aligned blocks start and end everywhere.
            for start in 0..16 {
                for len in 0..40 {
                    for (i, p) in pixels.iter_mut().enumerate() {
                        *p = noise(i as u32 + u32::from(alpha));
                    }
                    let before = pixels;
                    Rgb565::blend_slice(&mut pixels[start..start + len], color.with_alpha(alpha));
                    for (i, (&got, &was)) in pixels.iter().zip(&before).enumerate() {
                        let inside = (start..start + len).contains(&i);
                        let want = if inside {
                            reference(was, color, alpha)
                        } else {
                            was
                        };
                        assert!(
                            got == want,
                            "alpha {}, start {}, len {}, pixel {}",
                            alpha,
                            start,
                            len,
                            i
                        );
                    }
                }
            }
        }
    }

    fn time(label: &str, mut run: impl FnMut()) {
        let start = Instant::now();
        for _ in 0..ITERATIONS {
            run();
        }
        let micros = start.elapsed().as_micros();
        defmt::info!(
            "{=str}: {=u64} ns",
            label,
            micros * 1000 / u64::from(ITERATIONS)
        );
    }

    /// One row of the screen, filled and blended with SIMD and with the scalar formula.
    #[test]
    fn row() {
        let mut row = [Rgb565::BLACK; SIDE as usize];
        let color = Rgb565::new(21, 42, 9);
        time("390-pixel row, fill, SIMD", || {
            Rgb565::blend_slice(black_box(&mut row[1..]), color.with_alpha(255))
        });
        time("390-pixel row, fill, scalar", || {
            black_box(&mut row[1..]).fill(color)
        });
        time("390-pixel row, blend, SIMD", || {
            Rgb565::blend_slice(black_box(&mut row[1..]), color.with_alpha(128))
        });
        time("390-pixel row, blend, scalar", || {
            for p in black_box(&mut row[1..]) {
                *p = reference(*p, color, 128);
            }
        });
    }

    /// A band of the screen, as a device with a small buffer renders.
    struct Band<'a> {
        pixels: &'a mut [Rgb565],
        bounds: BoundingBox,
    }

    impl Surface for Band<'_> {
        type Color = Rgb565;

        fn viewport(&mut self, item: &impl Footprint) -> Option<impl PixelBuffer<Color = Rgb565>> {
            item.extent().intersection(&self.bounds)?;
            Some(Viewport::new(self.pixels, self.bounds))
        }
    }

    /// Draws `item` on the whole screen, band by band.
    fn draw_in_bands<T>(pixels: &mut [Rgb565], item: &T)
    where
        T: for<'a> Drawable<Band<'a>>,
    {
        for top in (0..SIDE).step_by(BAND as usize) {
            let bounds =
                BoundingBox::new(Point::new(0, top), Point::new(SIDE, (top + BAND).min(SIDE)));
            let pixels = &mut pixels[..bounds.width() as usize * bounds.height() as usize];
            item.draw(&mut Band { pixels, bounds });
        }
    }

    #[test]
    fn scenes() {
        static mut PIXELS: [Rgb565; SIDE as usize * BAND as usize] =
            [Rgb565::BLACK; SIDE as usize * BAND as usize];
        // SAFETY: only this test uses it.
        let pixels = unsafe { &mut *(&raw mut PIXELS) };
        let circle = Circle::new(PointT2::new(195, 195, 1), 179);
        let red = Paint::solid(Rgb565::RED);

        let disk = black_box(circle.with_style(Style::new().fill(red)));
        time("390x390 disk, opaque", || draw_in_bands(pixels, &disk));
        let disk = black_box(circle.with_style(Style::new().fill(red.alpha(128))));
        time("390x390 disk, half transparent", || {
            draw_in_bands(pixels, &disk)
        });
        let ring = black_box(circle.with_style(Style::new().stroke(Stroke::new(16, red))));
        time("390x390 ring, width 16, opaque", || {
            draw_in_bands(pixels, &ring)
        });
    }
}
