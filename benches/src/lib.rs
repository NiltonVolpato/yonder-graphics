//! Benchmark suite and generic scene harness for `yonder-graphics`.

pub mod harness;
pub mod scenes;

use embedded_graphics_core::pixelcolor::{IntoStorage, Rgb565, Rgb888};
pub use harness::bench_scene_pipeline;
pub use scenes::{BouncingBallScene, VisualizerScene};
use yonder_raster::{BlendablePixel, CompositingTarget};

/// Pixel formats supported by the benchmark pipeline (e.g. `Rgb565`, `Rgb888`).
pub trait BenchmarkPixel:
    BlendablePixel + IntoStorage<Storage = Self::StorageType> + From<Rgb888>
{
    type StorageType: Copy + PartialEq + core::fmt::Debug + Into<u32> + 'static;
    fn format_name() -> &'static str;
}

impl BenchmarkPixel for Rgb565 {
    type StorageType = u16;
    fn format_name() -> &'static str {
        "Rgb565"
    }
}

impl BenchmarkPixel for Rgb888 {
    type StorageType = u32;
    fn format_name() -> &'static str {
        "Rgb888"
    }
}

/// A renderable scene that can composite into any `CompositingTarget`.
pub trait Scene<C: BenchmarkPixel> {
    /// Canonical identifier for the scene used in benchmark labels.
    fn name(&self) -> &str;

    /// Renders the scene at `frame` into `target`.
    fn render<T: CompositingTarget<Color = C>>(&self, target: &mut T, frame: usize);
}
