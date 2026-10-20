//! Queue-driven morphological coalescing using an intrusive linked-list queue in `active_hashes`.
//!
//! Replaces arbitrary slot limits with in-place buffer reuse:
//! - Unqueued clean tiles: `0`
//! - Unqueued dirty tiles: `1`
//! - Queued seeds: `ptr >= 2` (points to `(ptr - 2)` as next queued index)
//! - Tombstoned seeds: `ptr | 0x8000` (absorbed while queued; next link preserved)
//! - End of queue: `0x7FFF` (`NULL`)

use crate::grid::DamageGrid;
use crate::rect::Rect;

/// Maximum extra clean pixels we are willing to transmit in order to merge two spans
/// vertically and avoid a separate SPI address-window command transaction.
///
/// 512 pixels = 2 tiles ($16 \times 16$), corresponding to the cost of an SPI command burst.
pub const COALESCE_THRESHOLD_PIXELS: usize = 512;

const TOMBSTONE_FLAG: u16 = 0x8000;
const PTR_MASK: u16 = 0x7FFF;
const NULL: u16 = 0x7FFF;

#[inline]
fn is_dirty(val: u16) -> bool {
    val != 0 && (val & TOMBSTONE_FLAG) == 0
}

#[inline]
fn absorb_tile(hashes: &mut [u16], idx: usize) {
    let val = hashes[idx];
    if (val & TOMBSTONE_FLAG) != 0 {
        // Already tombstoned: preserve existing link intact
        return;
    }
    if val >= 2 {
        // Currently queued seed: tombstone it, preserving next pointer
        hashes[idx] = val | TOMBSTONE_FLAG;
    } else {
        // Unqueued dirty (1) or clean (0)
        hashes[idx] = 0;
    }
}

#[inline]
fn enqueue(hashes: &mut [u16], head: &mut u16, tail: &mut u16, idx: usize) {
    if hashes[idx] >= 2 {
        // Already queued or tombstoned
        return;
    }
    let ptr = (idx + 2) as u16;
    hashes[idx] = NULL;
    if *head == NULL {
        *head = ptr;
        *tail = ptr;
    } else {
        let prev_tail_idx = ((*tail & PTR_MASK) - 2) as usize;
        hashes[prev_tail_idx] = (hashes[prev_tail_idx] & TOMBSTONE_FLAG) | ptr;
        *tail = ptr;
    }
}

#[inline]
fn dequeue(hashes: &mut [u16], head: &mut u16, tail: &mut u16) -> Option<usize> {
    while *head != NULL {
        let idx = ((*head & PTR_MASK) - 2) as usize;
        let entry = hashes[idx];
        let next_ptr = entry & PTR_MASK;
        let is_tombstone = (entry & TOMBSTONE_FLAG) != 0;

        *head = next_ptr;
        if *head == NULL {
            *tail = NULL;
        }

        if is_tombstone {
            // Lazy unlink: reset slot to 0 and skip to next queued entry
            hashes[idx] = 0;
        } else {
            // Active seed: reset slot to 1 so grow_rect can scan it
            hashes[idx] = 1;
            return Some(idx);
        }
    }
    None
}

/// Scans a contiguous dirty run starting at `c` along row `r`, bridging 1-2 clean tile gaps.
/// Returns `(start_c, end_c)` where `end_c` is exclusive.
#[inline]
fn scan_row_span(hashes: &[u16], cols: usize, r: usize, mut c: usize) -> (usize, usize) {
    let start_c = c;
    let row_offset = r * cols;

    while c < cols {
        let idx = row_offset + c;
        if is_dirty(hashes[idx]) {
            c += 1;
        } else if c + 1 < cols && is_dirty(hashes[row_offset + c + 1]) {
            // Bridge 1-tile gap
            c += 2;
        } else if c + 2 < cols && is_dirty(hashes[row_offset + c + 2]) {
            // Bridge 2-tile gap
            c += 3;
        } else {
            break;
        }
    }
    (start_c, c)
}

#[allow(clippy::too_many_arguments)]
fn grow_rect(
    hashes: &mut [u16],
    cols: usize,
    rows: usize,
    threshold_pixels: usize,
    head: &mut u16,
    tail: &mut u16,
    start_c: usize,
    start_r: usize,
) -> Rect {
    let (span_start, span_end) = scan_row_span(hashes, cols, start_r, start_c);
    let tile_size = DamageGrid::TILE_SIZE as i32;

    let mut active_c_start = span_start;
    let mut active_c_end = span_end;
    let active_r_start = start_r;
    let mut active_r_end = start_r + 1; // exclusive

    let mut dirty_pixels = (span_end - span_start) * (tile_size as usize * tile_size as usize);
    let mut accumulated_waste: usize = 0;

    // Mark absorbed tiles on this row
    for c in span_start..span_end {
        absorb_tile(hashes, start_r * cols + c);
    }

    // Scan right on this row for any additional disjoint spans and enqueue them
    let mut right_c = span_end;
    while right_c < cols {
        if is_dirty(hashes[start_r * cols + right_c]) {
            let (r_start, r_end) = scan_row_span(hashes, cols, start_r, right_c);
            enqueue(hashes, head, tail, start_r * cols + r_start);
            right_c = r_end;
        } else {
            right_c += 1;
        }
    }

    // Downward Expansion: examine rows below
    let mut curr_r = start_r + 1;
    while curr_r < rows {
        let row_offset = curr_r * cols;
        let mut merged_in_row = false;
        let mut c = 0;

        while c < cols {
            if is_dirty(hashes[row_offset + c]) {
                let (s_start, s_end) = scan_row_span(hashes, cols, curr_r, c);

                // Check if this span overlaps or touches active_rect's horizontal bounds
                let overlaps = s_start < active_c_end && s_end > active_c_start;

                if overlaps && !merged_in_row {
                    let cand_c_start = active_c_start.min(s_start);
                    let cand_c_end = active_c_end.max(s_end);
                    let cand_h = ((curr_r - active_r_start + 1) as i32) * tile_size;
                    let cand_w = ((cand_c_end - cand_c_start) as i32) * tile_size;
                    let cand_area = (cand_w as usize) * (cand_h as usize);

                    let span_pixels = (s_end - s_start) * (tile_size as usize * tile_size as usize);
                    let extra_waste = cand_area.saturating_sub(dirty_pixels + span_pixels);

                    if accumulated_waste + extra_waste <= threshold_pixels {
                        // Merge downward!
                        active_c_start = cand_c_start;
                        active_c_end = cand_c_end;
                        active_r_end = curr_r + 1;
                        dirty_pixels += span_pixels;
                        accumulated_waste += extra_waste;
                        merged_in_row = true;

                        // Mark absorbed tiles
                        for sc in s_start..s_end {
                            absorb_tile(hashes, row_offset + sc);
                        }
                    } else {
                        // Too expensive to merge; enqueue if on the right
                        if s_start >= active_c_end {
                            enqueue(hashes, head, tail, row_offset + s_start);
                        }
                    }
                } else if s_start >= active_c_end {
                    // Unrelated span on the right -> defer to queue
                    enqueue(hashes, head, tail, row_offset + s_start);
                }

                c = s_end;
            } else {
                c += 1;
            }
        }

        if !merged_in_row {
            // Downward expansion stopped
            break;
        }

        curr_r += 1;
    }

    Rect::new(
        (active_c_start as i32) * tile_size,
        (active_r_start as i32) * tile_size,
        ((active_c_end - active_c_start) as u32) * (tile_size as u32),
        ((active_r_end - active_r_start) as u32) * (tile_size as u32),
    )
}

/// Coalesces dirty tiles in `grid` into minimal rectangular update windows, streaming them to `consumer`.
///
/// Operates with **zero heap allocations** and **zero extra stack arrays** by repurposing
/// `grid.active_hashes` in-place as an intrusive FIFO queue.
pub fn coalesce_dirty_rects<F: FnMut(Rect)>(grid: &mut DamageGrid<'_>, consumer: F) {
    coalesce_dirty_rects_with_threshold(grid, COALESCE_THRESHOLD_PIXELS, consumer);
}

/// Coalesces dirty tiles with a custom maximum extra clean pixel threshold.
pub fn coalesce_dirty_rects_with_threshold<F: FnMut(Rect)>(
    grid: &mut DamageGrid<'_>,
    threshold_pixels: usize,
    mut consumer: F,
) {
    let cols = grid.cols;
    let rows = grid.rows;
    let total_tiles = cols * rows;

    if total_tiles == 0 {
        return;
    }

    // Phase 1: In-place diff & hash transfer
    // Active hashes are copied to prev_hashes (for next frame) while in-place marking:
    // 0 = clean, 1 = dirty
    for i in 0..total_tiles {
        let is_dirty = grid.active_hashes[i] != grid.prev_hashes[i];
        grid.prev_hashes[i] = grid.active_hashes[i];
        grid.active_hashes[i] = if is_dirty { 1 } else { 0 };
    }

    let hashes = &mut grid.active_hashes[..total_tiles];
    let mut head: u16 = NULL;
    let mut tail: u16 = NULL;

    // Phase 2: Processing loop
    // Process seeds from the grid and from the intrusive queue
    for r in 0..rows {
        for c in 0..cols {
            // First, drain any pending items in the queue
            while let Some(queued_idx) = dequeue(hashes, &mut head, &mut tail) {
                let qr = queued_idx / cols;
                let qc = queued_idx % cols;
                let rect = grow_rect(
                    hashes,
                    cols,
                    rows,
                    threshold_pixels,
                    &mut head,
                    &mut tail,
                    qc,
                    qr,
                );
                consumer(rect);
            }

            // Then check the current grid tile
            let idx = r * cols + c;
            if is_dirty(hashes[idx]) {
                let rect = grow_rect(
                    hashes,
                    cols,
                    rows,
                    threshold_pixels,
                    &mut head,
                    &mut tail,
                    c,
                    r,
                );
                consumer(rect);
            }
        }
    }

    // Drain any remaining items in queue
    while let Some(queued_idx) = dequeue(hashes, &mut head, &mut tail) {
        let qr = queued_idx / cols;
        let qc = queued_idx % cols;
        let rect = grow_rect(
            hashes,
            cols,
            rows,
            threshold_pixels,
            &mut head,
            &mut tail,
            qc,
            qr,
        );
        consumer(rect);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::DamageGridStorage;

    #[test]
    fn test_coalesce_single_and_horizontal_run() {
        let mut storage = DamageGridStorage::<4, 4>::new();
        let mut grid = storage.as_grid();
        grid.prev_hashes.fill(0);
        grid.active_hashes.fill(0);

        // Mark tiles (1, 1) and (2, 1) dirty
        grid.accumulate_tile(1, 1, 0x1111);
        grid.accumulate_tile(2, 1, 0x2222);

        let mut rects = [Rect::ZERO; 4];
        let mut count = 0;
        coalesce_dirty_rects(&mut grid, |rect| {
            rects[count] = rect;
            count += 1;
        });

        assert_eq!(count, 1);
        assert_eq!(rects[0], Rect::new(16, 16, 32, 16));
        // All active hashes should be cleanly zeroed for the next frame
        assert_eq!(grid.active_hashes[4 + 1], 0);
        assert_eq!(grid.active_hashes[4 + 2], 0);
    }

    #[test]
    fn test_coalesce_vertical_extension() {
        let mut storage = DamageGridStorage::<4, 4>::new();
        let mut grid = storage.as_grid();
        grid.prev_hashes.fill(0);
        grid.active_hashes.fill(0);

        // Mark tiles (1, 1) and (1, 2) dirty (vertical column of 2 tiles)
        grid.accumulate_tile(1, 1, 0x1111);
        grid.accumulate_tile(1, 2, 0x2222);

        let mut rects = [Rect::ZERO; 4];
        let mut count = 0;
        coalesce_dirty_rects(&mut grid, |rect| {
            rects[count] = rect;
            count += 1;
        });

        assert_eq!(count, 1);
        assert_eq!(rects[0], Rect::new(16, 16, 16, 32));
    }

    #[test]
    fn test_coalesce_disjoint_right_spans_not_lost() {
        // Row 0 has Icon A at col 1 and Icon B at col 5 (gap of 3 clean tiles: 2, 3, 4 -> not bridged)
        let mut storage = DamageGridStorage::<7, 3>::new();
        let mut grid = storage.as_grid();
        grid.prev_hashes.fill(0);
        grid.active_hashes.fill(0);

        grid.accumulate_tile(1, 0, 0xAAAA);
        grid.accumulate_tile(5, 0, 0xBBBB);

        let mut rects = [Rect::ZERO; 4];
        let mut count = 0;
        coalesce_dirty_rects(&mut grid, |rect| {
            rects[count] = rect;
            count += 1;
        });

        // Must emit TWO separate rectangles, neither lost, neither merged across cols 2..4
        assert_eq!(count, 2);
        assert_eq!(rects[0], Rect::new(16, 0, 16, 16));
        assert_eq!(rects[1], Rect::new(80, 0, 16, 16));
    }

    #[test]
    fn test_coalesce_gap_bridging() {
        // Gap of 1 clean tile: cols 1 and 3 are dirty, col 2 is clean
        let mut storage = DamageGridStorage::<5, 2>::new();
        let mut grid = storage.as_grid();
        grid.prev_hashes.fill(0);
        grid.active_hashes.fill(0);

        grid.accumulate_tile(1, 0, 0x1111);
        grid.accumulate_tile(3, 0, 0x3333);

        let mut rects = [Rect::ZERO; 4];
        let mut count = 0;
        coalesce_dirty_rects(&mut grid, |rect| {
            rects[count] = rect;
            count += 1;
        });

        assert_eq!(count, 1);
        // Spans col 1..4 (3 tiles = 48 px)
        assert_eq!(rects[0], Rect::new(16, 0, 48, 16));
    }

    #[test]
    fn test_coalesce_absorbed_enqueued_seed_does_not_panic() {
        // Reproduces the exact multi-span interaction from Frame 1 where downward
        // expansion absorbs previously enqueued right-hand seeds.
        let mut storage = DamageGridStorage::<23, 23>::new();
        let mut grid = storage.as_grid();
        grid.prev_hashes.fill(0);
        grid.active_hashes.fill(0);

        // Row 8: cols 16..21
        for c in 16..21 {
            grid.accumulate_tile(c, 8, 0x1111);
        }
        // Row 13: col 6
        grid.accumulate_tile(6, 13, 0x2222);
        // Row 14: cols 5..8 and cols 12..18
        for c in 5..8 {
            grid.accumulate_tile(c, 14, 0x3333);
        }
        for c in 12..18 {
            grid.accumulate_tile(c, 14, 0x4444);
        }
        // Row 15: cols 5..9 and cols 12..18
        for c in 5..9 {
            grid.accumulate_tile(c, 15, 0x5555);
        }
        for c in 12..18 {
            grid.accumulate_tile(c, 15, 0x6666);
        }
        // Row 16: cols 5..9 and cols 12..18
        for c in 5..9 {
            grid.accumulate_tile(c, 16, 0x7777);
        }
        for c in 12..18 {
            grid.accumulate_tile(c, 16, 0x8888);
        }
        // Row 17: cols 5..9
        for c in 5..9 {
            grid.accumulate_tile(c, 17, 0x9999);
        }
        // Row 18: cols 4..9
        for c in 4..9 {
            grid.accumulate_tile(c, 18, 0xAAAA);
        }

        let mut rects = [Rect::ZERO; 8];
        let mut count = 0;
        coalesce_dirty_rects(&mut grid, |rect| {
            if count < rects.len() {
                rects[count] = rect;
            }
            count += 1;
        });

        // Exactly 5 coalesced update windows:
        // - Row 8 badge
        // - Right needle/shape (fully coalesced rows 14..17 without corruption from absorbed seeds)
        // - 3 non-wasteful vertical segments for the curved ball
        assert_eq!(count, 5);
        // Active hashes must be cleanly reset to 0
        for &h in grid.active_hashes.iter() {
            assert_eq!(h, 0);
        }
    }
}
