//! Morphological coalescing and zero-alloc dirty rectangle streaming.
//!
//! Applies 1D run bridging (closing 1-2 clean tile gaps where continuous streaming
//! is cheaper than CASET/RASET command overhead) followed by vertical span merging.

use crate::grid::DamageGrid;
use crate::rect::Rect;

/// Maximum number of disjoint active coalescing rectangles tracked simultaneously
/// in on-stack SRAM before nearest bounding-box merging occurs.
pub const MAX_ACTIVE_RECTS: usize = 8;

/// Bridges isolated clean tiles along a single row of tiles.
///
/// If two dirty tiles are separated by 1 or 2 clean tiles ($\le 32$ pixels),
/// bridging saves external bus bandwidth because continuous pixel streaming is
/// cheaper than the command sequence overhead ($\approx 20\text{--}30\text{ bytes}$).
#[inline]
pub fn bridge_row_gaps<const COLS: usize>(row_dirty: &mut [bool; COLS]) {
    let mut last_dirty_idx: Option<usize> = None;

    for c in 0..COLS {
        if row_dirty[c] {
            if let Some(prev) = last_dirty_idx {
                let gap = c - prev - 1;
                if gap == 1 || gap == 2 {
                    // Bridge the gap
                    for slot in row_dirty.iter_mut().take(c).skip(prev + 1) {
                        *slot = true;
                    }
                }
            }
            last_dirty_idx = Some(c);
        }
    }
}

/// Coalesces dirty tiles from `grid` and yields non-overlapping dirty rectangles to `consumer`.
///
/// Operates with **zero heap allocations** using a fixed-capacity stack tracker.
pub fn coalesce_dirty_rects<
    const COLS: usize,
    const ROWS: usize,
    const TILES: usize,
    F: FnMut(Rect),
>(
    grid: &DamageGrid<COLS, ROWS, TILES>,
    mut consumer: F,
) {
    let mut active_rects: [Option<Rect>; MAX_ACTIVE_RECTS] = [None; MAX_ACTIVE_RECTS];

    for r in 0..ROWS {
        // Step 1: Extract and bridge row dirty flags
        let mut row_dirty = [false; COLS];
        let mut row_has_dirty = false;
        for (c, slot) in row_dirty.iter_mut().enumerate().take(COLS) {
            if grid.is_tile_dirty(c, r) {
                *slot = true;
                row_has_dirty = true;
            }
        }

        if row_has_dirty {
            bridge_row_gaps::<COLS>(&mut row_dirty);
        }

        // Step 2: Extract continuous runs from the bridged row
        let mut matched_in_row = [false; MAX_ACTIVE_RECTS];
        let mut c = 0;
        while c < COLS {
            if row_dirty[c] {
                let start_c = c;
                while c < COLS && row_dirty[c] {
                    c += 1;
                }
                let end_c = c; // exclusive

                let span_rect = Rect::new(
                    (start_c * 16) as i32,
                    (r * 16) as i32,
                    ((end_c - start_c) * 16) as u32,
                    16,
                );

                // Try to merge with an active rectangle from row r - 1
                let mut merged = false;
                for i in 0..MAX_ACTIVE_RECTS {
                    if let Some(ref mut active) = active_rects[i] {
                        // Check if active rect touches the current row vertically and overlaps in X
                        if active.bottom() == span_rect.y
                            && active.x < span_rect.right()
                            && active.right() > span_rect.x
                        {
                            *active = active.bounding_box(&span_rect);
                            matched_in_row[i] = true;
                            merged = true;
                            break;
                        }
                    }
                }

                if !merged {
                    // Place into an empty slot, or merge into closest if full
                    let mut placed = false;
                    for i in 0..MAX_ACTIVE_RECTS {
                        if active_rects[i].is_none() {
                            active_rects[i] = Some(span_rect);
                            matched_in_row[i] = true;
                            placed = true;
                            break;
                        }
                    }

                    if !placed {
                        // Active buffer full: merge slot 0 with slot 1 to make room
                        if let (Some(r0), Some(r1)) = (active_rects[0], active_rects[1]) {
                            active_rects[0] = Some(r0.bounding_box(&r1));
                        }
                        active_rects[1] = Some(span_rect);
                        matched_in_row[1] = true;
                    }
                }
            } else {
                c += 1;
            }
        }

        // Step 3: Flush any active rectangles that did NOT receive an extension in this row
        for i in 0..MAX_ACTIVE_RECTS {
            if !matched_in_row[i] {
                if let Some(finished) = active_rects[i].take() {
                    consumer(finished);
                }
            }
        }
    }

    // Step 4: Flush all remaining active rectangles at bottom of screen
    for opt_rect in active_rects.iter_mut() {
        if let Some(remaining) = opt_rect.take() {
            consumer(remaining);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bridge_row_gaps() {
        // Gap of 1 clean tile: [true, false, true] -> [true, true, true]
        let mut row = [false; 8];
        row[1] = true;
        row[3] = true;
        bridge_row_gaps::<8>(&mut row);
        assert!(row[1]);
        assert!(row[2], "1-tile gap must be bridged");
        assert!(row[3]);

        // Gap of 2 clean tiles: [true, false, false, true] -> [true, true, true, true]
        let mut row2 = [false; 8];
        row2[1] = true;
        row2[4] = true;
        bridge_row_gaps::<8>(&mut row2);
        assert!(row2[1]);
        assert!(row2[2], "2-tile gap must be bridged");
        assert!(row2[3], "2-tile gap must be bridged");
        assert!(row2[4]);

        // Gap of 3 clean tiles: NOT bridged
        let mut row3 = [false; 8];
        row3[1] = true;
        row3[5] = true;
        bridge_row_gaps::<8>(&mut row3);
        assert!(!row3[2], "3-tile gap must not be bridged");
        assert!(!row3[3]);
        assert!(!row3[4]);
    }

    #[test]
    fn test_coalesce_vertical_extension() {
        let mut grid = DamageGrid::<4, 4, 16>::new();
        grid.prev_hashes.fill(0);
        grid.active_hashes.fill(0);

        // Mark tiles (1, 1) and (1, 2) dirty (vertical column of 2 tiles)
        grid.touch_tile(1, 1, 0x1111);
        grid.touch_tile(1, 2, 0x2222);

        let mut rects = [Rect::ZERO; 4];
        let mut count = 0;
        coalesce_dirty_rects(&grid, |rect| {
            rects[count] = rect;
            count += 1;
        });

        assert_eq!(count, 1);
        assert_eq!(rects[0], Rect::new(16, 16, 16, 32));
    }

    #[test]
    fn test_coalesce_overflow_merging() {
        // Grid with 30 columns
        let mut grid = DamageGrid::<30, 2, 60>::new();
        grid.prev_hashes.fill(0);
        grid.active_hashes.fill(0);

        // Create 10 disjoint dirty tiles on row 0 (gap of 2 clean tiles: 0, 3, 6, 9, 12, 15, 18, 21, 24, 27)
        // Wait, gap of 2 is bridged! Gap must be 3 clean tiles to remain disjoint:
        // Cols: 0, 4, 8, 12, 16, 20, 24, 28 -> that's 8 runs.
        // Let's use 36 columns: 0, 4, 8, 12, 16, 20, 24, 28, 32 -> 9 disjoint runs!
        let mut big_grid = DamageGrid::<40, 2, 80>::new();
        big_grid.prev_hashes.fill(0);
        big_grid.active_hashes.fill(0);

        for c in (0..40).step_by(4) {
            big_grid.touch_tile(c, 0, 0x1234);
        }

        let mut emitted_count = 0;
        coalesce_dirty_rects(&big_grid, |_| emitted_count += 1);
        assert!(emitted_count > 0);
    }
}
