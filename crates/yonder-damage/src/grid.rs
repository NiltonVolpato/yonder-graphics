//! 16x16 spatial tile damage grid with double-buffered 16-bit hash tracking.

use crate::hash::hash_accumulate;
use crate::rect::Rect;

/// A spatial grid of 16x16 pixel tiles tracking damage between successive frames.
///
/// Uses double-buffered 16-bit hash accumulators. For a 320x240 display:
/// - Grid dimensions: 20 cols x 15 rows = 300 tiles.
/// - Hash table footprint: $300 \times 2 \text{ bytes} = \mathbf{600 \text{ bytes of SRAM}}$.
#[derive(Copy, Clone, Debug)]
pub struct DamageGrid<const COLS: usize, const ROWS: usize, const TILES: usize> {
    pub active_hashes: [u16; TILES],
    pub prev_hashes: [u16; TILES],
}

impl<const COLS: usize, const ROWS: usize, const TILES: usize> DamageGrid<COLS, ROWS, TILES> {
    pub const TILE_SIZE: u32 = 16;
    pub const WIDTH: u32 = (COLS as u32) * Self::TILE_SIZE;
    pub const HEIGHT: u32 = (ROWS as u32) * Self::TILE_SIZE;

    /// Creates a new damage grid with all tiles cleared.
    #[inline]
    pub const fn new() -> Self {
        Self {
            active_hashes: [0u16; TILES],
            prev_hashes: [0xFFFFu16; TILES], // Initialize to mismatch so initial frame is dirty
        }
    }

    /// Resets all active tile hashes to zero for the start of a new frame pass.
    #[inline]
    pub fn reset_active(&mut self) {
        self.active_hashes.fill(0);
    }

    /// Accumulates a hash token into the specified tile `(col, row)`.
    #[inline]
    pub fn touch_tile(&mut self, col: usize, row: usize, token: u16) {
        if col < COLS && row < ROWS {
            let idx = row * COLS + col;
            if idx < TILES {
                self.active_hashes[idx] = hash_accumulate(self.active_hashes[idx], token);
            }
        }
    }

    /// Accumulates a hash token across all 16x16 tiles intersected by `rect`.
    #[inline]
    pub fn touch_rect(&mut self, rect: Rect, token: u16) {
        if rect.is_empty() || rect.x >= Self::WIDTH as i32 || rect.y >= Self::HEIGHT as i32 {
            return;
        }

        let right = rect.right();
        let bottom = rect.bottom();
        if right <= 0 || bottom <= 0 {
            return;
        }

        let c_start = (rect.x.max(0) as u32 / Self::TILE_SIZE) as usize;
        let c_end = (((right - 1).max(0) as u32 / Self::TILE_SIZE) as usize).min(COLS - 1);

        let r_start = (rect.y.max(0) as u32 / Self::TILE_SIZE) as usize;
        let r_end = (((bottom - 1).max(0) as u32 / Self::TILE_SIZE) as usize).min(ROWS - 1);

        for r in r_start..=r_end {
            let row_offset = r * COLS;
            for c in c_start..=c_end {
                let idx = row_offset + c;
                if idx < TILES {
                    self.active_hashes[idx] = hash_accumulate(self.active_hashes[idx], token);
                }
            }
        }
    }

    /// Returns true if the tile at `(col, row)` was modified since the previous frame.
    #[inline(always)]
    pub fn is_tile_dirty(&self, col: usize, row: usize) -> bool {
        if col < COLS && row < ROWS {
            let idx = row * COLS + col;
            if idx < TILES {
                return self.active_hashes[idx] != self.prev_hashes[idx];
            }
        }
        false
    }

    /// Copies the active hashes to the previous hashes and clears active hashes for the next frame.
    #[inline]
    pub fn swap_buffers(&mut self) {
        self.prev_hashes.copy_from_slice(&self.active_hashes);
        self.active_hashes.fill(0);
    }

    /// Forces all tiles to be marked dirty on the next comparison.
    #[inline]
    pub fn mark_all_dirty(&mut self) {
        for prev in self.prev_hashes.iter_mut() {
            *prev = prev.wrapping_add(1);
        }
    }
}

impl<const COLS: usize, const ROWS: usize, const TILES: usize> Default
    for DamageGrid<COLS, ROWS, TILES>
{
    fn default() -> Self {
        Self::new()
    }
}

/// Standard 320x240 display damage grid (20x15 tiles, 600 bytes SRAM footprint).
pub type DamageGrid320x240 = DamageGrid<20, 15, 300>;

/// Standard 240x240 square display damage grid (15x15 tiles, 450 bytes SRAM footprint).
pub type DamageGrid240x240 = DamageGrid<15, 15, 225>;

/// Standard 128x128 display damage grid (8x8 tiles, 128 bytes SRAM footprint).
pub type DamageGrid128x128 = DamageGrid<8, 8, 64>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_damage_grid_touch_rect() {
        let mut grid = DamageGrid320x240::new();
        // Clear initial mismatch so we can test incremental touches
        grid.prev_hashes.fill(0);
        grid.active_hashes.fill(0);

        // Touch a rect covering tile (1, 1): x in [16, 31], y in [16, 31]
        let rect = Rect::new(16, 16, 16, 16);
        grid.touch_rect(rect, 0x1234);

        assert!(grid.is_tile_dirty(1, 1));
        assert!(!grid.is_tile_dirty(0, 0));
        assert!(!grid.is_tile_dirty(2, 2));

        // Touch crossing tiles (0, 0) and (1, 0): x=8, y=8, w=16, h=8 -> spans x: 8..24
        grid.touch_rect(Rect::new(8, 8, 16, 8), 0x5678);
        assert!(grid.is_tile_dirty(0, 0));
        assert!(grid.is_tile_dirty(1, 0));

        // Out of bounds and empty rects should not panic or modify
        grid.touch_rect(Rect::ZERO, 0xFFFF);
        grid.touch_rect(Rect::new(1000, 1000, 50, 50), 0xFFFF);
        grid.touch_rect(Rect::new(-100, -100, 50, 50), 0xFFFF);
        grid.touch_rect(Rect::new(-10, -10, 20, 20), 0x9999);
    }

    #[test]
    fn test_damage_grid_mark_all_dirty_and_default() {
        let mut grid = DamageGrid320x240::default();
        grid.swap_buffers();
        assert!(!grid.is_tile_dirty(0, 0));

        grid.mark_all_dirty();
        assert!(grid.is_tile_dirty(0, 0));
        assert!(grid.is_tile_dirty(19, 14));
    }
}
