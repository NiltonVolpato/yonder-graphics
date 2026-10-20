//! Generic pipeline benchmark runner for Criterion.

use core::hint::black_box;

use criterion::Criterion;
use embedded_graphics_core::pixelcolor::Rgb888;
use yonder_damage::{coalesce_dirty_rects, DamageGridStorage360x360, Rect};
use yonder_raster::{BlendablePixel, Viewport};

use crate::{BenchmarkPixel, Scene};

/// Benchmarks Phase 1 damage grid accumulation, Phase 2 full-frame rendering, and Phase 2 single dirty rect rendering.
pub fn bench_scene_pipeline<C: BenchmarkPixel, S: Scene<C>>(
    c: &mut Criterion,
    scene: &S,
    frame: usize,
) {
    let group_name = format!("{}/{}", scene.name(), C::format_name());
    let mut group = c.benchmark_group(group_name);

    let prev_frame = frame.saturating_sub(1);

    // Probe the primary dirty rectangle for this frame transition (frame - 1 -> frame)
    let mut storage_probe = DamageGridStorage360x360::new();
    let mut grid_probe = storage_probe.as_grid();
    scene.render(&mut grid_probe.as_target::<C>(), prev_frame);
    coalesce_dirty_rects(&mut grid_probe, |_| {});

    scene.render(&mut grid_probe.as_target::<C>(), frame);
    let mut primary_rect = Rect::ZERO;
    coalesce_dirty_rects(&mut grid_probe, |rect| {
        if rect.pixel_count() > primary_rect.pixel_count() {
            primary_rect = rect;
        }
    });

    if primary_rect.is_empty() {
        primary_rect = Rect::new(80, 200, 48, 48);
    }

    // 1. Phase 1: Damage Grid Hash Accumulation + Rect Coalescing
    // Alternates between frame and prev_frame to benchmark continuous active animation damage
    let mut storage = DamageGridStorage360x360::new();
    let mut grid = storage.as_grid();
    scene.render(&mut grid.as_target::<C>(), prev_frame);
    coalesce_dirty_rects(&mut grid, |_| {});

    let mut target_frame = frame;
    group.bench_function("phase1_damage_grid", |b| {
        b.iter(|| {
            scene.render(&mut grid.as_target::<C>(), black_box(target_frame));
            let mut count = 0;
            coalesce_dirty_rects(&mut grid, |rect| {
                count += 1;
                black_box(rect);
            });
            target_frame = if target_frame == frame {
                prev_frame
            } else {
                frame
            };
            black_box(count);
        });
    });

    // 2. Phase 2: Full Image Viewport Rendering (360x360 = 129,600 pixels)
    let full_bounds = Rect::new(0, 0, 360, 360);
    let clear_storage = BlendablePixel::into_storage(C::from(Rgb888::new(0, 0, 0)));
    let mut full_buffer = vec![clear_storage; 360 * 360];

    group.bench_function("phase2_full_image_360x360", |b| {
        b.iter(|| {
            let mut viewport = Viewport::<C>::new(&mut full_buffer, full_bounds);
            scene.render(&mut viewport, black_box(frame));
            black_box(&full_buffer[0]);
        });
    });

    // 3. Phase 2: Single Dirty Rectangle Viewport Rendering (scissored to primary rect)
    let rect_pixels = primary_rect.pixel_count();
    let mut rect_buffer = vec![clear_storage; rect_pixels];
    let rect_label = format!(
        "phase2_single_dirty_rect_{}x{}",
        primary_rect.width, primary_rect.height
    );

    group.bench_function(&rect_label, |b| {
        b.iter(|| {
            let mut viewport = Viewport::<C>::new(&mut rect_buffer, primary_rect);
            scene.render(&mut viewport, black_box(frame));
            black_box(&rect_buffer[0]);
        });
    });

    group.finish();
}
