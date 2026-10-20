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
    DamageGridStorage320x240, DamageGridStorage360x360, DamageTarget,
};
pub use hash::{
    hash_accumulate, hash_solid, TileHasher, TOOL_ID_CONTIGUOUS, TOOL_ID_PIXEL_ITER, TOOL_ID_SHAPE,
    TOOL_ID_SOLID_RECT,
};
pub use rect::Rect;
pub use stream::{slice_rect_to_bands, stream_dirty_bands};
