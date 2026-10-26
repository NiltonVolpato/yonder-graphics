use crate::screen::Point;

/// A point of the oriented projective plane, in integer homogeneous coordinates
/// `[x, y, w]`. Points on the hither side have `w > 0`; `[x, y, w]` is the Cartesian
/// point `(x / w, y / w)`.
///
/// No `PartialEq`: two points are equal when one is a positive multiple of the other,
/// which componentwise equality gets wrong.
#[derive(Clone, Copy)]
pub struct PointT2 {
    x: i32,
    y: i32,
    w: i32,
}

impl PointT2 {
    pub const fn new(x: i32, y: i32, w: i32) -> Self {
        PointT2 { x, y, w }
    }

    pub const fn from_screen(p: Point) -> Self {
        PointT2 {
            x: p.x as i32,
            y: p.y as i32,
            w: 1,
        }
    }

    /// The center of the pixel whose top-left corner is `p`.
    pub const fn pixel_center(p: Point) -> Self {
        PointT2 {
            x: 2 * p.x as i32 + 1,
            y: 2 * p.y as i32 + 1,
            w: 2,
        }
    }

    pub(crate) const fn coords(self) -> (i32, i32, i32) {
        (self.x, self.y, self.w)
    }
}
