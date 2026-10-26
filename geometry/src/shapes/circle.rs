use crate::screen::{Bounded, BoundingBox, Point};

pub struct Circle {
    pub center: Point,
    pub radius: i16,
}

impl Bounded for Circle {
    fn bounds(&self) -> BoundingBox {
        todo!()
    }
}
