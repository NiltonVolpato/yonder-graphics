#![no_std]

pub mod color;
pub mod draw;
pub mod shapes;
pub mod style;
pub mod surface;

pub use yonder_geometry::projective::PointT2;
pub use yonder_geometry::screen::{Bounded, BoundingBox, Point};

/// Everything a scene needs. Surface implementations also use [`surface`].
pub mod prelude {
    pub use crate::color::{Color, Rgb565, Rgb888, WithAlpha};
    pub use crate::draw::Drawable;
    pub use crate::shapes::{Circle, Rectangle, Shape};
    pub use crate::style::{Paint, Stroke, Style, Styled};
    pub use crate::surface::Pixel;
    pub use crate::{Bounded, BoundingBox, Point, PointT2};
}
