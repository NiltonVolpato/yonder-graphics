use crate::color::{Color, WithAlpha};
use crate::shapes::Shape;

/// What to paint with.
#[non_exhaustive]
#[derive(Clone, Copy, Hash)]
pub enum Paint<C> {
    Solid(WithAlpha<C>),
}

impl<C: Color> Paint<C> {
    /// An opaque solid color.
    pub const fn solid(color: C) -> Self {
        Paint::Solid(WithAlpha {
            color,
            alpha: u8::MAX,
        })
    }

    /// Sets the opacity (255 = opaque).
    pub const fn alpha(self, alpha: u8) -> Self {
        match self {
            Paint::Solid(c) => Paint::Solid(WithAlpha {
                color: c.color,
                alpha,
            }),
        }
    }
}

/// An outline `width` pixels thick, centered on the shape's edge.
#[non_exhaustive]
#[derive(Clone, Copy, Hash)]
pub struct Stroke<C> {
    pub width: u16,
    pub paint: Paint<C>,
}

impl<C: Color> Stroke<C> {
    pub const fn new(width: u16, paint: Paint<C>) -> Self {
        Self { width, paint }
    }
}

/// Builders are `const`, so themes can define styles as constants.
#[non_exhaustive]
#[derive(Clone, Copy, Hash)]
pub struct Style<C> {
    pub fill: Option<Paint<C>>,
    pub stroke: Option<Stroke<C>>,
}

impl<C: Color> Style<C> {
    pub const fn new() -> Self {
        Self {
            fill: None,
            stroke: None,
        }
    }

    pub const fn fill(mut self, paint: Paint<C>) -> Self {
        self.fill = Some(paint);
        self
    }

    pub const fn stroke(mut self, stroke: Stroke<C>) -> Self {
        self.stroke = Some(stroke);
        self
    }
}

impl<C: Color> Default for Style<C> {
    fn default() -> Self {
        Self::new()
    }
}

/// A shape together with its style.
pub trait Styled {
    type Shape: Shape;
    type Color: Color;

    fn shape(&self) -> &Self::Shape;
    fn style(&self) -> Style<Self::Color>;
}

// Covers references through the blanket impls on `Styled`, which can't have their own `&T` impls.
impl<T: Styled + ?Sized> Styled for &T {
    type Shape = T::Shape;
    type Color = T::Color;

    fn shape(&self) -> &Self::Shape {
        (**self).shape()
    }

    fn style(&self) -> Style<Self::Color> {
        (**self).style()
    }
}

/// A shape with a style stored next to it. Built by [`Shape::with_style`].
pub struct WithStyle<S, C> {
    pub(crate) shape: S,
    pub(crate) style: Style<C>,
}

impl<S: Shape, C: Color> Styled for WithStyle<S, C> {
    type Shape = S;
    type Color = C;

    fn shape(&self) -> &S {
        &self.shape
    }

    fn style(&self) -> Style<C> {
        self.style
    }
}
