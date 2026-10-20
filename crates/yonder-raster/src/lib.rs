#![no_std]

pub(crate) mod blend;
pub(crate) mod primitives;
pub(crate) mod target;
pub(crate) mod viewport;

// Canonical public facade exports (minimal single exports, no duplicate aliases)
pub use blend::BlendablePixel;
pub use primitives::{
    rasterize_conic, rasterize_conic_solid, rasterize_triangle, rasterize_triangle_solid,
    CompositingStyle, Conic, Styled, Triangle,
};
pub use target::CompositingTarget;
pub use viewport::Viewport;
