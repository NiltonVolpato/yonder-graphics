//! 2D integer rectangle primitives for damage tracking and clipping.

/// A 2D integer bounding rectangle on the screen.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    /// Creates a new rectangle.
    #[inline(always)]
    pub const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Creates an empty zero-size rectangle at the origin.
    pub const ZERO: Self = Self::new(0, 0, 0, 0);

    /// Returns true if this rectangle has zero area.
    #[inline(always)]
    pub const fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Returns a new rectangle dilated outward by `amount` pixels on all four sides.
    #[inline(always)]
    pub const fn dilate(&self, amount: u32) -> Self {
        if self.is_empty() {
            return *self;
        }
        let a = amount as i32;
        Self {
            x: self.x - a,
            y: self.y - a,
            width: self.width + 2 * amount,
            height: self.height + 2 * amount,
        }
    }

    /// Returns the rightmost horizontal edge coordinate ($x + \text{width}$).
    #[inline(always)]
    pub const fn right(&self) -> i32 {
        self.x + self.width as i32
    }

    /// Returns the bottommost vertical edge coordinate ($y + \text{height}$).
    #[inline(always)]
    pub const fn bottom(&self) -> i32 {
        self.y + self.height as i32
    }

    /// Returns the total number of pixels contained in this rectangle.
    #[inline(always)]
    pub const fn pixel_count(&self) -> usize {
        (self.width as usize) * (self.height as usize)
    }

    /// Returns true if this rectangle overlaps with `other`.
    #[inline]
    pub const fn intersects(&self, other: &Self) -> bool {
        if self.is_empty() || other.is_empty() {
            return false;
        }
        self.x < other.right()
            && self.right() > other.x
            && self.y < other.bottom()
            && self.bottom() > other.y
    }

    /// Computes the intersection of two rectangles.
    /// Returns `None` if they do not overlap.
    #[inline]
    pub fn intersection(&self, other: &Self) -> Option<Self> {
        if !self.intersects(other) {
            return None;
        }

        let x1 = self.x.max(other.x);
        let y1 = self.y.max(other.y);
        let x2 = self.right().min(other.right());
        let y2 = self.bottom().min(other.bottom());

        if x2 <= x1 || y2 <= y1 {
            None
        } else {
            Some(Self::new(x1, y1, (x2 - x1) as u32, (y2 - y1) as u32))
        }
    }

    /// Computes the minimal bounding box enclosing both rectangles.
    #[inline]
    pub fn bounding_box(&self, other: &Self) -> Self {
        if self.is_empty() {
            return *other;
        }
        if other.is_empty() {
            return *self;
        }

        let x1 = self.x.min(other.x);
        let y1 = self.y.min(other.y);
        let x2 = self.right().max(other.right());
        let y2 = self.bottom().max(other.bottom());

        Self::new(x1, y1, (x2 - x1) as u32, (y2 - y1) as u32)
    }
}

impl From<embedded_graphics_core::primitives::Rectangle> for Rect {
    #[inline(always)]
    fn from(rect: embedded_graphics_core::primitives::Rectangle) -> Self {
        Self::new(
            rect.top_left.x,
            rect.top_left.y,
            rect.size.width,
            rect.size.height,
        )
    }
}

impl From<&embedded_graphics_core::primitives::Rectangle> for Rect {
    #[inline(always)]
    fn from(rect: &embedded_graphics_core::primitives::Rectangle) -> Self {
        Self::new(
            rect.top_left.x,
            rect.top_left.y,
            rect.size.width,
            rect.size.height,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rect_intersection() {
        let r1 = Rect::new(10, 10, 50, 50);
        let r2 = Rect::new(40, 40, 50, 50);

        assert!(r1.intersects(&r2));
        let inter = r1.intersection(&r2).unwrap();
        assert_eq!(inter, Rect::new(40, 40, 20, 20));

        let r3 = Rect::new(100, 100, 20, 20);
        assert!(!r1.intersects(&r3));
        assert_eq!(r1.intersection(&r3), None);

        // Empty intersections
        assert!(!Rect::ZERO.intersects(&r1));
        assert_eq!(Rect::ZERO.intersection(&r1), None);
        assert!(Rect::new(10, 10, 0, 10).is_empty());
        assert!(Rect::new(10, 10, 10, 0).is_empty());
    }

    #[test]
    fn test_rect_bounding_box() {
        let r1 = Rect::new(10, 10, 20, 20);
        let r2 = Rect::new(50, 50, 20, 20);
        let bbox = r1.bounding_box(&r2);
        assert_eq!(bbox, Rect::new(10, 10, 60, 60));

        // Empty bounding box cases
        assert_eq!(Rect::ZERO.bounding_box(&r1), r1);
        assert_eq!(r1.bounding_box(&Rect::ZERO), r1);
    }

    #[test]
    fn test_rect_dilate() {
        let r = Rect::new(10, 10, 20, 30);
        let dilated = r.dilate(1);
        assert_eq!(dilated, Rect::new(9, 9, 22, 32));

        let empty = Rect::ZERO;
        assert_eq!(empty.dilate(1), Rect::ZERO);
    }
}
