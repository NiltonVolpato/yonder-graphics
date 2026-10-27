use core::hash::{Hash, Hasher};

use yonder_geometry::screen::{Bounded, BoundingBox, Point};
use yonder_raster::{Coverage, Filled, Rasterizable, Stroked};

use crate::color::{Color, WithAlpha};
use crate::shapes::Shape;
use crate::style::{Paint, Styled};
use crate::surface::{Footprint, Pixel, PixelBuffer, Surface};

pub trait Drawable<S: Surface> {
    fn draw(&self, surface: &mut S);
}

impl<T, S> Drawable<S> for T
where
    T: Styled,
    S: Surface,
    T::Color: Into<S::Color>,
    for<'a> Filled<'a, T::Shape>: Rasterizable,
    for<'a> Stroked<'a, T::Shape>: Rasterizable,
{
    fn draw(&self, surface: &mut S) {
        let Some(mut buffer) = surface.viewport(self) else {
            return;
        };
        let (shape, style) = (self.shape(), self.style());
        let clip = buffer.bounds();
        if let Some(Paint::Solid(color)) = style.fill {
            let color = convert(color);
            Filled(shape).rasterize(clip, |coverage| blend(&mut buffer, coverage, color));
        }
        if let Some(stroke) = style.stroke {
            let Paint::Solid(color) = stroke.paint;
            let color = convert(color);
            let outline = Stroked {
                shape,
                width: stroke.width,
            };
            outline.rasterize(clip, |coverage| blend(&mut buffer, coverage, color));
        }
    }
}

impl<T: Styled> Footprint for T {
    fn extent(&self) -> BoundingBox {
        let bounds = self.shape().bounds();
        match self.style().stroke {
            Some(stroke) => outset(bounds, half_width(stroke.width)).unwrap_or(bounds),
            None => bounds,
        }
    }

    fn interior(&self) -> Option<(BoundingBox, bool)> {
        let style = self.style();
        let Some(Paint::Solid(fill)) = style.fill else {
            return None;
        };
        let inside = self.shape().interior()?;
        // The inner half of the stroke paints over the fill near the edge.
        let inside = match style.stroke {
            Some(stroke) => outset(inside, -half_width(stroke.width))?,
            None => inside,
        };
        Some((inside, fill.alpha == u8::MAX))
    }

    fn hash_paint(&self, hasher: &mut impl Hasher) {
        self.style().fill.hash(hasher);
    }

    fn hash_tile(&self, _tile: &BoundingBox, hasher: &mut impl Hasher) {
        self.shape().hash(hasher);
        self.style().hash(hasher);
    }
}

/// Half a stroke, rounded up. Coordinates are whole pixels, so this also includes the
/// partly covered pixels at the stroke's outer edge.
fn half_width(width: u16) -> i16 {
    width.div_ceil(2).min(i16::MAX as u16) as i16
}

/// `b` grown by `d` on every side (shrunk if `d < 0`), or `None` if nothing is left.
fn outset(b: BoundingBox, d: i16) -> Option<BoundingBox> {
    let (tl, br) = (b.top_left, b.bottom_right);
    let tl = Point::new(tl.x.saturating_sub(d), tl.y.saturating_sub(d));
    let br = Point::new(br.x.saturating_add(d), br.y.saturating_add(d));
    (tl.x < br.x && tl.y < br.y).then(|| BoundingBox::new(tl, br))
}

impl<C, S> Drawable<S> for Pixel<C>
where
    C: Color + Into<S::Color>,
    S: Surface,
{
    fn draw(&self, surface: &mut S) {
        let Some(mut buffer) = surface.viewport(self) else {
            return;
        };
        if buffer.bounds().contains(&self.extent()) {
            buffer.blend_pixel(self.position, convert(self.color));
        }
    }
}

impl<C: Color> Footprint for Pixel<C> {
    fn extent(&self) -> BoundingBox {
        let p = self.position;
        BoundingBox::new(p, Point::new(p.x.saturating_add(1), p.y.saturating_add(1)))
    }

    fn interior(&self) -> Option<(BoundingBox, bool)> {
        None
    }

    fn hash_paint(&self, _hasher: &mut impl Hasher) {}

    fn hash_tile(&self, _tile: &BoundingBox, hasher: &mut impl Hasher) {
        self.hash(hasher);
    }
}

fn blend<B: PixelBuffer>(buffer: &mut B, coverage: Coverage, color: WithAlpha<B::Color>) {
    match coverage {
        Coverage::Interior(area) => buffer.blend_color(&area, color),
        Coverage::Partial(area, coverage) => buffer.blend_color(&area, scale(color, coverage)),
        Coverage::Boundary(position, coverage) => {
            buffer.blend_pixel(position, scale(color, coverage))
        }
    }
}

fn convert<C: Into<D>, D>(c: WithAlpha<C>) -> WithAlpha<D> {
    WithAlpha {
        color: c.color.into(),
        alpha: c.alpha,
    }
}

/// Multiplies the paint's alpha by the pixel coverage.
fn scale<C>(c: WithAlpha<C>, coverage: u8) -> WithAlpha<C> {
    WithAlpha {
        color: c.color,
        alpha: crate::color::div255(u16::from(c.alpha) * u16::from(coverage)),
    }
}
