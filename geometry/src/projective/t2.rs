use crate::screen::Point;

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
}
