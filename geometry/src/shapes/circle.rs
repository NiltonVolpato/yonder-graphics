use core::hash::{Hash, Hasher};

use crate::projective::PointT2;
use crate::screen::{Bounded, BoundingBox, Point};

/// A circle with a homogeneous center, so it can sit on a lattice point
/// (`[x, y, 1]`) or on a pixel center (`[2x + 1, 2y + 1, 2]`).
#[derive(Clone, Copy)]
pub struct Circle {
    center: PointT2,
    // In the center's units: the radius in pixels is `radius / w`.
    radius: i32,
}

impl Circle {
    /// A circle of `radius` pixels. `center` must be a lattice point (`w = 1`) or a pixel
    /// center (`w = 2`): rasterization relies on the center being on the half-pixel grid.
    /// Use `in_box` or `at_pixel_center` instead.
    #[doc(hidden)]
    pub const fn new(center: PointT2, radius: i16) -> Self {
        let (_, _, w) = center.coords();
        debug_assert!(
            w == 1 || w == 2,
            "circle center must be on the half-pixel grid"
        );
        Self {
            center,
            radius: radius as i32 * w,
        }
    }

    /// A circle of `radius` pixels around the center of `pixel`.
    pub const fn at_pixel_center(pixel: Point, radius: i16) -> Self {
        Self::new(PointT2::pixel_center(pixel), radius)
    }

    /// The circle inscribed in `bounds`. If `bounds` isn't square, the circle uses the
    /// shorter side and is centered.
    pub const fn in_box(bounds: BoundingBox) -> Self {
        let (tl, br) = (bounds.top_left, bounds.bottom_right);
        let side = if bounds.width() < bounds.height() {
            bounds.width()
        } else {
            bounds.height()
        };
        Self {
            center: PointT2::new(tl.x as i32 + br.x as i32, tl.y as i32 + br.y as i32, 2),
            radius: side as i32,
        }
    }

    pub const fn center(&self) -> PointT2 {
        self.center
    }

    pub const fn radius(&self) -> i32 {
        self.radius
    }

    /// The circle as integers `[x, y, w, r]`: center `(x / w, y / w)`, radius `r / w`, `w > 0`.
    pub const fn homogeneous(&self) -> [i32; 4] {
        let (x, y, w) = self.center.coords();
        [x, y, w, self.radius]
    }
}

impl Bounded for Circle {
    fn bounds(&self) -> BoundingBox {
        let [x, y, w, r] = self.homogeneous();
        let lo = |c: i32| {
            (c - r)
                .div_euclid(w)
                .clamp(i16::MIN as i32, i16::MAX as i32) as i16
        };
        let hi =
            |c: i32| (-(-(c + r)).div_euclid(w)).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        BoundingBox::new(Point::new(lo(x), lo(y)), Point::new(hi(x), hi(y)))
    }
}

// Hashes this representation: an equal circle written with a different scale hashes
// differently, which can only cause extra damage, never missed damage.
impl Hash for Circle {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.homogeneous().hash(state);
    }
}
