#![no_std]

pub mod blend;
pub mod primitives;
pub mod target;
pub mod viewport;

// Canonical public facade exports (minimal single exports, no duplicate aliases)
pub use blend::{
    blend_channel, blend_rgb565, blend_slice_rgb565, blend_slice_xrgb8888, blend_xrgb8888,
    BlendablePixel,
};
pub use primitives::{
    rasterize_conic, rasterize_conic_solid, rasterize_triangle, rasterize_triangle_solid,
    CompositingStyle, Conic, Styled, Triangle,
};
pub use target::CompositingTarget;
pub use viewport::Viewport;
