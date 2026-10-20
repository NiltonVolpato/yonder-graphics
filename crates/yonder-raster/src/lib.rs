#![no_std]

pub mod blend;
pub mod conic;
pub mod scratchpad;
pub mod target;
pub mod triangle;

// Public facade exports
pub use blend::{blend_channel, blend_rgb565, blend_slice_rgb565};
pub use conic::rasterize_conic;
pub use scratchpad::ViewportScratchpad;
pub use target::CompositingTarget;
pub use triangle::rasterize_triangle;
