//! Profiles circle rasterization on the device. Run with `just profile`, which also
//! symbolizes the hot spots.

#![no_std]
#![no_main]

esp_bootloader_esp_idf::esp_app_desc!();

#[cfg(test)]
#[embedded_test::tests(executor = esp_rtos::embassy::Executor::new())]
mod tests {
    use core::hint::black_box;

    use esp_hal::clock::CpuClock;
    use esp_hal::peripherals::TIMG1;
    use yonder_geometry::projective::PointT2;
    use yonder_geometry::screen::{BoundingBox, Point};
    use yonder_geometry::shapes::Circle;
    use yonder_raster::{Coverage, Rasterizable, Stroked, walk_fill};

    #[init]
    fn init() -> TIMG1<'static> {
        let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
        let peripherals = esp_hal::init(config);

        let timg0 = esp_hal::timer::timg::TimerGroup::new(peripherals.TIMG0);
        let sw_interrupt =
            esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
        esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);

        rtt_target::rtt_init_defmt!();
        peripherals.TIMG1
    }

    /// The 390×390 progress ring from the `circles` benchmark, about 5 s of it.
    #[test]
    fn progress_ring(timg1: TIMG1<'static>) {
        let circle = black_box(Circle::new(PointT2::new(195, 195, 1), 179));
        let ring = Stroked {
            shape: &circle,
            width: 16,
        };
        let screen = BoundingBox::new(Point::new(0, 0), Point::new(390, 390));

        s3::profiler::start(timg1);
        for _ in 0..400 {
            ring.rasterize(screen, |item| {
                black_box(match item {
                    Coverage::Boundary(position, coverage) => (position.x, coverage),
                    Coverage::Partial(bounds, coverage) => (bounds.width(), coverage),
                    Coverage::Interior(bounds) => (bounds.width(), 255),
                });
            });
        }
        s3::profiler::stop();
    }

    /// The walk's fill of a radius 110 disk, about 5 s of it.
    #[test]
    fn walk_fill_disk(timg1: TIMG1<'static>) {
        let circle = black_box(Circle::new(PointT2::new(160, 120, 1), 110));
        s3::profiler::start(timg1);
        for _ in 0..15_000 {
            walk_fill(&circle, |item| {
                black_box(match item {
                    Coverage::Boundary(position, coverage) => (position.x, coverage),
                    Coverage::Partial(bounds, coverage) => (bounds.width(), coverage),
                    Coverage::Interior(bounds) => (bounds.width(), 255),
                });
            });
        }
        s3::profiler::stop();
    }

    /// A band of the screen, as in the `blend` test.
    struct Band<'a> {
        pixels: &'a mut [yonder_graphics::prelude::Rgb565],
        bounds: BoundingBox,
    }

    impl yonder_graphics::surface::Surface for Band<'_> {
        type Color = yonder_graphics::prelude::Rgb565;

        fn viewport(
            &mut self,
            item: &impl yonder_graphics::surface::Footprint,
        ) -> Option<impl yonder_graphics::surface::PixelBuffer<Color = Self::Color>> {
            item.extent().intersection(&self.bounds)?;
            Some(yonder_graphics::surface::Viewport::new(
                self.pixels,
                self.bounds,
            ))
        }
    }

    /// Drawing the 390x390 ring in 16-row bands, rasterizing and blending, about 5 s of it.
    #[test]
    fn ring_draw(timg1: TIMG1<'static>) {
        use yonder_graphics::prelude::*;
        yonder_graphics::simd::enable_pie();
        static mut PIXELS: [Rgb565; 390 * 16] = [Rgb565::BLACK; 390 * 16];
        // SAFETY: only this test uses it.
        let pixels = unsafe { &mut *(&raw mut PIXELS) };
        let circle = Circle::new(PointT2::new(195, 195, 1), 179);
        let ring = black_box(
            circle.with_style(Style::new().stroke(Stroke::new(16, Paint::solid(Rgb565::RED)))),
        );
        s3::profiler::start(timg1);
        for _ in 0..900 {
            for top in (0..390).step_by(16) {
                let bounds =
                    BoundingBox::new(Point::new(0, top), Point::new(390, (top + 16).min(390)));
                let pixels = &mut pixels[..390 * bounds.height() as usize];
                ring.draw(&mut Band { pixels, bounds });
            }
        }
        s3::profiler::stop();
    }
}
