//! Viewport scratchpad buffer with scissoring and local coordinate translation.

use core::convert::Infallible;
use embedded_graphics_core::draw_target::DrawTarget;
use embedded_graphics_core::geometry::{OriginDimensions, Point, Size};
use embedded_graphics_core::pixelcolor::{IntoStorage, Rgb565};
use embedded_graphics_core::primitives::Rectangle;
use embedded_graphics_core::Pixel;

use yonder_damage::Rect;

use crate::blend::{blend_rgb565, blend_slice_rgb565};
use crate::target::CompositingTarget;

/// An SRAM-backed relocatable viewport buffer bound to a scissored screen rectangle.
///
/// Converts global screen coordinates $(x, y)$ to local scratchpad offsets
/// $(x - \text{rect.x}, y - \text{rect.y})$ and performs all compositing directly in local SRAM.
pub struct ViewportScratchpad<'a> {
    buffer: &'a mut [u16],
    bounds: Rect,
}

impl<'a> ViewportScratchpad<'a> {
    /// Binds an existing SRAM buffer slice to a target screen bounding rectangle.
    ///
    /// # Panics
    /// Panics in debug builds if `buffer.len() < bounds.pixel_count()`.
    #[inline]
    pub fn new(buffer: &'a mut [u16], bounds: Rect) -> Self {
        debug_assert!(
            buffer.len() >= bounds.pixel_count(),
            "Buffer capacity ({} pixels) smaller than bounds area ({} pixels)",
            buffer.len(),
            bounds.pixel_count()
        );
        Self { buffer, bounds }
    }

    /// Returns the global screen rectangle bounds this scratchpad is bound to.
    #[inline(always)]
    pub const fn bounds(&self) -> Rect {
        self.bounds
    }

    /// Returns an immutable reference to the raw pixel slice.
    #[inline(always)]
    pub fn buffer(&self) -> &[u16] {
        self.buffer
    }

    /// Returns a mutable reference to the raw pixel slice.
    #[inline(always)]
    pub fn buffer_mut(&mut self) -> &mut [u16] {
        self.buffer
    }

    /// Translates global screen coordinates $(x, y)$ to a local buffer slice index.
    /// Returns `None` if the point falls outside the scratchpad's bounds.
    #[inline(always)]
    pub fn global_to_local_index(&self, x: i32, y: i32) -> Option<usize> {
        if x >= self.bounds.x
            && x < self.bounds.right()
            && y >= self.bounds.y
            && y < self.bounds.bottom()
        {
            let lx = (x - self.bounds.x) as usize;
            let ly = (y - self.bounds.y) as usize;
            Some(ly * (self.bounds.width as usize) + lx)
        } else {
            None
        }
    }
}

impl OriginDimensions for ViewportScratchpad<'_> {
    #[inline(always)]
    fn size(&self) -> Size {
        Size::new(self.bounds.width, self.bounds.height)
    }
}

impl DrawTarget for ViewportScratchpad<'_> {
    type Color = Rgb565;
    type Error = Infallible;

    #[inline]
    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(Point { x, y }, color) in pixels {
            if let Some(idx) = self.global_to_local_index(x, y) {
                if idx < self.buffer.len() {
                    self.buffer[idx] = color.into_storage();
                }
            }
        }
        Ok(())
    }

    #[inline]
    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        let draw_rect = Rect::new(
            area.top_left.x,
            area.top_left.y,
            area.size.width,
            area.size.height,
        );

        if let Some(clipped) = self.bounds.intersection(&draw_rect) {
            let col = color.into_storage();
            let w = self.bounds.width as usize;
            let clipped_w = clipped.width as usize;

            let start_y = (clipped.y - self.bounds.y) as usize;
            let end_y = start_y + clipped.height as usize;
            let start_x = (clipped.x - self.bounds.x) as usize;

            for row in start_y..end_y {
                let row_offset = row * w + start_x;
                self.buffer[row_offset..row_offset + clipped_w].fill(col);
            }
        }
        Ok(())
    }

    #[inline]
    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        let count = self.bounds.pixel_count().min(self.buffer.len());
        self.buffer[..count].fill(color.into_storage());
        Ok(())
    }
}

impl CompositingTarget for ViewportScratchpad<'_> {
    #[inline]
    fn fill_span(
        &mut self,
        y: i32,
        x_start: i32,
        x_end: i32,
        color: Self::Color,
    ) -> Result<(), Self::Error> {
        if y < self.bounds.y || y >= self.bounds.bottom() {
            return Ok(());
        }

        let clamped_x1 = x_start.max(self.bounds.x);
        let clamped_x2 = x_end.min(self.bounds.right() - 1);

        if clamped_x1 <= clamped_x2 {
            let lx1 = (clamped_x1 - self.bounds.x) as usize;
            let lx2 = (clamped_x2 - self.bounds.x) as usize;
            let ly = (y - self.bounds.y) as usize;
            let row_start = ly * (self.bounds.width as usize);

            self.buffer[row_start + lx1..=row_start + lx2].fill(color.into_storage());
        }
        Ok(())
    }

    #[inline]
    fn blend_span(
        &mut self,
        y: i32,
        x_start: i32,
        x_end: i32,
        color: Self::Color,
        alpha: u8,
    ) -> Result<(), Self::Error> {
        if alpha == 0 || y < self.bounds.y || y >= self.bounds.bottom() {
            return Ok(());
        }

        let clamped_x1 = x_start.max(self.bounds.x);
        let clamped_x2 = x_end.min(self.bounds.right() - 1);

        if clamped_x1 <= clamped_x2 {
            let lx1 = (clamped_x1 - self.bounds.x) as usize;
            let lx2 = (clamped_x2 - self.bounds.x) as usize;
            let ly = (y - self.bounds.y) as usize;
            let row_start = ly * (self.bounds.width as usize);

            blend_slice_rgb565(
                &mut self.buffer[row_start + lx1..=row_start + lx2],
                color.into_storage(),
                alpha,
            );
        }
        Ok(())
    }

    #[inline]
    fn blend_pixel(
        &mut self,
        x: i32,
        y: i32,
        color: Self::Color,
        alpha: u8,
    ) -> Result<(), Self::Error> {
        if alpha == 0 {
            return Ok(());
        }

        if let Some(idx) = self.global_to_local_index(x, y) {
            if idx < self.buffer.len() {
                self.buffer[idx] = blend_rgb565(self.buffer[idx], color.into_storage(), alpha);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scratchpad_fill_and_blend_span() {
        let mut mem = [0u16; 64 * 32];
        let bounds = Rect::new(100, 50, 64, 32);
        let mut scratch = ViewportScratchpad::new(&mut mem, bounds);

        // Fill background green
        let green = Rgb565::new(0, 63, 0);
        scratch.clear(green).unwrap();
        assert_eq!(scratch.buffer()[0], green.into_storage());

        // Fill span on row y = 50 from x = 100 to 110 with red
        let red = Rgb565::new(31, 0, 0);
        scratch.fill_span(50, 100, 109, red).unwrap();
        assert_eq!(scratch.buffer()[0], red.into_storage());
        assert_eq!(scratch.buffer()[9], red.into_storage());
        assert_eq!(scratch.buffer()[10], green.into_storage());

        // Blend span on row y = 51 with 50% blue
        let blue = Rgb565::new(0, 0, 31);
        scratch.blend_span(51, 100, 109, blue, 128).unwrap();
        let blended = scratch.buffer()[64]; // row 1, col 0
        assert_ne!(blended, green.into_storage());
        assert_ne!(blended, blue.into_storage());

        // Outside coordinates should be safely ignored
        scratch.fill_span(20, 0, 200, red).unwrap();
        scratch.blend_pixel(5, 5, red, 255).unwrap();
    }
}
