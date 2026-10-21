//! Scanline edge-equation, conic, and oriented projective primitives and styles.

pub mod conic;
pub mod style;
pub mod triangle;

pub use conic::{rasterize_conic, rasterize_conic_aa, rasterize_conic_solid, Conic};
pub use style::{CompositingStyle, Styled};
pub use triangle::{rasterize_triangle, rasterize_triangle_solid, Triangle};
