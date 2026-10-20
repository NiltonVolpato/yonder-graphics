//! 16x16 spatial tile damage grid with double-buffered 16-bit hash tracking.

use embedded_graphics_core::draw_target::DrawTarget;
use embedded_graphics_core::geometry::{OriginDimensions, Size};
use embedded_graphics_core::pixelcolor::{IntoStorage, PixelColor, Rgb565};
use embedded_graphics_core::primitives::Rectangle;
use embedded_graphics_core::Pixel;

use crate::hash::{
    hash_accumulate, hash_solid, TileHasher, TOOL_ID_CONTIGUOUS, TOOL_ID_PIXEL_ITER, TOOL_ID_SHAPE,
    TOOL_ID_SOLID_RECT,
};
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
            #[cfg(debug_assertions)]
            accumulate_count: 0,
        }
    }
}

impl<const COLS: usize, const ROWS: usize> Default for DamageGridStorage<COLS, ROWS> {
    fn default() -> Self {
        Self::new()
    }
}

/// Canonical 360x360 display damage grid storage (23x23 tiles of 16x16, 1058 bytes SRAM footprint).
///
/// Exercises edge clipping on the 8-pixel partial border (pixels 352..360).
pub type DamageGridStorage360x360 = DamageGridStorage<23, 23>;

/// Standard 320x240 display damage grid storage (20x15 tiles, 600 bytes SRAM footprint).
pub type DamageGridStorage320x240 = DamageGridStorage<20, 15>;

/// Standard 240x240 square display damage grid storage (15x15 tiles, 450 bytes SRAM footprint).
pub type DamageGridStorage240x240 = DamageGridStorage<15, 15>;

/// Standard 128x128 display damage grid storage (8x8 tiles, 128 bytes SRAM footprint).
pub type DamageGridStorage128x128 = DamageGridStorage<8, 8>;

/// A non-generic borrowed view over a spatial grid of 16x16 pixel tiles tracking damage between successive frames.
#[derive(Debug)]
pub struct DamageGrid<'a> {
    pub(crate) cols: usize,
    pub(crate) rows: usize,
    pub(crate) active_hashes: &'a mut [u16],
    pub(crate) prev_hashes: &'a mut [u16],
    #[cfg(debug_assertions)]
    accumulate_count: usize,
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
            #[cfg(debug_assertions)]
            accumulate_count: 0,
        }
    }

    /// Wraps this grid in a `DamageTarget<'_, 'a, C>` for an arbitrary pixel color type `C`.
    #[inline]
    pub fn as_target<'t, C>(&'t mut self) -> DamageTarget<'t, 'a, C> {
        DamageTarget {
            grid: self,
            _color: core::marker::PhantomData,
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

    /// Returns an immutable slice of active tile hashes.
    #[inline(always)]
    pub fn active_hashes(&self) -> &[u16] {
        self.active_hashes
    }

    /// Returns a mutable slice of active tile hashes.
    #[inline(always)]
    pub fn active_hashes_mut(&mut self) -> &mut [u16] {
        self.active_hashes
    }

    /// Returns an immutable slice of previous tile hashes.
    #[inline(always)]
    pub fn prev_hashes(&self) -> &[u16] {
        self.prev_hashes
    }

    /// Returns a mutable slice of previous tile hashes.
    #[inline(always)]
    pub fn prev_hashes_mut(&mut self) -> &mut [u16] {
        self.prev_hashes
    }

    /// Returns mutable slices to both active and previous tile hashes simultaneously.
    #[inline(always)]
    pub fn hashes_mut(&mut self) -> (&mut [u16], &mut [u16]) {
        (self.active_hashes, self.prev_hashes)
    }

    #[inline(always)]
    pub fn tile_count(&self) -> usize {
        self.cols * self.rows
    }

    /// Resets all active tile hashes to zero for the start of a new frame pass.
    #[inline]
    pub fn reset_active(&mut self) {
        let count = self.tile_count();
        self.active_hashes[..count].fill(0);
    }

    /// Resets all active tile hashes to the solid color hash for a full background clear.
    #[inline]
    pub fn clear_solid(&mut self, color: u16) {
        let solid = hash_solid(color);
        let count = self.tile_count();
        self.active_hashes[..count].fill(solid);
    }

    /// Resets a single tile to a solid opaque color (full-tile overwrite).
    #[inline]
    pub fn set_tile_solid(&mut self, col: usize, row: usize, color: u16) {
        if col < self.cols && row < self.rows {
            let idx = row * self.cols + col;
            self.active_hashes[idx] = hash_solid(color);
        }
    }

    /// Accumulates a partial shape token into the tile hash at `(col, row)` without bounds checking.
    #[inline(always)]
    pub(crate) fn accumulate_tile_unchecked(&mut self, col: usize, row: usize, token: u16) {
        #[cfg(debug_assertions)]
        {
            self.accumulate_count += 1;
        }
        let idx = row * self.cols + col;
        self.active_hashes[idx] = hash_accumulate(self.active_hashes[idx], token);
    }

    /// Accumulates a partial shape token into the tile hash at `(col, row)`.
    #[inline]
    pub fn accumulate_tile(&mut self, col: usize, row: usize, token: u16) {
        if col < self.cols && row < self.rows {
            self.accumulate_tile_unchecked(col, row, token);
        }
    }

    /// Returns the number of times `accumulate_tile` has been called (debug only).
    #[cfg(debug_assertions)]
    #[doc(hidden)]
    #[inline(always)]
    pub fn accumulate_count(&self) -> usize {
        self.accumulate_count
    }

    /// Resets the accumulate invocation counter to zero (debug only).
    #[cfg(debug_assertions)]
    #[doc(hidden)]
    #[inline(always)]
    pub fn reset_accumulate_count(&mut self) {
        self.accumulate_count = 0;
    }

    /// Fills a rectangular tile range with a solid opaque color.
    ///
    /// `c_end` and `r_end` are exclusive column and row boundaries.
    #[inline]
    pub fn fill_tiles_solid(
        &mut self,
        c_start: usize,
        c_end: usize,
        r_start: usize,
        r_end: usize,
        color: u16,
    ) {
        let solid = hash_solid(color);
        let r_limit = r_end.min(self.rows);
        let c_limit = c_end.min(self.cols);
        for r in r_start..r_limit {
            let row_offset = r * self.cols;
            for c in c_start..c_limit {
                self.active_hashes[row_offset + c] = solid;
            }
        }
    }

    /// Returns the inclusive range of tile indices `(c_start, c_end, r_start, r_end)`
    /// intersected by `rect`, or `None` if `rect` does not intersect the grid or is empty.
    #[inline]
    pub(crate) fn tile_range(&self, rect: Rect) -> Option<(usize, usize, usize, usize)> {
        let grid_rect = Rect::new(0, 0, self.width(), self.height());
        let clipped = rect.intersection(&grid_rect)?;
        if clipped.is_empty() {
            return None;
        }

        let c_start = (clipped.x as u32 / Self::TILE_SIZE) as usize;
        let c_end = ((clipped.right() - 1) as u32 / Self::TILE_SIZE) as usize;
        let r_start = (clipped.y as u32 / Self::TILE_SIZE) as usize;
        let r_end = ((clipped.bottom() - 1) as u32 / Self::TILE_SIZE) as usize;

        Some((
            c_start,
            c_end.min(self.cols.saturating_sub(1)),
            r_start,
            r_end.min(self.rows.saturating_sub(1)),
        ))
    }

    /// Accumulates a hash token across all 16x16 tiles intersected by `rect`.
    #[inline]
    pub fn touch_rect(&mut self, rect: Rect, token: u16) {
        if let Some((c_start, c_end, r_start, r_end)) = self.tile_range(rect) {
            for r in r_start..=r_end {
                for c in c_start..=c_end {
                    self.accumulate_tile_unchecked(c, r, token);
                }
            }
        }
    }

    /// Returns true if the tile at `(col, row)` was modified since the previous frame.
    #[inline(always)]
    pub fn is_tile_dirty(&self, col: usize, row: usize) -> bool {
        if col < self.cols && row < self.rows {
            let idx = row * self.cols + col;
            self.active_hashes[idx] != self.prev_hashes[idx]
        } else {
            false
        }
    }

    /// Copies the active hashes to the previous hashes and clears active hashes for the next frame.
    #[inline]
    pub fn swap_buffers(&mut self) {
        let count = self.tile_count();
        self.prev_hashes[..count].copy_from_slice(&self.active_hashes[..count]);
        self.active_hashes[..count].fill(0);
    }

    /// Forces all tiles to be marked dirty on the next comparison.
    #[inline]
    pub fn mark_all_dirty(&mut self) {
        let count = self.tile_count();
        for prev in self.prev_hashes[..count].iter_mut() {
            *prev = prev.wrapping_add(1);
        }
    }
}

/// A typed drawing target adapter wrapping a [`DamageGrid`] for an arbitrary pixel color `C`.
#[derive(Debug)]
pub struct DamageTarget<'t, 'a, C = Rgb565> {
    pub(crate) grid: &'t mut DamageGrid<'a>,
    pub(crate) _color: core::marker::PhantomData<fn() -> C>,
}

impl<'t, 'a, C> OriginDimensions for DamageTarget<'t, 'a, C> {
    #[inline(always)]
    fn size(&self) -> Size {
        Size::new(self.grid.width(), self.grid.height())
    }
}

impl<'t, 'a, C> DamageTarget<'t, 'a, C>
where
    C: PixelColor + IntoStorage,
    C::Storage: Into<u32>,
{
    /// Blends a solid rectangular area with a uniform color and alpha factor.
    pub fn blend_solid(
        &mut self,
        area: &Rectangle,
        color: C,
        alpha: u8,
    ) -> Result<(), core::convert::Infallible> {
        if alpha == 0 {
            return Ok(());
        }

        let Some((c_start, c_end, r_start, r_end)) = self.grid.tile_range(Rect::from(area)) else {
            return Ok(());
        };

        let raw: u32 = color.into_storage().into();
        let top_left = area.top_left;
        let right = top_left.x + area.size.width as i32;
        let bottom = top_left.y + area.size.height as i32;

        if alpha == 255 {
            let color_hash = hash_solid((raw ^ (raw >> 16)) as u16);
            let token = TileHasher::new(TOOL_ID_SOLID_RECT)
                .write_u32(raw)
                .write_i32(top_left.x)
                .write_i32(top_left.y)
                .write_u32(area.size.width)
                .write_u32(area.size.height)
                .finish();

            for r in r_start..=r_end {
                let tile_y = (r as u32 * DamageGrid::TILE_SIZE) as i32;
                let tile_bottom = tile_y + DamageGrid::TILE_SIZE as i32;
                let row_fully_covered = top_left.y <= tile_y && bottom >= tile_bottom;

                for c in c_start..=c_end {
                    let tile_x = (c as u32 * DamageGrid::TILE_SIZE) as i32;
                    let tile_right = tile_x + DamageGrid::TILE_SIZE as i32;
                    let col_fully_covered = top_left.x <= tile_x && right >= tile_right;

                    if row_fully_covered && col_fully_covered {
                        self.grid.active_hashes[r * self.grid.cols + c] = color_hash;
                    } else {
                        self.grid.accumulate_tile_unchecked(c, r, token);
                    }
                }
            }
        } else {
            let token = TileHasher::new(TOOL_ID_SHAPE)
                .write_u32(raw)
                .write_u8(alpha)
                .write_i32(top_left.x)
                .write_i32(top_left.y)
                .write_u32(area.size.width)
                .write_u32(area.size.height)
                .finish();
            for r in r_start..=r_end {
                for c in c_start..=c_end {
                    self.grid.accumulate_tile_unchecked(c, r, token);
                }
            }
        }
        Ok(())
    }

    /// Blends an iterator of individual pixels with their respective alpha factors.
    pub fn blend_iter<I>(&mut self, pixels: I) -> Result<(), core::convert::Infallible>
    where
        I: IntoIterator<Item = (embedded_graphics_core::geometry::Point, C, u8)>,
    {
        let mut last_tile = (usize::MAX, usize::MAX);
        for (point, color, alpha) in pixels {
            if alpha > 0 && point.x >= 0 && point.y >= 0 {
                let col = (point.x as u32 / DamageGrid::TILE_SIZE) as usize;
                let row = (point.y as u32 / DamageGrid::TILE_SIZE) as usize;
                if col < self.grid.cols && row < self.grid.rows && (col, row) != last_tile {
                    let raw: u32 = color.into_storage().into();
                    let token = TileHasher::new(TOOL_ID_PIXEL_ITER)
                        .write_u32(raw)
                        .write_u8(alpha)
                        .finish();
                    self.grid.accumulate_tile_unchecked(col, row, token);
                    last_tile = (col, row);
                }
            }
        }
        Ok(())
    }

    /// Blends contiguous pixel colors and alphas over a rectangular area in row-major order.
    pub fn blend_contiguous<I>(
        &mut self,
        area: &Rectangle,
        items: I,
    ) -> Result<(), core::convert::Infallible>
    where
        I: IntoIterator<Item = (C, u8)>,
    {
        let Some((c_start, c_end, r_start, r_end)) = self.grid.tile_range(Rect::from(area)) else {
            return Ok(());
        };

        let mut hasher = TileHasher::new(TOOL_ID_CONTIGUOUS);
        let mut has_visible = false;
        for (color, alpha) in items {
            if alpha > 0 {
                has_visible = true;
            }
            let raw: u32 = color.into_storage().into();
            hasher = hasher.write_u32(raw).write_u8(alpha);
        }
        if !has_visible {
            return Ok(());
        }
        let token = hasher.finish();

        for r in r_start..=r_end {
            for c in c_start..=c_end {
                self.grid.accumulate_tile_unchecked(c, r, token);
            }
        }
        Ok(())
    }
}

impl<'t, 'a, C> DrawTarget for DamageTarget<'t, 'a, C>
where
    C: PixelColor + IntoStorage,
    C::Storage: Into<u32>,
{
    type Color = C;
    type Error = core::convert::Infallible;

    #[inline(always)]
    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        self.blend_iter(pixels.into_iter().map(|Pixel(pt, c)| (pt, c, 255)))
    }

    #[inline(always)]
    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        self.blend_solid(area, color, 255)
    }

    #[inline(always)]
    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        self.blend_contiguous(area, colors.into_iter().map(|c| (c, 255)))
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        let raw: u32 = color.into_storage().into();
        self.grid.clear_solid((raw ^ (raw >> 16)) as u16);
        Ok(())
    }
}

impl<'a> OriginDimensions for DamageGrid<'a> {
    #[inline(always)]
    fn size(&self) -> Size {
        Size::new(self.width(), self.height())
    }
}

impl<'a> DrawTarget for DamageGrid<'a> {
    type Color = Rgb565;
    type Error = core::convert::Infallible;

    #[inline(always)]
    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        self.as_target::<Rgb565>().draw_iter(pixels)
    }

    #[inline(always)]
    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        self.as_target::<Rgb565>().fill_solid(area, color)
    }

    #[inline(always)]
    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        self.as_target::<Rgb565>().fill_contiguous(area, colors)
    }

    #[inline(always)]
    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        self.as_target::<Rgb565>().clear(color)
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

    #[test]
    fn test_damage_grid_solid_resets_and_accumulate() {
        let mut storage = DamageGridStorage320x240::new();
        let mut grid = storage.as_grid();
        grid.prev_hashes.fill(0);
        grid.active_hashes.fill(0);

        // Clear entire grid to white
        let white = 0xFFFF;
        grid.clear_solid(white);
        let expected_white_hash = hash_solid(white);
        assert_eq!(grid.active_hashes[0], expected_white_hash);
        assert_eq!(grid.active_hashes[10], expected_white_hash);

        // Accumulate partial token on tile (2, 2)
        grid.accumulate_tile(2, 2, 0x1234);
        assert_ne!(grid.active_hashes[2 * 20 + 2], expected_white_hash);

        // Overwrite tile (2, 2) with solid red
        let red = 0xF800;
        grid.set_tile_solid(2, 2, red);
        assert_eq!(grid.active_hashes[2 * 20 + 2], hash_solid(red));

        // Fill a block of tiles solid green
        let green = 0x07E0;
        grid.fill_tiles_solid(3, 6, 1, 4, green);
        let green_hash = hash_solid(green);
        for r in 1..4 {
            for c in 3..6 {
                assert_eq!(grid.active_hashes[r * 20 + c], green_hash);
            }
        }
    }

    #[test]
    fn test_damage_grid_draw_target_embedded_graphics() {
        use embedded_graphics_core::geometry::Point;
        use embedded_graphics_core::pixelcolor::Rgb888;
        use embedded_graphics_core::primitives::Rectangle;

        let mut storage = DamageGridStorage360x360::new();
        let mut grid = storage.as_grid();
        grid.prev_hashes.fill(0);
        grid.active_hashes.fill(0);

        // Clear with embedded-graphics DrawTarget::clear
        let white = Rgb565::new(31, 63, 31);
        grid.clear(white).unwrap();
        assert_eq!(grid.active_hashes[0], hash_solid(0xFFFF));

        // Fill solid rect covering tile (1, 1) strictly: [16, 16] to [32, 32]
        let red = Rgb565::new(31, 0, 0);
        let rect = Rectangle::new(Point::new(16, 16), Size::new(16, 16));
        grid.fill_solid(&rect, red).unwrap();

        // Tile (1, 1) should be solid red hash!
        assert_eq!(grid.active_hashes[23 + 1], hash_solid(0xF800));

        // Test with Rgb888 via as_target
        let blue888 = Rgb888::new(0, 0, 255);
        let rect2 = Rectangle::new(Point::new(32, 32), Size::new(16, 16));
        {
            let mut target888 = grid.as_target::<Rgb888>();
            target888.fill_solid(&rect2, blue888).unwrap();
            target888.blend_solid(&rect2, blue888, 128).unwrap();
            target888
                .blend_iter([(Point::new(10, 10), blue888, 200)])
                .unwrap();
            let contig_rect = Rectangle::new(Point::new(48, 48), Size::new(16, 16));
            target888
                .blend_contiguous(&contig_rect, [(blue888, 150)])
                .unwrap();
        }
        assert_ne!(grid.active_hashes[2 * 23 + 2], 0);
    }

    #[test]
    fn test_damage_grid_fill_contiguous_ssot() {
        use embedded_graphics_core::geometry::Point;
        use embedded_graphics_core::primitives::Rectangle;

        let mut storage1 = DamageGridStorage360x360::new();
        let mut grid1 = storage1.as_grid();
        grid1.active_hashes.fill(0);
        grid1.prev_hashes.fill(0);

        let mut storage2 = DamageGridStorage360x360::new();
        let mut grid2 = storage2.as_grid();
        grid2.active_hashes.fill(0);
        grid2.prev_hashes.fill(0);

        let rect = Rectangle::new(Point::new(16, 16), Size::new(16, 16));
        let colors = [Rgb565::new(31, 0, 0); 256];

        // Path 1: DrawTarget::fill_contiguous directly on grid1
        grid1
            .fill_contiguous(&rect, colors.iter().copied())
            .unwrap();

        // Path 2: DamageTarget::fill_contiguous via as_target on grid2
        grid2
            .as_target::<Rgb565>()
            .fill_contiguous(&rect, colors.iter().copied())
            .unwrap();

        // Both must produce the EXACT same active hashes!
        assert_eq!(grid1.active_hashes, grid2.active_hashes);
        assert_ne!(grid1.active_hashes[23 + 1], 0);
    }

    #[test]
    fn test_damage_target_blend_contiguous_zero_alpha() {
        use embedded_graphics_core::geometry::Point;
        use embedded_graphics_core::primitives::Rectangle;

        let mut storage = DamageGridStorage360x360::new();
        let mut grid = storage.as_grid();
        grid.active_hashes.fill(0);
        grid.prev_hashes.fill(0);

        let rect = Rectangle::new(Point::new(16, 16), Size::new(16, 16));
        let items = [(Rgb565::new(31, 0, 0), 0u8); 256];

        grid.as_target::<Rgb565>()
            .blend_contiguous(&rect, items)
            .unwrap();

        // When all alphas are 0, tile must NOT be marked dirty or modified
        assert_eq!(grid.active_hashes[23 + 1], 0);
        assert!(!grid.is_tile_dirty(1, 1));
    }
}
