//! `CompositingTarget` implementation for `DamageGrid` and `DamageTarget`.

use embedded_graphics_core::geometry::Point;
use embedded_graphics_core::pixelcolor::{IntoStorage, PixelColor, Rgb565};
use embedded_graphics_core::primitives::Rectangle;
use yonder_damage::{DamageGrid, DamageTarget};

use super::CompositingTarget;

impl<'t, 'a, C> CompositingTarget for DamageTarget<'t, 'a, C>
where
    C: PixelColor + IntoStorage,
    C::Storage: Into<u32>,
{
    #[inline(always)]
    fn blend_solid(
        &mut self,
        area: &Rectangle,
        color: Self::Color,
        alpha: u8,
    ) -> Result<(), Self::Error> {
        self.blend_solid(area, color, alpha)
    }

    #[inline(always)]
    fn blend_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = (Point, Self::Color, u8)>,
    {
        self.blend_iter(pixels)
    }

    #[inline(always)]
    fn blend_contiguous<I>(&mut self, area: &Rectangle, items: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = (Self::Color, u8)>,
    {
        self.blend_contiguous(area, items)
    }
}

impl<'a> CompositingTarget for DamageGrid<'a> {
    #[inline(always)]
    fn blend_solid(
        &mut self,
        area: &Rectangle,
        color: Self::Color,
        alpha: u8,
    ) -> Result<(), Self::Error> {
        self.as_target::<Rgb565>().blend_solid(area, color, alpha)
    }

    #[inline(always)]
    fn blend_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = (Point, Self::Color, u8)>,
    {
        self.as_target::<Rgb565>().blend_iter(pixels)
    }

    #[inline(always)]
    fn blend_contiguous<I>(&mut self, area: &Rectangle, items: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = (Self::Color, u8)>,
    {
        self.as_target::<Rgb565>().blend_contiguous(area, items)
    }
}

#[cfg(test)]
mod tests {
    use embedded_graphics_core::geometry::Size;
    use yonder_damage::DamageGridStorage320x240;

    use super::*;

    #[test]
    fn test_damage_grid_compositing_target_methods() {
        let mut storage = DamageGridStorage320x240::new();
        let mut grid = storage.as_grid();
        grid.prev_hashes_mut().fill(0);
        grid.active_hashes_mut().fill(0);

        let red = Rgb565::new(31, 0, 0);
        let rect = Rectangle::new(Point::new(16, 16), Size::new(16, 16));
        grid.blend_solid(&rect, red, 200).unwrap();
        assert_ne!(grid.active_hashes()[20 + 1], 0);

        grid.blend_iter([(Point::new(48, 48), red, 150)]).unwrap();
        assert_ne!(grid.active_hashes()[3 * 20 + 3], 0);

        let rect2 = Rectangle::new(Point::new(64, 64), Size::new(16, 16));
        grid.blend_contiguous(&rect2, [(red, 120)]).unwrap();
        assert_ne!(grid.active_hashes()[4 * 20 + 4], 0);
    }
}
