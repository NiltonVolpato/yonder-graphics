//! Lightweight profiling binary for `samply` and `cargo-flamegraph`.
//!
//! Executes pure tight hot loops of selected benchmark scenes without Criterion statistical overhead.
//! Frames advance sequentially (`0, 1, 2, ...`) across the animation sequence.
//!
//! Usage:
//! ```bash
//! cargo run --release -p yonder-benches --bin profile [target] [iterations]
//! # or with samply:
//! samply record target/release/profile visualizer-e2e-565 100000
//! ```

use std::hint::black_box;
use std::time::Instant;

use embedded_graphics_core::pixelcolor::{Rgb565, Rgb888};
use yonder_benches::scenes::{BouncingBallScene, VisualizerScene};
use yonder_benches::{BenchmarkPixel, Scene};
use yonder_damage::{coalesce_dirty_rects, DamageGridStorage360x360, Rect};
use yonder_raster::{BlendablePixel, Viewport};

const DEFAULT_ITERATIONS: usize = 100_000;
const CANVAS_WIDTH: u32 = 360;
const CANVAS_HEIGHT: u32 = 360;
const MAX_GRID_PIXELS: usize =
    (DamageGridStorage360x360::WIDTH * DamageGridStorage360x360::HEIGHT) as usize;

/// End-to-End Pipeline: Phase 1 (DamageGrid) -> Coalescing -> Phase 2 (Viewport per dirty rect).
fn profile_e2e<C: BenchmarkPixel, S: Scene<C>>(label: &str, scene: &S, iterations: usize) {
    let mut storage = DamageGridStorage360x360::new();
    let mut grid = storage.as_grid();
    let clear_storage = BlendablePixel::into_storage(C::from(Rgb888::new(0, 0, 0)));
    let mut dma_scratch = vec![clear_storage; MAX_GRID_PIXELS].into_boxed_slice();
    let mut rect_collector = Vec::with_capacity(16);

    println!(
        "Profiling End-to-End {} for {} sequential frames (0..{})...",
        label, iterations, iterations
    );
    let start = Instant::now();
    let mut total_rects = 0;

    for frame in 0..iterations {
        // Phase 1: Damage grid accumulation
        scene.render(&mut grid.as_target::<C>(), black_box(frame));

        // Coalesce dirty rects
        rect_collector.clear();
        coalesce_dirty_rects(&mut grid, |rect| {
            rect_collector.push(rect);
        });
        total_rects += rect_collector.len();

        // Phase 2: Render each dirty rect into Viewport
        for &rect in &rect_collector {
            let mut viewport = Viewport::<C>::new(&mut dma_scratch[..rect.pixel_count()], rect);
            scene.render(&mut viewport, black_box(frame));
            black_box(&dma_scratch[0]);
        }
    }

    let elapsed = start.elapsed();
    let us_per_frame = (elapsed.as_secs_f64() * 1e6) / (iterations as f64);
    println!(
        "Done: {:?} total ({:.3} µs/frame, {:.0} FPS, avg {:.1} rects/frame)",
        elapsed,
        us_per_frame,
        1e6 / us_per_frame,
        total_rects as f64 / iterations as f64
    );
}

/// Phase 1 only: DamageGrid hash accumulation + rect coalescing across sequential frames.
fn profile_phase1<C: BenchmarkPixel, S: Scene<C>>(label: &str, scene: &S, iterations: usize) {
    let mut storage = DamageGridStorage360x360::new();
    let mut grid = storage.as_grid();

    println!(
        "Profiling Phase 1 {} for {} sequential frames (0..{})...",
        label, iterations, iterations
    );
    let start = Instant::now();
    for frame in 0..iterations {
        scene.render(&mut grid.as_target::<C>(), black_box(frame));
        let mut count = 0;
        coalesce_dirty_rects(&mut grid, |rect| {
            count += 1;
            black_box(rect);
        });
        black_box(count);
    }
    let elapsed = start.elapsed();
    let us_per_frame = (elapsed.as_secs_f64() * 1e6) / (iterations as f64);
    println!(
        "Done: {:?} total ({:.3} µs/frame, {:.0} FPS)",
        elapsed,
        us_per_frame,
        1e6 / us_per_frame
    );
}

/// Phase 2 full frame only: Viewport rasterization over entire 360x360 canvas across sequential frames.
fn profile_phase2_full<C: BenchmarkPixel, S: Scene<C>>(label: &str, scene: &S, iterations: usize) {
    let full_bounds = Rect::new(0, 0, CANVAS_WIDTH, CANVAS_HEIGHT);
    let clear_storage = BlendablePixel::into_storage(C::from(Rgb888::new(0, 0, 0)));
    let mut full_buffer =
        vec![clear_storage; (CANVAS_WIDTH * CANVAS_HEIGHT) as usize].into_boxed_slice();

    println!(
        "Profiling Phase 2 Full {} for {} sequential frames (0..{})...",
        label, iterations, iterations
    );
    let start = Instant::now();
    for frame in 0..iterations {
        let mut viewport = Viewport::<C>::new(&mut full_buffer, full_bounds);
        scene.render(&mut viewport, black_box(frame));
        black_box(&full_buffer[0]);
    }
    let elapsed = start.elapsed();
    let us_per_frame = (elapsed.as_secs_f64() * 1e6) / (iterations as f64);
    println!(
        "Done: {:?} total ({:.3} µs/frame, {:.0} FPS)",
        elapsed,
        us_per_frame,
        1e6 / us_per_frame
    );
}

fn print_usage() {
    println!("Usage: profile [target] [iterations]");
    println!("Targets:");
    println!(
        "  visualizer-e2e-565          (default: End-to-End Phase 1 + Phase 2 per dirty rect)"
    );
    println!("  visualizer-e2e-888");
    println!("  ball-e2e-565");
    println!("  ball-e2e-888");
    println!("  visualizer-phase1-565       (Phase 1 only: DamageGrid + Coalesce)");
    println!("  visualizer-phase1-888");
    println!("  visualizer-phase2-full-565  (Phase 2 only: unculled full 360x360 draw)");
    println!("  visualizer-phase2-full-888");
    println!("  ball-phase1-565");
    println!("  ball-phase1-888");
    println!("  ball-phase2-full-565");
    println!("  ball-phase2-full-888");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let target = args
        .get(1)
        .map(|s| s.as_str())
        .unwrap_or("visualizer-e2e-565");
    let iterations = args
        .get(2)
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(DEFAULT_ITERATIONS);

    match target {
        "visualizer-e2e-565" | "e2e-565" | "e2e" => {
            profile_e2e::<Rgb565, _>("visualizer_scene/Rgb565/e2e", &VisualizerScene, iterations);
        }
        "visualizer-e2e-888" | "e2e-888" => {
            profile_e2e::<Rgb888, _>("visualizer_scene/Rgb888/e2e", &VisualizerScene, iterations);
        }
        "ball-e2e-565" | "ball-e2e" => {
            profile_e2e::<Rgb565, _>(
                "bouncing_ball_scene/Rgb565/e2e",
                &BouncingBallScene,
                iterations,
            );
        }
        "ball-e2e-888" => {
            profile_e2e::<Rgb888, _>(
                "bouncing_ball_scene/Rgb888/e2e",
                &BouncingBallScene,
                iterations,
            );
        }
        "visualizer-phase1-565" | "v1-565" => {
            profile_phase1::<Rgb565, _>(
                "visualizer_scene/Rgb565/phase1",
                &VisualizerScene,
                iterations,
            );
        }
        "visualizer-phase1-888" | "v1-888" => {
            profile_phase1::<Rgb888, _>(
                "visualizer_scene/Rgb888/phase1",
                &VisualizerScene,
                iterations,
            );
        }
        "visualizer-phase2-full-565" | "v2-full-565" => {
            profile_phase2_full::<Rgb565, _>(
                "visualizer_scene/Rgb565/phase2_full",
                &VisualizerScene,
                iterations,
            );
        }
        "visualizer-phase2-full-888" | "v2-full-888" => {
            profile_phase2_full::<Rgb888, _>(
                "visualizer_scene/Rgb888/phase2_full",
                &VisualizerScene,
                iterations,
            );
        }
        "ball-phase1-565" | "b1-565" => {
            profile_phase1::<Rgb565, _>(
                "bouncing_ball_scene/Rgb565/phase1",
                &BouncingBallScene,
                iterations,
            );
        }
        "ball-phase1-888" | "b1-888" => {
            profile_phase1::<Rgb888, _>(
                "bouncing_ball_scene/Rgb888/phase1",
                &BouncingBallScene,
                iterations,
            );
        }
        "ball-phase2-full-565" | "b2-full-565" => {
            profile_phase2_full::<Rgb565, _>(
                "bouncing_ball_scene/Rgb565/phase2_full",
                &BouncingBallScene,
                iterations,
            );
        }
        "ball-phase2-full-888" | "b2-full-888" => {
            profile_phase2_full::<Rgb888, _>(
                "bouncing_ball_scene/Rgb888/phase2_full",
                &BouncingBallScene,
                iterations,
            );
        }
        "--help" | "-h" | "help" => {
            print_usage();
        }
        other => {
            eprintln!("Unknown target: {}", other);
            print_usage();
            std::process::exit(1);
        }
    }
}
