#![no_std]

pub mod coalesce;
pub mod grid;
pub mod hash;
pub mod rect;
pub mod stream;

// Public facade exports
pub use coalesce::{
    coalesce_dirty_rects, coalesce_dirty_rects_with_threshold, COALESCE_THRESHOLD_PIXELS,
};
pub use grid::{
    DamageGrid, DamageGridStorage, DamageGridStorage128x128, DamageGridStorage240x240,
    DamageGridStorage320x240,
};
pub use hash::{hash_accumulate, make_primitive_token};
pub use rect::Rect;
pub use stream::{slice_rect_to_bands, stream_dirty_bands};
