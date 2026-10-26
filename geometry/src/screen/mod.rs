#[derive(Clone, Copy)]
pub struct Point {
    pub x: i16,
    pub y: i16,
}

impl Point {
    pub const fn new(x: i16, y: i16) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy)]
pub struct BoundingBox {
    pub top_left: Point,
    pub bottom_right: Point,
}

impl BoundingBox {
    pub const fn new(top_left: Point, bottom_right: Point) -> Self {
        debug_assert!(
            bottom_right.x >= top_left.x && bottom_right.y >= top_left.y,
            "BoundingBox coordinates are invalid: bottom_right must be greater than or equal to top_left"
        );
        Self {
            top_left,
            bottom_right,
        }
    }

    pub fn width(&self) -> i16 {
        self.bottom_right.x - self.top_left.x
    }

    pub fn height(&self) -> i16 {
        self.bottom_right.y - self.top_left.y
    }
}

pub trait Bounded {
    fn bounds(&self) -> BoundingBox;
}
