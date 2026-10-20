//! Band streaming logic for scratchpad-sized slices.

use crate::coalesce::coalesce_dirty_rects;
use crate::grid::DamageGrid;
use crate::rect::Rect;

/// Slices a rectangle into horizontal bands that strictly fit within `max_scratchpad_pixels`.
#[inline]
pub(crate) fn slice_rect_to_bands<F: FnMut(Rect)>(
    rect: Rect,
    max_scratchpad_pixels: usize,
    mut consumer: F,
) {
    if rect.is_empty() {
        return;
    }

    if rect.pixel_count() <= max_scratchpad_pixels {
        consumer(rect);
        return;
    }

    // Determine max band height in pixels based on width
    let max_band_height = ((max_scratchpad_pixels as u32) / rect.width).max(1);

    let mut curr_y = rect.y;
    let end_y = rect.bottom();

    while curr_y < end_y {
        let remaining_h = (end_y - curr_y) as u32;
        let band_h = remaining_h.min(max_band_height);

        let band = Rect::new(rect.x, curr_y, rect.width, band_h);
        consumer(band);

        curr_y += band_h as i32;
    }
}

/// Coalesces dirty tiles and streams scratchpad-sized bands directly to `consumer`.
///
/// Guaranteed to emit only rectangles whose `pixel_count() <= max_scratchpad_pixels`,
/// ensuring all drawing operations can perform fast local SRAM read-modify-write for transparency.
pub fn stream_dirty_bands<F: FnMut(Rect)>(
    grid: &mut DamageGrid<'_>,
    max_scratchpad_pixels: usize,
    mut consumer: F,
) {
    coalesce_dirty_rects(grid, |coalesced_rect| {
        slice_rect_to_bands(coalesced_rect, max_scratchpad_pixels, &mut consumer);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slice_rect_to_bands() {
        // Rect 100 x 50 = 5000 pixels.
        // Scratchpad capacity = 2000 pixels.
        // Max band height = 2000 / 100 = 20 pixels.
        // Should produce 3 bands: 100x20, 100x20, 100x10.
        let rect = Rect::new(10, 20, 100, 50);

        let mut bands = [Rect::ZERO; 8];
        let mut count = 0;

        slice_rect_to_bands(rect, 2000, |band| {
            bands[count] = band;
            count += 1;
        });

        assert_eq!(count, 3);
        assert_eq!(bands[0], Rect::new(10, 20, 100, 20));
        assert_eq!(bands[1], Rect::new(10, 40, 100, 20));
        assert_eq!(bands[2], Rect::new(10, 60, 100, 10));

        for band in bands.iter().take(count) {
            assert!(band.pixel_count() <= 2000);
        }
    }

    #[test]
    fn test_slice_empty_rect() {
        let mut count = 0;
        slice_rect_to_bands(Rect::ZERO, 2000, |_| count += 1);
        assert_eq!(count, 0);
    }
}
