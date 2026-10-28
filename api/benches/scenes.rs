//! The device benchmarks (`esp32/s3/tests/circles.rs` and `blend.rs`) on the host, for quick comparisons. Run with
//! `cargo bench -p yonder-graphics`. Only the ratios carry over to the device, not the times.

use std::cell::Cell;
use std::hint::black_box;
use std::time::{Duration, Instant};

use human_repr::HumanDuration;

use yonder_graphics::prelude::*;
use yonder_graphics::surface::{Footprint, PixelBuffer, Surface, Viewport};
use yonder_raster::{Coverage, Filled, Rasterizable, Stroked, walk_fill};

const BAND: i16 = 16;

/// Parses a duration string such as `"500ms"`, `"2s"`, or `"500"` (treated as milliseconds).
fn parse_duration(value: &str) -> Result<Duration, String> {
    if let Some(millis) = value.strip_suffix("ms") {
        let millis: u64 = millis
            .trim()
            .parse()
            .map_err(|error| format!("invalid milliseconds in '{value}': {error}"))?;
        Ok(Duration::from_millis(millis))
    } else if let Some(secs) = value.strip_suffix('s') {
        let secs: f64 = secs
            .trim()
            .parse()
            .map_err(|error| format!("invalid seconds in '{value}': {error}"))?;
        Ok(Duration::from_secs_f64(secs))
    } else {
        let millis: u64 = value.trim().parse().map_err(|error| {
            format!(
                "invalid duration '{value}', expected number in ms or duration with 'ms'/'s': {error}"
            )
        })?;
        Ok(Duration::from_millis(millis))
    }
}

/// Manages benchmark filtering, timing batches, and output reporting.
struct BenchmarkRunner {
    filters: Vec<String>,
    batch_duration: Duration,
    has_custom_duration: bool,
    explicit_iterations: Option<u64>,
    group_has_printed: Cell<bool>,
}

impl BenchmarkRunner {
    const DEFAULT_BATCH_DURATION: Duration = Duration::from_millis(20);
    const BATCH_COUNT: u32 = 5;

    fn from_args(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut filters = Vec::new();
        let mut total_duration = None;
        let mut has_custom_duration = false;
        let mut explicit_iterations = None;

        let mut args_iter = args.into_iter();
        while let Some(arg) = args_iter.next() {
            if arg == "--profile" {
                total_duration = Some(Duration::from_secs(2));
                has_custom_duration = true;
            } else if let Some(duration_str) = arg.strip_prefix("--duration=") {
                total_duration = Some(parse_duration(duration_str)?);
                has_custom_duration = true;
            } else if arg == "--duration" {
                let duration_str = args_iter.next().ok_or_else(|| {
                    "--duration requires a value (e.g., --duration 2s or --duration 500ms)"
                        .to_string()
                })?;
                if duration_str.starts_with('-') {
                    return Err(
                        "--duration requires a value (e.g., --duration 2s or --duration 500ms)"
                            .to_string(),
                    );
                }
                total_duration = Some(parse_duration(&duration_str)?);
                has_custom_duration = true;
            } else if let Some(iters_str) = arg.strip_prefix("--iterations=") {
                let iters: u64 = iters_str.parse().map_err(|error| {
                    format!("invalid iteration count in '{arg}': {error}")
                })?;
                explicit_iterations = Some(iters);
            } else if arg == "--iterations" || arg == "-n" {
                let iters_str = args_iter.next().ok_or_else(|| {
                    format!("{arg} requires a positive number of iterations")
                })?;
                if iters_str.starts_with('-') {
                    return Err(format!("{arg} requires a positive number of iterations"));
                }
                let iters: u64 = iters_str.parse().map_err(|error| {
                    format!("invalid iteration count '{iters_str}': {error}")
                })?;
                explicit_iterations = Some(iters);
            } else if arg == "--self-test" {
                run_self_tests();
                println!("Self-tests passed.");
                std::process::exit(0);
            } else if arg == "--help" || arg == "-h" {
                println!(
                    "Usage: cargo bench -p yonder-graphics --bench scenes -- [FILTERS] [--iterations <N>] [--duration <ms|s>] [--profile]"
                );
                println!();
                println!("Filters benchmarks matching all positional arguments (AND logic).");
                println!("Options:");
                println!("  -n, --iterations <N>  Run exactly N iterations (skips calibration, ideal for profiling)");
                println!("      --duration <ms|s> Total benchmark duration (divided across 5 batches)");
                println!("      --profile         Shortcut for --duration 2s");
                println!("      --self-test       Run internal runner sanity checks and exit");
                println!("  -h, --help            Print this help");
                std::process::exit(0);
            } else if arg == "--bench" || arg == "--exact" {
                // Passed automatically by `cargo bench`; ignore.
            } else if arg.starts_with('-') {
                return Err(format!("unknown option '{arg}'. Run with --help for usage."));
            } else {
                filters.push(arg);
            }
        }

        let batch_duration = match total_duration {
            Some(duration) => duration / Self::BATCH_COUNT,
            None => Self::DEFAULT_BATCH_DURATION,
        };

        Ok(Self {
            filters,
            batch_duration,
            has_custom_duration,
            explicit_iterations,
            group_has_printed: Cell::new(false),
        })
    }

    fn matches(&self, label: &str) -> bool {
        self.filters.iter().all(|filter| label.contains(filter))
    }

    fn print_header(&self) {
        if !self.filters.is_empty() {
            println!("Filtering benchmarks matching: {:?}", self.filters);
            if let Some(iters) = self.explicit_iterations {
                println!("Running exactly {iters} iterations per matched benchmark.");
            } else if !self.has_custom_duration {
                println!(
                    "Running with default duration 20ms per batch (~100ms total). Use --duration <ms|s> or --profile to increase."
                );
            }
        } else if let Some(iters) = self.explicit_iterations {
            println!("Running exactly {iters} iterations per benchmark.");
        }
    }

    fn end_group(&self) {
        if self.group_has_printed.replace(false) {
            println!();
        }
    }

    /// Measures the runtime of `run`.
    /// When `explicit_iterations` is set, runs a single flat loop without calibration.
    /// Otherwise, runs calibration and 5 batches, reporting the fastest iteration time and
    /// the estimated iterations needed to run for 1 second.
    fn time(&self, label: &str, mut run: impl FnMut()) {
        if !self.matches(label) {
            return;
        }

        if let Some(iterations) = self.explicit_iterations {
            let start = Instant::now();
            for _ in 0..iterations {
                run();
            }
            let elapsed = start.elapsed();
            let per_iteration = if iterations > 0 {
                Duration::from_nanos((elapsed.as_nanos() / iterations as u128) as u64)
            } else {
                Duration::ZERO
            };
            println!(
                "{label:48} {:>10}  ({iterations} iters in {})",
                per_iteration.human_duration(),
                elapsed.human_duration()
            );
            self.group_has_printed.set(true);
            return;
        }

        let mut iterations = 1u64;
        while {
            let start = Instant::now();
            for _ in 0..iterations {
                run();
            }
            start.elapsed() < self.batch_duration
        } {
            iterations *= 2;
        }

        let mut fastest = Duration::MAX;
        for _ in 0..Self::BATCH_COUNT {
            let start = Instant::now();
            for _ in 0..iterations {
                run();
            }
            let batch_per_iteration = Duration::from_nanos((start.elapsed().as_nanos() / iterations as u128) as u64);
            fastest = fastest.min(batch_per_iteration);
        }

        let iters_for_1s = if fastest.as_nanos() > 0 {
            Duration::from_secs(1).as_nanos() / fastest.as_nanos()
        } else {
            0
        };

        println!(
            "{label:48} {:>10}  ({iters_for_1s} iters for 1s)",
            fastest.human_duration()
        );
        self.group_has_printed.set(true);
    }
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
    let runner = match BenchmarkRunner::from_args(std::env::args().skip(1)) {
        Ok(runner) => runner,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    runner.print_header();

    let screen = BoundingBox::new(Point::new(0, 0), Point::new(320, 240));
    for radius in [4, 16, 64, 110] {
        let circle = black_box(Circle::new(PointT2::new(160, 120, 1), radius));
        runner.time(
            &format!("rasterize disk, radius {radius}, ChordWalk"),
            || consume(&Filled(&circle), screen),
        );
        runner.time(&format!("rasterize disk, radius {radius}, walk"), || {
            let mut sum = 0u32;
            walk_fill(&circle, |item| sum = sum.wrapping_add(checksum(item)));
            black_box(sum);
        });
    }
    runner.end_group();

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

        runner.time(&format!("rasterize {size} disk, ChordWalk"), || {
            consume(&Filled(&circle), screen)
        });
        runner.time(&format!("rasterize {size} disk, walk"), || {
            let mut sum = 0u32;
            walk_fill(&circle, |item| sum = sum.wrapping_add(checksum(item)));
            black_box(sum);
        });
        let ring = Stroked {
            shape: &circle,
            width: 16,
        };
        runner.time(&format!("rasterize {size} ring, full frame"), || {
            consume(&ring, screen)
        });
        runner.time(&format!("rasterize {size} ring, 16-row bands"), || {
            bands(screen).for_each(|band| consume(&ring, band))
        });

        let mut row = vec![Rgb565::BLACK; width as usize];
        let color = Rgb565::new(21, 42, 9);
        runner.time(&format!("{width}-pixel row, fill"), || {
            Rgb565::blend_slice(black_box(&mut row[1..]), color.with_alpha(255))
        });
        runner.time(&format!("{width}-pixel row, blend"), || {
            Rgb565::blend_slice(black_box(&mut row[1..]), color.with_alpha(128))
        });

        let mut pixels = vec![Rgb565::BLACK; width as usize * BAND as usize];
        let red = Paint::solid(Rgb565::RED);
        let disk = black_box(circle.with_style(Style::new().fill(red)));
        runner.time(&format!("draw {size} disk, opaque, bands"), || {
            draw_in_bands(screen, &mut pixels, &disk)
        });
        let disk = black_box(circle.with_style(Style::new().fill(red.alpha(128))));
        runner.time(
            &format!("draw {size} disk, half transparent, bands"),
            || draw_in_bands(screen, &mut pixels, &disk),
        );
        let ring = black_box(circle.with_style(Style::new().stroke(Stroke::new(16, red))));
        runner.time(&format!("draw {size} ring, width 16, bands"), || {
            draw_in_bands(screen, &mut pixels, &ring)
        });

        runner.end_group();
    }
}

fn run_self_tests() {
    assert_eq!(parse_duration("500ms").unwrap(), Duration::from_millis(500));
    assert_eq!(parse_duration("2s").unwrap(), Duration::from_secs(2));
    assert_eq!(parse_duration("1.5s").unwrap(), Duration::from_millis(1500));
    assert_eq!(parse_duration("300").unwrap(), Duration::from_millis(300));
    assert!(parse_duration("invalid").is_err());

    // Ignores cargo's default `--bench` flag
    let runner = BenchmarkRunner::from_args(["--bench".into()]).unwrap();
    assert!(runner.filters.is_empty());
    assert_eq!(runner.batch_duration, Duration::from_millis(20));
    assert!(!runner.has_custom_duration);

    let runner = BenchmarkRunner::from_args(["--bench".into(), "390x390".into(), "blend".into()]).unwrap();
    assert_eq!(runner.filters, vec!["390x390", "blend"]);
    assert_eq!(runner.batch_duration, Duration::from_millis(20));
    assert!(!runner.has_custom_duration);

    let runner = BenchmarkRunner::from_args(["--profile".into(), "disk".into()]).unwrap();
    assert_eq!(runner.filters, vec!["disk"]);
    assert_eq!(runner.batch_duration, Duration::from_millis(400));
    assert!(runner.has_custom_duration);

    let runner = BenchmarkRunner::from_args(["--duration=500ms".into()]).unwrap();
    assert_eq!(runner.batch_duration, Duration::from_millis(100));
    assert!(runner.has_custom_duration);

    let runner = BenchmarkRunner::from_args(["--duration".into(), "1s".into()]).unwrap();
    assert_eq!(runner.batch_duration, Duration::from_millis(200));
    assert!(runner.has_custom_duration);

    assert!(BenchmarkRunner::from_args(["--duration".into()]).is_err());
    assert!(BenchmarkRunner::from_args(["--duration".into(), "--profile".into()]).is_err());
    assert!(BenchmarkRunner::from_args(["--unknown-flag".into()]).is_err());

    let runner = BenchmarkRunner::from_args(["--iterations".into(), "5000".into()]).unwrap();
    assert_eq!(runner.explicit_iterations, Some(5000));

    let runner = BenchmarkRunner::from_args(["-n".into(), "10000".into()]).unwrap();
    assert_eq!(runner.explicit_iterations, Some(10000));

    let runner = BenchmarkRunner::from_args(["--iterations=25000".into()]).unwrap();
    assert_eq!(runner.explicit_iterations, Some(25000));

    assert!(BenchmarkRunner::from_args(["--iterations".into()]).is_err());
    assert!(BenchmarkRunner::from_args(["--iterations".into(), "invalid".into()]).is_err());
    assert!(BenchmarkRunner::from_args(["--iterations".into(), "--profile".into()]).is_err());

    let runner = BenchmarkRunner::from_args(["390x390".into(), "ChordWalk".into()]).unwrap();
    assert!(runner.matches("rasterize 390x390 disk, ChordWalk"));
    assert!(!runner.matches("rasterize 390x390 disk, walk"));
    assert!(!runner.matches("rasterize 1024x1024 disk, ChordWalk"));
    assert!(!runner.matches("rasterize 390x390 disk, chordwalk"));

    let runner_all = BenchmarkRunner::from_args([] as [String; 0]).unwrap();
    assert!(runner_all.matches("rasterize 390x390 disk, ChordWalk"));
    assert!(runner_all.matches("anything"));
}
