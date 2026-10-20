# Damage Hash Tracking & Tile Architecture

## 1. Problem Statement: Why Bounding Boxes Fail

In naive spatial damage tracking, a primitive's axis-aligned bounding box (AABB) is used to flag intersecting tiles as damaged. This causes pathological overdraw in common UI scenarios:
- **Sweeping Paths & Arcs:** If an arc of $1^\circ$ sweeps across the display from a center point, its AABB covers almost the **entire screen**. Bounding-box tracking flags every tile on the screen as dirty, completely defeating damage culling.
- **Diagonal Lines & Needles:** A 1-pixel wide gauge needle passing through a $16\times 16$ tile only touches a few pixels. But its bounding box covers a huge area.
- **Occluded & Redundant Geometry:** If an opaque red shape covers a tile, and in the next frame another opaque red shape covers the exact same tile, the pixels of that tile did not change at all. Hashing primitive IDs or tracking AABBs falsely marks the tile as dirty.

---

## 2. The Core Mental Model: Conceptual Tile-Pixel Hashing

The fundamental invariant of Yonder's damage pipeline is:
> **A $16\times 16$ tile's hash must reflect the visual result of what is rendered into its 256 pixels.**
> - If the final 256 pixels of a tile are identical to the previous frame $\implies$ **Identical Hash** (0 damage, 0 overdraw).
> - If any pixel in that tile changes (including antialiasing bleeding across the boundary from a neighboring shape) $\implies$ **Different Hash** (dirty tile, re-rasterized).

Because evaluating and buffering 256 high-resolution pixels per tile across the entire display in Pass 1 would violate `#![no_std]` memory constraints and double the rendering time, we achieve this through **tool-aware damage-mode execution**.

---

## 3. Tool-Aware Damage Mode (Dual-Mode Rendering)

Instead of guessing from the outside, **each drawing tool/shape knows its own geometry**:
- In **Raster Mode** (Pass 2): The tool rasterizes physical RGB565 pixels into the SRAM scratchpad.
- In **Damage Mode** (Pass 1): The tool visits **only the specific tiles it actually draws into**. Unrelated tiles across the screen are never visited and remain completely untouched.

### The Two Tile Draw Behaviors

When a shape or tool draws into a $16\times 16$ tile during Damage Mode, there are strictly two cases:

#### Case A: Full-Tile Opaque Overwrite (Reset)
- **Condition:** An opaque shape completely covers the entire $16\times 16$ tile (e.g. background clear, solid rectangle fill, or the interior of a large polygon where all 4 tile corners are strictly inside the shape).
- **Behavior:** The contents underneath are 100% occluded and irrelevant. The tile hash is **reset**:
  $$\text{hash} = \mathtt{hash\_solid}(\text{color})$$
- **Result:** If Frame 1 ended with this tile being solid red, and Frame 2 ends with this tile being solid red, both produce the identical solid-red hash. The diff detects zero damage.

#### Case B: Partial Coverage / Antialiasing / Blending (Accumulate)
- **Condition:** The shape only partially covers the tile (edges cutting through, thin strokes, 1-degree arcs, semi-transparent fills, or antialiasing fringes bleeding into the tile from an adjacent edge).
- **Behavior:** It **cannot reset** the hash. It must combine with what was underneath:
  $$\text{hash} = \mathtt{hash\_accumulate}(\text{current\_hash}, \text{shape\_token})$$
- **Result:** Any change in line position, curve angle, stroke width, or color changes `shape_token`, which alters the accumulated hash and correctly flags the tile as dirty.

---

## 4. `TileHasher`: Zero-Alloc Shape Token Builder

To produce rich, collision-resistant tokens without memory allocation or branching overhead:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileHasher(u16);

impl TileHasher {
    #[inline(always)]
    pub const fn new(tool_id: u8) -> Self {
        Self(0x811Cu16 ^ (tool_id as u16))
    }

    #[inline(always)]
    pub const fn write_u16(self, val: u16) -> Self {
        Self(hash_accumulate(self.0, val))
    }

    #[inline(always)]
    pub const fn write_i32(self, val: i32) -> Self {
        let low = val as u16;
        let high = (val >> 16) as u16;
        Self(hash_accumulate(hash_accumulate(self.0, low), high))
    }

    #[inline(always)]
    pub const fn finish(self) -> u16 {
        self.0
    }
}
```

### Usage Example:
```rust
// A 1-degree arc tool hashes its tool ID and relevant settings:
let token = TileHasher::new(TOOL_ARC)
    .write_u16(color)
    .write_u16(stroke_width)
    .write_i32(angle_deg)
    .finish();

// Tool walks ONLY the tiles intersected by the arc:
for (col, row) in arc_stepper {
    grid.accumulate_tile(col, row, token);
}
```

---

## 5. API Additions to `DamageGrid`

```rust
impl<'a> DamageGrid<'a> {
    /// Resets the tile hash to a solid opaque color (full-tile overwrite).
    #[inline]
    pub fn set_tile_solid(&mut self, col: usize, row: usize, color: u16) {
        if col < self.cols && row < self.rows {
            self.active_hashes[row * self.cols + col] = hash_solid(color);
        }
    }

    /// Accumulates a partial shape token into the tile hash.
    #[inline]
    pub fn accumulate_tile(&mut self, col: usize, row: usize, token: u16) {
        if col < self.cols && row < self.rows {
            let idx = row * self.cols + col;
            self.active_hashes[idx] = hash_accumulate(self.active_hashes[idx], token);
        }
    }

    /// Fills a rectangular block of full tiles with a solid opaque color.
    pub fn fill_tiles_solid(&mut self, c_start: usize, c_end: usize, r_start: usize, r_end: usize, color: u16) {
        let solid_hash = hash_solid(color);
        for row in r_start..r_end.min(self.rows) {
            let row_offset = row * self.cols;
            for col in c_start..c_end.min(self.cols) {
                self.active_hashes[row_offset + col] = solid_hash;
            }
        }
    }
}
```

---

## 6. Antialiasing Bleed

Antialiasing filters have a subpixel footprint (typically $\pm 1$ pixel).
When a tool tests whether an edge touches a tile, it accounts for this filter radius:
- An edge at subpixel $x = 15.8$ inside tile $(col, row)$ bleeds into subpixel $x = 0.2$ of tile $(col + 1, row)$.
- The tool's edge walker naturally touches both tile $(col, row)$ and tile $(col + 1, row)$, accumulating the token into both.
- If the shape moves, the antialiased fringe changes, and both tiles are marked dirty.

---

## 7. Resuming Checklist for Next Session
1. **[`hash.rs`](crates/yonder-damage/src/hash.rs):** Implement `hash_solid(color)`, refine `hash_accumulate`, and add `TileHasher`.
2. **[`grid.rs`](crates/yonder-damage/src/grid.rs):** Add `set_tile_solid`, `accumulate_tile`, and `fill_tiles_solid`.
3. **Tests:** Add unit tests validating:
   - Full-tile reset produces identical hashes for identical solid colors regardless of prior history.
   - Partial accumulation changes hash when shape settings or position change.
   - Sweeping arc only touches its line of tiles, leaving surrounding tiles clean.
4. **Integration:** Implement `DamageCanvas` in `yonder` / `yonder-raster` using these methods in Pass 1.
