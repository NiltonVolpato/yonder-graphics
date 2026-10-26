#[cfg_attr(feature = "std", derive(Debug))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Point {
    pub x: i16,
    pub y: i16,
}

impl Point {
    pub const fn new(x: i16, y: i16) -> Self {
        Self { x, y }
    }
}

/// A half-open box: `top_left` is inside, `bottom_right` is just outside.
#[cfg_attr(feature = "std", derive(Debug))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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

    pub const fn width(&self) -> i16 {
        self.bottom_right.x - self.top_left.x
    }

    pub const fn height(&self) -> i16 {
        self.bottom_right.y - self.top_left.y
    }

    /// Whether `other` lies entirely inside `self`.
    pub fn contains(&self, other: &BoundingBox) -> bool {
        other.top_left.x >= self.top_left.x
            && other.top_left.y >= self.top_left.y
            && other.bottom_right.x <= self.bottom_right.x
            && other.bottom_right.y <= self.bottom_right.y
    }

    /// The overlapping area of both boxes, or `None` if it is empty.
    pub fn intersection(&self, other: &BoundingBox) -> Option<BoundingBox> {
        let top_left = Point::new(
            self.top_left.x.max(other.top_left.x),
            self.top_left.y.max(other.top_left.y),
        );
        let bottom_right = Point::new(
            self.bottom_right.x.min(other.bottom_right.x),
            self.bottom_right.y.min(other.bottom_right.y),
        );
        (bottom_right.x > top_left.x && bottom_right.y > top_left.y)
            .then(|| BoundingBox::new(top_left, bottom_right))
    }
}

pub trait Bounded {
    fn bounds(&self) -> BoundingBox;
}
