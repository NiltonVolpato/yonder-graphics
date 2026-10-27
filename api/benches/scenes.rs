//! The device benchmarks (`esp32/s3/tests/circles.rs` and `blend.rs`) on the host, for quick comparisons. Run with
//! `cargo bench -p yonder-graphics`. Only the ratios carry over to the device, not the times.

use std::hint::black_box;
use std::time::{Duration, Instant};

use yonder_graphics::prelude::*;
use yonder_graphics::surface::{Footprint, PixelBuffer, Surface, Viewport};
use yonder_raster::{Coverage, Filled, Rasterizable, Stroked, walk_fill};

const SIDE: i16 = 390;
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

/// Draws `item` on the whole screen, band by band.
fn draw_in_bands<T>(pixels: &mut [Rgb565], item: &T)
where
    T: for<'a> Drawable<Band<'a>>,
{
    for top in (0..SIDE).step_by(BAND as usize) {
        let bounds = BoundingBox::new(Point::new(0, top), Point::new(SIDE, (top + BAND).min(SIDE)));
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

    let circle = black_box(Circle::new(PointT2::new(195, 195, 1), 179));
    let ring = Stroked {
        shape: &circle,
        width: 16,
    };
    let full = BoundingBox::new(Point::new(0, 0), Point::new(SIDE, SIDE));
    time("rasterize 390x390 ring, full frame", || {
        consume(&ring, full)
    });
    time("rasterize 390x390 ring, 16-row bands", || {
        for top in (0..SIDE).step_by(BAND as usize) {
            let band =
                BoundingBox::new(Point::new(0, top), Point::new(SIDE, (top + BAND).min(SIDE)));
            consume(&ring, band);
        }
    });

    let mut row = [Rgb565::BLACK; SIDE as usize];
    let color = Rgb565::new(21, 42, 9);
    time("390-pixel row, fill", || {
        Rgb565::blend_slice(black_box(&mut row[1..]), color.with_alpha(255))
    });
    time("390-pixel row, blend", || {
        Rgb565::blend_slice(black_box(&mut row[1..]), color.with_alpha(128))
    });

    let mut pixels = vec![Rgb565::BLACK; SIDE as usize * BAND as usize];
    let red = Paint::solid(Rgb565::RED);
    let disk = black_box(circle.with_style(Style::new().fill(red)));
    time("draw 390x390 disk, opaque, bands", || {
        draw_in_bands(&mut pixels, &disk)
    });
    let disk = black_box(circle.with_style(Style::new().fill(red.alpha(128))));
    time("draw 390x390 disk, half transparent, bands", || {
        draw_in_bands(&mut pixels, &disk)
    });
    let ring = black_box(circle.with_style(Style::new().stroke(Stroke::new(16, red))));
    time("draw 390x390 ring, width 16, bands", || {
        draw_in_bands(&mut pixels, &ring)
    });
}
