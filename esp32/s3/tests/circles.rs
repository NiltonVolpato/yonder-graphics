//! Times circle rasterization on the device. Run with `just bench`.
//!
//! Only rasterization is timed: the coverage items are folded into a checksum, not
//! blended.

#![no_std]
#![no_main]

esp_bootloader_esp_idf::esp_app_desc!();

#[cfg(test)]
#[embedded_test::tests(executor = esp_rtos::embassy::Executor::new())]
mod tests {
    use core::hint::black_box;

    use esp_hal::clock::CpuClock;
    use esp_hal::time::Instant;
    use yonder_geometry::projective::PointT2;
    use yonder_geometry::screen::{BoundingBox, Point};
    use yonder_geometry::shapes::Circle;
    use yonder_raster::{Coverage, Filled, Rasterizable, Stroked, walk_fill};

    const ITERATIONS: u32 = 200;

    #[init]
    fn init() {
        let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
        let peripherals = esp_hal::init(config);

        let timg0 = esp_hal::timer::timg::TimerGroup::new(peripherals.TIMG0);
        let sw_interrupt =
            esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
        esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);

        rtt_target::rtt_init_defmt!();
    }

    /// Folds an item into a checksum, so the work can't be optimized away.
    fn checksum(item: Coverage) -> u32 {
        match item {
            Coverage::Boundary(position, coverage) => position.x as u32 ^ u32::from(coverage),
            Coverage::Partial(bounds, coverage) => bounds.width() as u32 ^ u32::from(coverage),
            Coverage::Interior(bounds) => bounds.width() as u32,
        }
    }

    fn consume(shape: &impl Rasterizable, clip: BoundingBox) -> u32 {
        let mut sum = 0u32;
        shape.rasterize(clip, |item| sum = sum.wrapping_add(checksum(item)));
        sum
    }

    /// A circle without antialiasing, one span per row: the lattice points inside, walked
    /// top to bottom with F updated by additions and mirrored left to right. A lower bound
    /// for the antialiased fill. shortcut: no clipping, the benchmark circles fit the screen.
    fn plain_circle(center: Point, r: i16, mut emit: impl FnMut(Coverage)) {
        let mut span = |half_width: i32, y: i16| {
            let (left, right) = (center.x - half_width as i16, center.x + half_width as i16);
            let y = center.y + y;
            emit(Coverage::Interior(BoundingBox::new(
                Point::new(left, y),
                Point::new(right + 1, y + 1),
            )));
        };
        let mut x: i32 = 0;
        let mut f: i32 = 0; // F(0, -r) = 0
        // Top half: the chord widens. F(x + 1, y) = F(x, y) + 2x + 1.
        for y in -r..=0 {
            while f + 2 * x + 1 <= 0 {
                f += 2 * x + 1;
                x += 1;
            }
            span(x, y);
            // F(x, y + 1) = F(x, y) + 2y + 1.
            f += 2 * i32::from(y) + 1;
        }
        // Bottom half: the chord narrows. F(x − 1, y) = F(x, y) − 2x + 1.
        for y in 1..=r {
            while f > 0 {
                f += -2 * x + 1;
                x -= 1;
            }
            span(x, y);
            f += 2 * i32::from(y) + 1;
        }
    }

    /// The walk's fill, unclipped: the benchmark circles fit the screen.
    fn consume_walk(circle: &Circle) -> u32 {
        let mut sum = 0u32;
        walk_fill(circle, |item| sum = sum.wrapping_add(checksum(item)));
        sum
    }

    fn time(label: &str, size: i16, mut run: impl FnMut() -> u32) {
        let start = Instant::now();
        let mut sink = 0u32;
        for _ in 0..ITERATIONS {
            sink = sink.wrapping_add(run());
        }
        let micros = start.elapsed().as_micros();
        black_box(sink);
        defmt::info!(
            "{=str} {=i16}: {=u64} ns per call",
            label,
            size,
            micros * 1000 / u64::from(ITERATIONS)
        );
    }

    #[test]
    fn circles() {
        let screen = BoundingBox::new(Point::new(0, 0), Point::new(320, 240));
        let band = BoundingBox::new(Point::new(0, 112), Point::new(320, 128));
        let lattice = |radius| Circle::new(PointT2::new(160, 120, 1), radius);
        let pixel_center =
            |radius| Circle::new(PointT2::pixel_center(Point::new(160, 120)), radius);

        for radius in [4, 16, 64, 110] {
            let circle = black_box(lattice(radius));
            time("fill, lattice center, radius", radius, || {
                consume(&Filled(&circle), screen)
            });
            time("walk fill, lattice center, radius", radius, || {
                consume_walk(&circle)
            });
        }
        for radius in [4, 16, 64, 110] {
            let radius = black_box(radius);
            time("fill without antialiasing, radius", radius, || {
                let mut sum = 0u32;
                plain_circle(Point::new(160, 120), radius, |item| {
                    sum = sum.wrapping_add(checksum(item))
                });
                sum
            });
        }
        for radius in [16, 64] {
            let circle = black_box(pixel_center(radius));
            time("fill, pixel center, radius", radius, || {
                consume(&Filled(&circle), screen)
            });
            time("walk fill, pixel center, radius", radius, || {
                consume_walk(&circle)
            });
        }
        for width in [1, 8] {
            let circle = black_box(lattice(64));
            time("ring, radius 64, width", width as i16, || {
                consume(
                    &Stroked {
                        shape: &circle,
                        width,
                    },
                    screen,
                )
            });
        }
        let circle = black_box(lattice(110));
        time("fill, radius 110, 16-row band, radius", 110, || {
            consume(&Filled(&circle), band)
        });
    }

    /// A clock or timer ring on a 390×390 screen: 16 px thick, outer edge 8 px from the
    /// screen edge.
    #[test]
    fn progress_ring() {
        const SIDE: i16 = 390;
        const BAND: i16 = 16;
        let circle = black_box(Circle::new(PointT2::new(195, 195, 1), 179));
        let ring = Stroked {
            shape: &circle,
            width: 16,
        };
        let screen = BoundingBox::new(Point::new(0, 0), Point::new(SIDE, SIDE));
        time("390x390 ring, full frame, width", 16, || {
            consume(&ring, screen)
        });
        time("390x390 ring, all 16-row bands, width", 16, || {
            (0..SIDE)
                .step_by(BAND as usize)
                .map(|top| {
                    let band = BoundingBox::new(
                        Point::new(0, top),
                        Point::new(SIDE, (top + BAND).min(SIDE)),
                    );
                    consume(&ring, band)
                })
                .fold(0, u32::wrapping_add)
        });
    }
}
