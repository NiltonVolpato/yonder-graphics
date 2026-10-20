use criterion::{criterion_group, criterion_main, Criterion};
use embedded_graphics_core::pixelcolor::{Rgb565, Rgb888};
use yonder_benches::{bench_scene_pipeline, BouncingBallScene, VisualizerScene};

fn bench_pipeline(c: &mut Criterion) {
    // Frame 20 represents an airborne phase with high-velocity vertical stretch,
    // dynamic shadow, and active needle and badge translation.
    const FRAME_AIRBORNE: usize = 20;

    // 1. Canonical Visualizer Scene (Full multi-primitive composition)
    bench_scene_pipeline::<Rgb565, _>(c, &VisualizerScene, FRAME_AIRBORNE);
    bench_scene_pipeline::<Rgb888, _>(c, &VisualizerScene, FRAME_AIRBORNE);

    // 2. Focused Bouncing Ball Scene (Conic ellipse deformation + alpha shadow)
    bench_scene_pipeline::<Rgb565, _>(c, &BouncingBallScene, FRAME_AIRBORNE);
    bench_scene_pipeline::<Rgb888, _>(c, &BouncingBallScene, FRAME_AIRBORNE);
}

criterion_group!(benches, bench_pipeline);
criterion_main!(benches);
