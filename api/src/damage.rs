//! The damage grid: a 16-bit hash per 16×16 tile of everything drawn into it, compared with
//! the previous frame's. Ported from `yonder-damage` on `main`.

use core::hash::Hasher;
use core::marker::PhantomData;

use yonder_geometry::screen::BoundingBox;

use crate::color::Color;
use crate::surface::{Footprint, PixelBuffer, Surface, Viewport};

pub const TILE: i16 = 16;

/// Folds `token` into `hash`. Order matters: drawing A then B hashes differently from B then A.
#[inline(always)]
const fn accumulate(hash: u16, token: u16) -> u16 {
    // 0x9E37 is 2¹⁶ / φ.
    hash.wrapping_add(token).rotate_left(5) ^ token.wrapping_mul(0x9E37)
}

/// A 16-bit [`Hasher`] for `Footprint`'s hashes, cheap on 32-bit microcontrollers.
pub struct TileHasher(u16);

impl TileHasher {
    pub const fn new() -> Self {
        Self(0x811C)
    }
}

impl Default for TileHasher {
    fn default() -> Self {
        Self::new()
    }
}

// Every write is one step per 16 bits: the derived `Hash` impls also call `write_u8` and
// `write_usize` (enum discriminants), which would otherwise go byte by byte through `write`.
impl Hasher for TileHasher {
    fn write(&mut self, bytes: &[u8]) {
        let (pairs, rest) = bytes.as_chunks::<2>();
        for &pair in pairs {
            self.write_u16(u16::from_le_bytes(pair));
        }
        for &byte in rest {
            self.write_u16(byte.into());
        }
    }

    fn write_u8(&mut self, value: u8) {
        self.write_u16(value.into());
    }

    fn write_i8(&mut self, value: i8) {
        self.write_u16(value as u16);
    }

    // shortcut: on 64-bit hosts the high half is dropped; these hash lengths and enum
    // discriminants, which are small.
    fn write_usize(&mut self, value: usize) {
        self.write_u32(value as u32);
    }

    fn write_isize(&mut self, value: isize) {
        self.write_u32(value as u32);
    }

    fn write_u16(&mut self, value: u16) {
        self.0 = accumulate(self.0, value);
    }

    fn write_i16(&mut self, value: i16) {
        self.write_u16(value as u16);
    }

    fn write_u32(&mut self, value: u32) {
        self.write_u16(value as u16);
        self.write_u16((value >> 16) as u16);
    }

    fn write_i32(&mut self, value: i32) {
        self.write_u32(value as u32);
    }

    fn finish(&self) -> u64 {
        self.0.into()
    }
}

/// A damage pass over a screen of `COLUMNS × ROWS` tiles. Drawing into it updates the
/// tiles' hashes and paints nothing. A tile is damaged when its hash differs from the
/// previous frame's.
pub struct DamageGrid<C, const COLUMNS: usize, const ROWS: usize> {
    current: [[u16; COLUMNS]; ROWS],
    previous: [[u16; COLUMNS]; ROWS],
    color: PhantomData<C>,
}

impl<C, const COLUMNS: usize, const ROWS: usize> DamageGrid<C, COLUMNS, ROWS> {
    /// A grid whose first frame damages every tile.
    pub const fn new() -> Self {
        Self {
            current: [[0; COLUMNS]; ROWS],
            previous: [[u16::MAX; COLUMNS]; ROWS],
            color: PhantomData,
        }
    }

    /// Ends the frame: its hashes become the previous ones.
    pub fn next_frame(&mut self) {
        self.previous = self.current;
        self.current = [[0; COLUMNS]; ROWS];
    }

    pub fn is_damaged(&self, column: usize, row: usize) -> bool {
        self.current[row][column] != self.previous[row][column]
    }
}

impl<C, const COLUMNS: usize, const ROWS: usize> Default for DamageGrid<C, COLUMNS, ROWS> {
    fn default() -> Self {
        Self::new()
    }
}

/// The tiles `area` touches, as half-open column and row ranges clamped to the grid.
fn touched(area: &BoundingBox, columns: usize, rows: usize) -> (usize, usize, usize, usize) {
    let first = |v: i16, n: usize| (v.max(0) / TILE).min(n as i16) as usize;
    let end = |v: i16, n: usize| ((v.max(0) + TILE - 1) / TILE).min(n as i16) as usize;
    let (top_left, bottom_right) = (area.top_left, area.bottom_right);
    (
        first(top_left.x, columns),
        end(bottom_right.x, columns),
        first(top_left.y, rows),
        end(bottom_right.y, rows),
    )
}

/// The tiles fully inside `area`, as half-open column and row ranges.
fn inside(area: &BoundingBox) -> (i16, i16, i16, i16) {
    let ceil = |v: i16| (v + TILE - 1).div_euclid(TILE);
    let (top_left, bottom_right) = (area.top_left, area.bottom_right);
    (
        ceil(top_left.x),
        bottom_right.x.div_euclid(TILE),
        ceil(top_left.y),
        bottom_right.y.div_euclid(TILE),
    )
}

impl<C: Color + 'static, const COLUMNS: usize, const ROWS: usize> Surface
    for DamageGrid<C, COLUMNS, ROWS>
{
    type Color = C;

    fn viewport(&mut self, item: &impl Footprint) -> Option<impl PixelBuffer<Color = C>> {
        let (column_start, column_end, row_start, row_end) = touched(&item.extent(), COLUMNS, ROWS);
        // shortcut: one hash_tile for every edge tile, which holds while no Footprint uses
        // the tile (true for circles and rectangles); arcs and text will need one per tile.
        let mut hasher = TileHasher::new();
        item.hash_tile(&item.extent(), &mut hasher);
        let edge = hasher.0;
        let (interior, opaque) = item
            .interior()
            .map_or(((0, 0, 0, 0), false), |(area, opaque)| {
                (inside(&area), opaque)
            });
        let mut hasher = TileHasher::new();
        item.hash_paint(&mut hasher);
        let paint = hasher.0;
        for row in row_start..row_end {
            let row_inside = interior.2 <= row as i16 && (row as i16) < interior.3;
            for column in column_start..column_end {
                let tile = &mut self.current[row][column];
                *tile = if row_inside && interior.0 <= column as i16 && (column as i16) < interior.1
                {
                    // Nothing below shows through an opaque paint: start over from it.
                    if opaque {
                        paint
                    } else {
                        accumulate(*tile, paint)
                    }
                } else {
                    accumulate(*tile, edge)
                };
            }
        }
        None::<Viewport<'_, C>>
    }
}
