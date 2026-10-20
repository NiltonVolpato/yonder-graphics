//! Viewport buffer with scissoring and local coordinate translation.

use core::convert::Infallible;

use embedded_graphics_core::draw_target::DrawTarget;
use embedded_graphics_core::geometry::{OriginDimensions, Point, Size};
use embedded_graphics_core::pixelcolor::Rgb565;
use embedded_graphics_core::primitives::Rectangle;
use embedded_graphics_core::Pixel;
use yonder_damage::Rect;

use crate::blend::BlendablePixel;
use crate::target::CompositingTarget;

/// A relocatable pixel viewport buffer bound to a scissored screen rectangle.
///
/// Converts global screen coordinates $(x, y)$ to local viewport offsets
/// $(x - \text{rect.x}, y - \text{rect.y})$ and performs compositing in local memory.
pub struct Viewport<'a, C: BlendablePixel = Rgb565> {
    buffer: &'a mut [C::Storage],
    bounds: Rect,
    _color: core::marker::PhantomData<fn() -> C>,
}

impl<'a, C: BlendablePixel> Viewport<'a, C> {
    /// Binds an existing buffer slice to a target screen bounding rectangle.
    ///
    /// The buffer must be tightly packed in row-major order with capacity $\ge$ `bounds.pixel_count()`.
    /// Excess buffer capacity beyond `bounds.pixel_count()` is ignored.
    ///
    /// # Panics
    /// Panics in debug builds if `buffer.len() < bounds.pixel_count()`.
    #[inline]
    pub fn new(buffer: &'a mut [C::Storage], bounds: Rect) -> Self {
        debug_assert!(
            buffer.len() >= bounds.pixel_count(),
            "Buffer capacity ({} pixels) smaller than bounds area ({} pixels)",
            buffer.len(),
            bounds.pixel_count()
        );
        Self {
            buffer,
            bounds,
            _color: core::marker::PhantomData,
        }
    }

    /// Returns the global screen rectangle bounds this viewport is bound to.
    #[inline(always)]
    pub const fn bounds(&self) -> Rect {
        self.bounds
    }

    /// Returns an immutable reference to the raw pixel slice.
    #[inline(always)]
    pub fn buffer(&self) -> &[C::Storage] {
        self.buffer
    }

    /// Returns a mutable reference to the raw pixel slice.
    #[inline(always)]
    pub fn buffer_mut(&mut self) -> &mut [C::Storage] {
        self.buffer
    }

    /// Translates global screen coordinates $(x, y)$ to a local buffer slice index.
    /// Returns `None` if the point falls outside the viewport's bounds.
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

impl<C: BlendablePixel> OriginDimensions for Viewport<'_, C> {
    #[inline(always)]
    fn size(&self) -> Size {
        Size::new(self.bounds.width, self.bounds.height)
    }
}

impl<C: BlendablePixel> DrawTarget for Viewport<'_, C> {
    type Color = C;
    type Error = Infallible;

    #[inline(always)]
    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        self.blend_iter(pixels.into_iter().map(|Pixel(pt, color)| (pt, color, 255)))
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

    #[inline]
    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        let count = self.bounds.pixel_count().min(self.buffer.len());
        self.buffer[..count].fill(color.into_storage());
        Ok(())
    }
}

impl<C: BlendablePixel> CompositingTarget for Viewport<'_, C> {
    #[inline]
    fn blend_solid(
        &mut self,
        area: &Rectangle,
        color: Self::Color,
        alpha: u8,
    ) -> Result<(), Self::Error> {
        if alpha == 0 {
            return Ok(());
        }

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

            if alpha == 255 {
                for row in start_y..end_y {
                    let row_offset = row * w + start_x;
                    self.buffer[row_offset..row_offset + clipped_w].fill(col);
                }
            } else {
                for row in start_y..end_y {
                    let row_offset = row * w + start_x;
                    C::blend_slice(
                        &mut self.buffer[row_offset..row_offset + clipped_w],
                        color,
                        alpha,
                    );
                }
            }
        }
        Ok(())
    }

    #[inline]
    fn blend_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = (Point, Self::Color, u8)>,
    {
        for (Point { x, y }, color, alpha) in pixels {
            if alpha == 0 {
                continue;
            }
            if let Some(idx) = self.global_to_local_index(x, y) {
                if alpha == 255 {
                    self.buffer[idx] = color.into_storage();
                } else {
                    self.buffer[idx] = C::blend(self.buffer[idx], color, alpha);
                }
            }
        }
        Ok(())
    }

    #[inline]
    fn blend_contiguous<I>(&mut self, area: &Rectangle, items: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = (Self::Color, u8)>,
    {
        let mut items = items.into_iter();
        let width = area.size.width as i32;
        let height = area.size.height as i32;
        for y in 0..height {
            let global_y = area.top_left.y + y;
            for x in 0..width {
                if let Some((color, alpha)) = items.next() {
                    if alpha == 0 {
                        continue;
                    }
                    let global_x = area.top_left.x + x;
                    if let Some(idx) = self.global_to_local_index(global_x, global_y) {
                        if alpha == 255 {
                            self.buffer[idx] = color.into_storage();
                        } else {
                            self.buffer[idx] = C::blend(self.buffer[idx], color, alpha);
                        }
                    }
                } else {
                    return Ok(());
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_viewport_fill_and_blend_solid() {
        let mut mem = [0u16; 64 * 32];
        let bounds = Rect::new(100, 50, 64, 32);
        let mut viewport = Viewport::new(&mut mem, bounds);

        // Fill background green
        let green = Rgb565::new(0, 63, 0);
        viewport.clear(green).unwrap();
        assert_eq!(viewport.buffer()[0], green.into_storage());

        // Fill span/rect on row y = 50 from x = 100 to 109 with red
        let red = Rgb565::new(31, 0, 0);
        let span_rect = Rectangle::new(Point::new(100, 50), Size::new(10, 1));
        viewport.fill_solid(&span_rect, red).unwrap();
        assert_eq!(viewport.buffer()[0], red.into_storage());
        assert_eq!(viewport.buffer()[9], red.into_storage());
        assert_eq!(viewport.buffer()[10], green.into_storage());

        // Blend span on row y = 51 with 50% blue
        let blue = Rgb565::new(0, 0, 31);
        let blend_rect = Rectangle::new(Point::new(100, 51), Size::new(10, 1));
        viewport.blend_solid(&blend_rect, blue, 128).unwrap();
        let blended = viewport.buffer()[64]; // row 1, col 0
        assert_ne!(blended, green.into_storage());
        assert_ne!(blended, blue.into_storage());

        // Outside coordinates should be safely ignored
        let outside_rect = Rectangle::new(Point::new(0, 20), Size::new(200, 10));
        viewport.blend_solid(&outside_rect, red, 255).unwrap();

        // Test blend_iter
        viewport
            .blend_iter([(Point::new(105, 52), blue, 255)])
            .unwrap();
        assert_eq!(viewport.buffer()[2 * 64 + 5], blue.into_storage());

        // Test fill_contiguous and blend_contiguous
        let contig_rect = Rectangle::new(Point::new(100, 53), Size::new(2, 2));
        viewport
            .fill_contiguous(&contig_rect, [red, green, blue, red])
            .unwrap();
        assert_eq!(viewport.buffer()[3 * 64], red.into_storage());
        assert_eq!(viewport.buffer()[3 * 64 + 1], green.into_storage());
        assert_eq!(viewport.buffer()[4 * 64], blue.into_storage());
        assert_eq!(viewport.buffer()[4 * 64 + 1], red.into_storage());

        viewport
            .blend_contiguous(
                &contig_rect,
                [(blue, 128), (red, 0), (green, 255), (blue, 0)],
            )
            .unwrap();
        assert_eq!(viewport.buffer()[4 * 64], green.into_storage());
    }

    #[test]
    fn test_viewport_rgb888_fill_and_blend_solid() {
        use embedded_graphics_core::pixelcolor::Rgb888;

        let mut mem = [0u32; 64 * 32];
        let bounds = Rect::new(100, 50, 64, 32);
        let mut viewport = Viewport::<Rgb888>::new(&mut mem, bounds);

        // Fill background green
        let green = Rgb888::new(0, 255, 0);
        viewport.clear(green).unwrap();
        assert_eq!(viewport.buffer()[0], green.into_storage());

        // Fill solid red on row y = 50
        let red = Rgb888::new(255, 0, 0);
        let span_rect = Rectangle::new(Point::new(100, 50), Size::new(10, 1));
        viewport.fill_solid(&span_rect, red).unwrap();
        assert_eq!(viewport.buffer()[0], red.into_storage());
        assert_eq!(viewport.buffer()[9], red.into_storage());
        assert_eq!(viewport.buffer()[10], green.into_storage());

        // Blend blue on row y = 51 with 50% alpha
        let blue = Rgb888::new(0, 0, 255);
        let blend_rect = Rectangle::new(Point::new(100, 51), Size::new(10, 1));
        viewport.blend_solid(&blend_rect, blue, 128).unwrap();
        let blended = viewport.buffer()[64];
        assert_ne!(blended, green.into_storage());
        assert_ne!(blended, blue.into_storage());

        // Verify channel values in blended u32
        let r = (blended >> 16) & 0xFF;
        let g = (blended >> 8) & 0xFF;
        let b = blended & 0xFF;
        assert_eq!(r, 0);
        assert!((126..=129).contains(&g));
        assert!((126..=129).contains(&b));

        // Test blend_iter with Rgb888
        viewport
            .blend_iter([(Point::new(105, 52), blue, 255)])
            .unwrap();
        assert_eq!(viewport.buffer()[2 * 64 + 5], blue.into_storage());

        // Test contiguous blending with Rgb888
        let contig_rect = Rectangle::new(Point::new(100, 53), Size::new(2, 2));
        viewport
            .fill_contiguous(&contig_rect, [red, green, blue, red])
            .unwrap();
        assert_eq!(viewport.buffer()[3 * 64], red.into_storage());
        assert_eq!(viewport.buffer()[3 * 64 + 1], green.into_storage());
        assert_eq!(viewport.buffer()[4 * 64], blue.into_storage());
        assert_eq!(viewport.buffer()[4 * 64 + 1], red.into_storage());
    }
}
