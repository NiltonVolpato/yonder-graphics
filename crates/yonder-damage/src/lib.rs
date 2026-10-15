#![no_std]

pub mod coalesce;
pub mod grid;
pub mod hash;
pub mod rect;
pub mod stream;

// Public facade exports
pub use coalesce::{bridge_row_gaps, coalesce_dirty_rects, MAX_ACTIVE_RECTS};
pub use grid::{DamageGrid, DamageGrid128x128, DamageGrid240x240, DamageGrid320x240};
pub use hash::{hash_accumulate, make_primitive_token};
pub use rect::Rect;
pub use stream::{slice_rect_to_bands, stream_dirty_bands, SettleTimer};
