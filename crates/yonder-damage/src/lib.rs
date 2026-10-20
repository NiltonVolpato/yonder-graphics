#![no_std]

pub(crate) mod coalesce;
pub(crate) mod grid;
pub(crate) mod hash;
pub(crate) mod rect;
pub(crate) mod stream;

// Public facade exports (single canonical export location per AGENTS.md)
pub use coalesce::{
    coalesce_dirty_rects, coalesce_dirty_rects_with_threshold, COALESCE_THRESHOLD_PIXELS,
};
pub use grid::{
    DamageGrid, DamageGridStorage, DamageGridStorage128x128, DamageGridStorage240x240,
    DamageGridStorage320x240, DamageGridStorage360x360, DamageTarget,
};
pub use hash::{hash_solid, TileHasher};
pub use rect::Rect;
pub use stream::stream_dirty_bands;
