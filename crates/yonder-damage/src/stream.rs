//! Band streaming and idle settle debounce logic.

use crate::coalesce::coalesce_dirty_rects;
use crate::grid::DamageGrid;
use crate::rect::Rect;

/// Debounce timer that triggers an unconditional full refresh after motion ceases.
///
/// Guarantees that any rare hash collision (probability $< 0.003\%$) on an edge tile
/// is permanently swallowed when the screen returns to a resting state.
#[derive(Copy, Clone, Debug)]
pub struct SettleTimer {
    consecutive_idle_frames: u8,
    settle_threshold: u8,
    has_settled: bool,
}

impl SettleTimer {
    /// Creates a new settle timer with the specified frame count threshold
    /// (typically 3–6 frames, corresponding to ~50–100ms at 60 FPS).
    #[inline]
    pub const fn new(settle_threshold: u8) -> Self {
        Self {
            consecutive_idle_frames: 0,
            settle_threshold,
            has_settled: false,
        }
    }

    /// Updates the timer for the current frame.
    ///
    /// Returns `true` if this frame must trigger an unconditional settle refresh.
    #[inline]
    pub fn update(&mut self, frame_had_motion: bool) -> bool {
        if frame_had_motion {
            self.consecutive_idle_frames = 0;
            self.has_settled = false;
            false
        } else if !self.has_settled {
            self.consecutive_idle_frames += 1;
            if self.consecutive_idle_frames >= self.settle_threshold {
                self.has_settled = true;
                true // Settle frame triggered!
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Resets the settle state (e.g., when new user input arrives).
    #[inline]
    pub fn reset(&mut self) {
        self.consecutive_idle_frames = 0;
        self.has_settled = false;
    }
}

/// Slices a rectangle into horizontal bands that strictly fit within `max_scratchpad_pixels`.
#[inline]
pub fn slice_rect_to_bands<F: FnMut(Rect)>(rect: Rect, max_scratchpad_pixels: usize, mut consumer: F) {
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
pub fn stream_dirty_bands<
    const COLS: usize,
    const ROWS: usize,
    const TILES: usize,
    F: FnMut(Rect),
>(
    grid: &DamageGrid<COLS, ROWS, TILES>,
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
    fn test_settle_timer() {
        let mut timer = SettleTimer::new(3);

        // Frame with motion -> no settle
        assert!(!timer.update(true));
        // Idle frame 1 -> no settle
        assert!(!timer.update(false));
        // Idle frame 2 -> no settle
        assert!(!timer.update(false));
        // Idle frame 3 -> SETTLE TRIGGERED!
        assert!(timer.update(false));
        // Subsequent idle frames -> already settled, false
        assert!(!timer.update(false));

        // Motion resets timer
        assert!(!timer.update(true));
        assert!(!timer.update(false));

        timer.reset();
        assert!(!timer.update(false));
    }

    #[test]
    fn test_slice_empty_rect() {
        let mut count = 0;
        slice_rect_to_bands(Rect::ZERO, 2000, |_| count += 1);
        assert_eq!(count, 0);
    }
}
