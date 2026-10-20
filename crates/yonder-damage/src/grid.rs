//! 16x16 spatial tile damage grid with double-buffered 16-bit hash tracking.

use crate::hash::hash_accumulate;
use crate::rect::Rect;

/// Owning storage buffer for a damage grid, holding active and previous 16-bit hash arrays.
///
/// Uses contiguous 2D arrays `[[u16; COLS]; ROWS]` so that `COLS` and `ROWS` are directly
/// stored without requiring an additional `TILES` const generic.
#[derive(Copy, Clone, Debug)]
pub struct DamageGridStorage<const COLS: usize, const ROWS: usize> {
    pub active_hashes: [[u16; COLS]; ROWS],
    pub prev_hashes: [[u16; COLS]; ROWS],
}

impl<const COLS: usize, const ROWS: usize> DamageGridStorage<COLS, ROWS> {
    pub const TILE_SIZE: u32 = 16;
    pub const WIDTH: u32 = (COLS as u32) * Self::TILE_SIZE;
    pub const HEIGHT: u32 = (ROWS as u32) * Self::TILE_SIZE;

    /// Creates a new storage buffer with all tiles initialized to a dirty mismatch state.
    #[inline]
    pub const fn new() -> Self {
        Self {
            active_hashes: [[0u16; COLS]; ROWS],
            prev_hashes: [[0xFFFFu16; COLS]; ROWS],
        }
    }

    /// Borrows this storage as a non-generic `DamageGrid<'a>` view.
    #[inline]
    pub fn as_grid(&mut self) -> DamageGrid<'_> {
        DamageGrid {
            cols: COLS,
            rows: ROWS,
            active_hashes: self.active_hashes.as_flattened_mut(),
            prev_hashes: self.prev_hashes.as_flattened_mut(),
        }
    }
}

impl<const COLS: usize, const ROWS: usize> Default for DamageGridStorage<COLS, ROWS> {
    fn default() -> Self {
        Self::new()
    }
}

/// Standard 320x240 display damage grid storage (20x15 tiles, 600 bytes SRAM footprint).
pub type DamageGridStorage320x240 = DamageGridStorage<20, 15>;

/// Standard 240x240 square display damage grid storage (15x15 tiles, 450 bytes SRAM footprint).
pub type DamageGridStorage240x240 = DamageGridStorage<15, 15>;

/// Standard 128x128 display damage grid storage (8x8 tiles, 128 bytes SRAM footprint).
pub type DamageGridStorage128x128 = DamageGridStorage<8, 8>;

/// A non-generic borrowed view over a spatial grid of 16x16 pixel tiles tracking damage between successive frames.
#[derive(Debug)]
pub struct DamageGrid<'a> {
    pub cols: usize,
    pub rows: usize,
    pub active_hashes: &'a mut [u16],
    pub prev_hashes: &'a mut [u16],
}

impl<'a> DamageGrid<'a> {
    pub const TILE_SIZE: u32 = 16;

    /// Creates a damage grid view bound to external hash slices.
    ///
    /// # Panics
    /// Panics in debug builds if `active_hashes.len() < cols * rows` or `prev_hashes.len() < cols * rows`.
    #[inline]
    pub fn new(
        cols: usize,
        rows: usize,
        active_hashes: &'a mut [u16],
        prev_hashes: &'a mut [u16],
    ) -> Self {
        debug_assert!(active_hashes.len() >= cols * rows);
        debug_assert!(prev_hashes.len() >= cols * rows);
        Self {
            cols,
            rows,
            active_hashes,
            prev_hashes,
        }
    }

    #[inline(always)]
    pub fn width(&self) -> u32 {
        (self.cols as u32) * Self::TILE_SIZE
    }

    #[inline(always)]
    pub fn height(&self) -> u32 {
        (self.rows as u32) * Self::TILE_SIZE
    }

    #[inline(always)]
    pub fn tile_count(&self) -> usize {
        self.cols * self.rows
    }

    /// Resets all active tile hashes to zero for the start of a new frame pass.
    #[inline]
    pub fn reset_active(&mut self) {
        let count = self.tile_count().min(self.active_hashes.len());
        self.active_hashes[..count].fill(0);
    }

    /// Accumulates a hash token into the specified tile `(col, row)`.
    #[inline]
    pub fn touch_tile(&mut self, col: usize, row: usize, token: u16) {
        if col < self.cols && row < self.rows {
            let idx = row * self.cols + col;
            if idx < self.active_hashes.len() {
                self.active_hashes[idx] = hash_accumulate(self.active_hashes[idx], token);
            }
        }
    }

    /// Accumulates a hash token across all 16x16 tiles intersected by `rect`.
    #[inline]
    pub fn touch_rect(&mut self, rect: Rect, token: u16) {
        let width = self.width();
        let height = self.height();
        if rect.is_empty() || rect.x >= width as i32 || rect.y >= height as i32 {
            return;
        }

        let right = rect.right();
        let bottom = rect.bottom();
        if right <= 0 || bottom <= 0 {
            return;
        }

        let c_start = (rect.x.max(0) as u32 / Self::TILE_SIZE) as usize;
        let c_end = (((right - 1).max(0) as u32 / Self::TILE_SIZE) as usize).min(self.cols - 1);

        let r_start = (rect.y.max(0) as u32 / Self::TILE_SIZE) as usize;
        let r_end = (((bottom - 1).max(0) as u32 / Self::TILE_SIZE) as usize).min(self.rows - 1);

        for r in r_start..=r_end {
            let row_offset = r * self.cols;
            for c in c_start..=c_end {
                let idx = row_offset + c;
                if idx < self.active_hashes.len() {
                    self.active_hashes[idx] = hash_accumulate(self.active_hashes[idx], token);
                }
            }
        }
    }

    /// Returns true if the tile at `(col, row)` was modified since the previous frame.
    #[inline(always)]
    pub fn is_tile_dirty(&self, col: usize, row: usize) -> bool {
        if col < self.cols && row < self.rows {
            let idx = row * self.cols + col;
            if idx < self.active_hashes.len() && idx < self.prev_hashes.len() {
                return self.active_hashes[idx] != self.prev_hashes[idx];
            }
        }
        false
    }

    /// Copies the active hashes to the previous hashes and clears active hashes for the next frame.
    #[inline]
    pub fn swap_buffers(&mut self) {
        let count = self
            .tile_count()
            .min(self.active_hashes.len())
            .min(self.prev_hashes.len());
        self.prev_hashes[..count].copy_from_slice(&self.active_hashes[..count]);
        self.active_hashes[..count].fill(0);
    }

    /// Forces all tiles to be marked dirty on the next comparison.
    #[inline]
    pub fn mark_all_dirty(&mut self) {
        let count = self.tile_count().min(self.prev_hashes.len());
        for prev in self.prev_hashes[..count].iter_mut() {
            *prev = prev.wrapping_add(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_damage_grid_touch_rect() {
        let mut storage = DamageGridStorage320x240::new();
        let mut grid = storage.as_grid();
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
        let mut storage = DamageGridStorage320x240::default();
        let mut grid = storage.as_grid();
        grid.swap_buffers();
        assert!(!grid.is_tile_dirty(0, 0));

        grid.mark_all_dirty();
        assert!(grid.is_tile_dirty(0, 0));
        assert!(grid.is_tile_dirty(19, 14));
    }
}
