use yonder_geometry::screen::{BoundingBox, Point};
use yonder_raster::Rasterizable;

use crate::shapes::Shape;

pub struct Pixel<C: Color> {
    position: Point,
    color: C,
}

pub trait Color: Copy + PartialEq {}

pub trait PixelBuffer {
    type Color: Color;

    fn blend_iter<I>(&mut self, pixels: I)
    where
        I: IntoIterator<Item = Pixel<Self::Color>>;

    fn blend_solid(&mut self, area: &BoundingBox, color: Self::Color);

    fn blend_contiguous<I>(&mut self, area: &BoundingBox, items: I)
    where
        I: IntoIterator<Item = Self::Color>;
}

pub struct Viewport<'a> {
    buffer: &'a mut [u8],
    bounds: BoundingBox,
}

pub trait Drawable<P: PixelBuffer> {
    fn draw(&self, surface: &mut P);
}

pub struct Style<C: Color> {
    fill: Option<C>,
    stroke: Option<StrokeStyle<C>>,
}

pub struct StrokeStyle<C: Color> {
    // ...
}

pub trait Styled {
    type Shape;
    type Color: Color;
    fn shape(&self) -> &Self::Shape;
    fn style(&self) -> &Style<Self::Color>;
}

pub struct WithStyle<S, C>
where
    S: Shape,
    C: Color,
{
    pub(super) shape: S,
    pub(super) style: Style<C>,
}

impl<T, P> Drawable<P> for T
where
    T: Styled,
    P: PixelBuffer,
    T::Color: Into<P::Color>,
    for<'a> Fill<'a, T::Shape>: Rasterizable,
    for<'a> Stroke<'a, T::Shape>: Rasterizable,
{
    fn draw(&self, surface: &mut P) {
        todo!()
    }
}
