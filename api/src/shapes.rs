use yonder_geometry::screen::Bounded;
pub use yonder_geometry::shapes::Circle;
pub use yonder_geometry::shapes::Rectangle;

use crate::display::Color;
use crate::display::WithStyle;

pub trait Shape: Bounded {
    fn with_style<C: Color>(self, style: crate::display::Style<C>) -> WithStyle<Self, C>
    where
        Self: Sized,
    {
        WithStyle { shape: self, style }
    }
}

impl Shape for Circle {}
impl Shape for Rectangle {}
