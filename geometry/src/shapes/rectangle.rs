use crate::screen::{Bounded, BoundingBox, Point};

pub struct Rectangle {
    pub top_left: Point,
    pub bottom_right: Point,
}

impl Rectangle {
    pub fn new(top_left: Point, bottom_right: Point) -> Self {
        Self {
            top_left,
            bottom_right,
        }
    }
}

impl Bounded for Rectangle {
    fn bounds(&self) -> BoundingBox {
        BoundingBox::new(self.top_left, self.bottom_right)
    }
}
