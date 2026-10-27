use core::hash::Hash;

use yonder_geometry::screen::{Bounded, BoundingBox, Point};
pub use yonder_geometry::shapes::{Circle, Rectangle};

use crate::color::Color;
use crate::style::{Style, WithStyle};

/// Geometry that can be styled and drawn. `Hash` covers the geometry only; damage
/// tracking combines it with the style.
pub trait Shape: Bounded + Hash {
    fn with_style<C: Color>(self, style: Style<C>) -> WithStyle<Self, C>
    where
        Self: Sized,
    {
        WithStyle { shape: self, style }
    }

    /// A box whose pixels are all fully inside the shape. `None` is always safe.
    fn interior(&self) -> Option<BoundingBox> {
        None
    }
}

impl Shape for Rectangle {
    fn interior(&self) -> Option<BoundingBox> {
        let b = self.bounds();
        (b.width() > 0 && b.height() > 0).then_some(b)
    }
}

impl Shape for Circle {
    /// The inscribed square, with 70/99 (just under 1/√2) in place of a square root.
    fn interior(&self) -> Option<BoundingBox> {
        // 32 bits suffice: with 14-bit coordinates and w ≤ 2, 99·c and 70·r stay under 2²².
        let [x, y, w, r] = self.homogeneous();
        let half = 70 * r;
        let lo = |c: i32| -((half - 99 * c).div_euclid(99 * w));
        let hi = |c: i32| (99 * c + half).div_euclid(99 * w);
        let px = |v: i32| v.clamp(i16::MIN.into(), i16::MAX.into()) as i16;
        let (x0, y0, x1, y1) = (lo(x), lo(y), hi(x), hi(y));
        (x0 < x1 && y0 < y1)
            .then(|| BoundingBox::new(Point::new(px(x0), px(y0)), Point::new(px(x1), px(y1))))
    }
}
