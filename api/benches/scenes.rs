//! The device benchmarks (`esp32/s3/tests/circles.rs` and `blend.rs`) on the host, for quick comparisons. Run with
//! `cargo bench -p yonder-graphics`. Only the ratios carry over to the device, not the times.

use std::hint::black_box;
use std::time::{Duration, Instant};

use yonder_graphics::prelude::*;
use yonder_graphics::surface::{Footprint, PixelBuffer, Surface, Viewport};
use yonder_raster::{Coverage, Filled, Rasterizable, Stroked, walk_fill};

const BAND: i16 = 16;

/// Prints the time per call of `run`: the fastest of 5 batches of about 20 ms, the one least disturbed by the rest
/// of the machine.
fn time(label: &str, mut run: impl FnMut()) {
    let mut iterations = 1u32;
    while {
        let start = Instant::now();
        (0..iterations).for_each(|_| run());
        start.elapsed() < Duration::from_millis(20)
    } {
        iterations *= 2;
    }
    let fastest = (0..5)
        .map(|_| {
            let start = Instant::now();
            (0..iterations).for_each(|_| run());
            start.elapsed() / iterations
        })
        .min()
        .unwrap();
    println!("{label:48} {:>10} ns", fastest.as_nanos());
}

/// Folds an item into a checksum, so the work can't be optimized away.
fn checksum(item: Coverage) -> u32 {
    match item {
        Coverage::Boundary(position, coverage) => position.x as u32 ^ u32::from(coverage),
        Coverage::Partial(bounds, coverage) => bounds.width() as u32 ^ u32::from(coverage),
        Coverage::Interior(bounds) => bounds.width() as u32,
    }
}

fn consume(shape: &impl Rasterizable, clip: BoundingBox) {
    let mut sum = 0u32;
    shape.rasterize(clip, |item| sum = sum.wrapping_add(checksum(item)));
    black_box(sum);
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

/// The 16-row bands of `screen`.
fn bands(screen: BoundingBox) -> impl Iterator<Item = BoundingBox> {
    let (width, height) = (screen.width(), screen.height());
    (0..height).step_by(BAND as usize).map(move |top| {
        BoundingBox::new(
            Point::new(0, top),
            Point::new(width, (top + BAND).min(height)),
        )
    })
}

/// Draws `item` on the whole screen, band by band.
fn draw_in_bands<T>(screen: BoundingBox, pixels: &mut [Rgb565], item: &T)
where
    T: for<'a> Drawable<Band<'a>>,
{
    for bounds in bands(screen) {
        let pixels = &mut pixels[..bounds.width() as usize * bounds.height() as usize];
        item.draw(&mut Band { pixels, bounds });
    }
}

fn main() {
    let screen = BoundingBox::new(Point::new(0, 0), Point::new(320, 240));
    for radius in [4, 16, 64, 110] {
        let circle = black_box(Circle::new(PointT2::new(160, 120, 1), radius));
        time(
            &format!("rasterize disk, radius {radius}, ChordWalk"),
            || consume(&Filled(&circle), screen),
        );
        time(&format!("rasterize disk, radius {radius}, walk"), || {
            let mut sum = 0u32;
            walk_fill(&circle, |item| sum = sum.wrapping_add(checksum(item)));
            black_box(sum);
        });
    }

    // The device's screen, and bigger ones where the per-call overhead of the benchmark matters less.
    for (width, height) in [(390, 390), (1024, 1024), (3840, 2160)] {
        let screen = BoundingBox::new(Point::new(0, 0), Point::new(width, height));
        let size = format!("{width}x{height}");
        // As on the device: a ring 16 px thick, its outer edge 8 px from the screen edge.
        let radius = width.min(height) / 2 - 16;
        let circle = black_box(Circle::new(
            PointT2::new(width.into(), height.into(), 2),
            radius,
        ));
        println!();

        time(&format!("rasterize {size} disk, ChordWalk"), || {
            consume(&Filled(&circle), screen)
        });
        time(&format!("rasterize {size} disk, walk"), || {
            let mut sum = 0u32;
            walk_fill(&circle, |item| sum = sum.wrapping_add(checksum(item)));
            black_box(sum);
        });
        let ring = Stroked {
            shape: &circle,
            width: 16,
        };
        time(&format!("rasterize {size} ring, full frame"), || {
            consume(&ring, screen)
        });
        time(&format!("rasterize {size} ring, 16-row bands"), || {
            bands(screen).for_each(|band| consume(&ring, band))
        });

        let mut row = vec![Rgb565::BLACK; width as usize];
        let color = Rgb565::new(21, 42, 9);
        time(&format!("{width}-pixel row, fill"), || {
            Rgb565::blend_slice(black_box(&mut row[1..]), color.with_alpha(255))
        });
        time(&format!("{width}-pixel row, blend"), || {
            Rgb565::blend_slice(black_box(&mut row[1..]), color.with_alpha(128))
        });

        let mut pixels = vec![Rgb565::BLACK; width as usize * BAND as usize];
        let red = Paint::solid(Rgb565::RED);
        let disk = black_box(circle.with_style(Style::new().fill(red)));
        time(&format!("draw {size} disk, opaque, bands"), || {
            draw_in_bands(screen, &mut pixels, &disk)
        });
        let disk = black_box(circle.with_style(Style::new().fill(red.alpha(128))));
        time(
            &format!("draw {size} disk, half transparent, bands"),
            || draw_in_bands(screen, &mut pixels, &disk),
        );
        let ring = black_box(circle.with_style(Style::new().stroke(Stroke::new(16, red))));
        time(&format!("draw {size} ring, width 16, bands"), || {
            draw_in_bands(screen, &mut pixels, &ring)
        });
    }
}
