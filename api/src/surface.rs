use core::hash::Hasher;
use core::slice;

use yonder_geometry::screen::{Bounded, BoundingBox, Point};

use crate::color::{Color, WithAlpha};

/// One pixel to blend.
#[derive(Clone, Copy, Hash)]
pub struct Pixel<C> {
    pub position: Point,
    pub color: WithAlpha<C>,
}

/// What a [`Surface`] sees of an item: where it paints, and enough to hash what it
/// paints per tile for damage tracking, without evaluating the shape per tile.
pub trait Footprint {
    /// Every pixel the item may touch, including strokes and antialiasing.
    fn extent(&self) -> BoundingBox;

    /// A box whose pixels are all painted with one paint, and whether that paint is
    /// opaque. `None` is always safe.
    fn interior(&self) -> Option<(BoundingBox, bool)>;

    /// Hashes the paint used inside [`interior`](Self::interior).
    fn hash_paint(&self, hasher: &mut impl Hasher);

    /// Hashes everything that affects the pixels of `tile`. Called for tiles inside
    /// [`extent`](Self::extent) but not inside [`interior`](Self::interior).
    fn hash_tile(&self, tile: &BoundingBox, hasher: &mut impl Hasher);
}

/// A writable region of a [`Surface`]. All points are in surface coordinates, and
/// callers only write inside `bounds()`.
pub trait PixelBuffer: Bounded {
    type Color: Color;

    fn blend_iter<I>(&mut self, pixels: I)
    where
        I: IntoIterator<Item = Pixel<Self::Color>>;

    /// Blends `color` over the pixel at `position`.
    fn blend_pixel(&mut self, position: Point, color: WithAlpha<Self::Color>);

    /// Blends the same color over every pixel of `area`.
    fn blend_color(&mut self, area: &BoundingBox, color: WithAlpha<Self::Color>);

    /// Blends `colors` over `area` in row-major order.
    fn blend_colors<I>(&mut self, area: &BoundingBox, colors: I)
    where
        I: IntoIterator<Item = WithAlpha<Self::Color>>;
}

/// Something that can be drawn on, such as a framebuffer or a display.
pub trait Surface {
    type Color: Color;

    /// The buffer for this surface's current viewport, or `None` if `item` falls
    /// outside it.
    fn viewport(&mut self, item: &impl Footprint) -> Option<impl PixelBuffer<Color = Self::Color>>;
}

/// A row-major pixel buffer covering exactly `bounds`.
pub struct Viewport<'a, C> {
    pixels: &'a mut [C],
    bounds: BoundingBox,
}

impl<'a, C: Color> Viewport<'a, C> {
    pub fn new(pixels: &'a mut [C], bounds: BoundingBox) -> Self {
        assert_eq!(
            pixels.len(),
            bounds.width() as usize * bounds.height() as usize,
            "pixel buffer size does not match bounds"
        );
        Self { pixels, bounds }
    }

    /// The pixels of row `y` from `x0` to `x1`.
    fn span(&mut self, y: i16, x0: i16, x1: i16) -> &mut [C] {
        let BoundingBox { top_left, .. } = self.bounds;
        let start = (y - top_left.y) as usize * self.bounds.width() as usize;
        &mut self.pixels[start + (x0 - top_left.x) as usize..start + (x1 - top_left.x) as usize]
    }
}

impl<C> Bounded for Viewport<'_, C> {
    fn bounds(&self) -> BoundingBox {
        self.bounds
    }
}

impl<C: Color> PixelBuffer for Viewport<'_, C> {
    type Color = C;

    fn blend_iter<I>(&mut self, pixels: I)
    where
        I: IntoIterator<Item = Pixel<C>>,
    {
        for Pixel { position: p, color } in pixels {
            C::blend_slice(self.span(p.y, p.x, p.x + 1), color);
        }
    }

    #[inline]
    fn blend_pixel(&mut self, position: Point, color: WithAlpha<C>) {
        let BoundingBox {
            top_left,
            bottom_right,
        } = self.bounds;
        debug_assert!(top_left.x <= position.x && position.x < bottom_right.x);
        let row = (position.y - top_left.y) as usize * self.bounds.width() as usize;
        C::blend(
            &mut self.pixels[row + (position.x - top_left.x) as usize],
            color,
        );
    }

    fn blend_color(&mut self, area: &BoundingBox, color: WithAlpha<C>) {
        debug_assert!(self.bounds.contains(area));
        for y in area.top_left.y..area.bottom_right.y {
            C::blend_slice(self.span(y, area.top_left.x, area.bottom_right.x), color);
        }
    }

    fn blend_colors<I>(&mut self, area: &BoundingBox, colors: I)
    where
        I: IntoIterator<Item = WithAlpha<C>>,
    {
        debug_assert!(self.bounds.contains(area));
        let mut colors = colors.into_iter();
        for y in area.top_left.y..area.bottom_right.y {
            for (pixel, color) in self
                .span(y, area.top_left.x, area.bottom_right.x)
                .iter_mut()
                .zip(&mut colors)
            {
                C::blend_slice(slice::from_mut(pixel), color);
            }
        }
    }
}
