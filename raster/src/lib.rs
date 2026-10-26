#![no_std]

use yonder_geometry::screen::{BoundingBox, Point};

pub enum Coverage {
    // A pixel that is not fully covered. Where `Point` is the pixel's top-left
    // corner and `u8` is the coverage value (0-255). Never 0 or 255.
    Boundary(Point, u8),
    // A fully covered region.
    Interior(BoundingBox),
}

pub trait Rasterizable {
    // Rasterizes the shape into an iterator of `Coverage` values, clipped to the given bounding box.
    // The iterator must not return overlapping regions.
    fn rasterize(&self, clip: BoundingBox) -> impl Iterator<Item = Coverage>;
}
