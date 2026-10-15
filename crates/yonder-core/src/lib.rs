#![no_std]

pub mod conic;
pub mod coord;
pub mod duality;
pub mod gcd;
pub mod predicate;
pub mod transform;

// Public facade exports
pub use conic::{ConicMatrix, ConicScanlineStepper};
pub use coord::{Coord3, Coord4};
pub use duality::{join, meet, Line3};
pub use gcd::{
    gcd_binary_u32, gcd_binary_u64, normalize_quad_i32, normalize_triple_i32,
    normalize_triple_i64_to_i32,
};
pub use predicate::{orientation, orientation_det, Orientation};
pub use transform::{Matrix3x3, Matrix4x4};
