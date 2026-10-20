# Project TODOs

## Architecture & Refactoring

### 1. Remove Const Generics from `DamageGrid` Struct (Completed ✅)
- **Implemented:**
  - `DamageGridStorage<const COLS: usize, const ROWS: usize>`: Owns contiguous 2D arrays `[[u16; COLS]; ROWS]`. Uses both `COLS` and `ROWS` directly with no `TILES` parameter.
  - `DamageGrid<'a>`: A non-generic borrowed view over the hash slices (`active_hashes: &'a mut [u16]`, `prev_hashes: &'a mut [u16]`) used in all damage algorithms, functions, and traits.
  - `coalesce_dirty_rects` and `stream_dirty_bands` now take `&DamageGrid<'_>` with zero const generic parameters.

### 2. Redesign `coalesce_dirty_rects` Algorithm (Completed ✅)
- **Implemented:**
  - In-place buffer reuse of `active_hashes` as an intrusive FIFO queue with `+ 2` pointer offset.
  - Zero heap allocations, zero external queue arrays, and zero row buffers (`MAX_COLS` and `MAX_ACTIVE_RECTS` completely eliminated).
  - Row-first contiguous span formation with 1–2 tile clean gap bridging.
  - Downward rectangle merging with accumulated waste tracking against `COALESCE_THRESHOLD_PIXELS`.
  - Disjoint right-side spans on rows are deferred into the intrusive queue and preserved.

### 3. Damage Hash Architecture Implementation (High Priority)
- **Documented in:** [`damage-hash-architecture.md`](damage-hash-architecture.md)
- **Goal:** Replace coarse bounding box hashing with tool-aware damage mode:
  - Implement `hash_solid(color: u16) -> u16` for full-tile opaque overwrites (resets tile hash).
  - Implement `TileHasher` builder for packaging `tool_id` + settings into a 16-bit token.
  - Implement `DamageGrid` methods: `set_tile_solid`, `accumulate_tile`, `fill_tiles_solid`.
  - Add comprehensive unit tests verifying full-tile reset vs partial accumulation, antialiasing bleed, and arc/stroke tile stepping.

### 4. Configurable Tile Size & Hash Type (Low Priority)
- **Goal:**
  - Make tile size configurable (e.g. 8x8, 16x16, 32x32) rather than fixed 16x16.
  - Make hash type configurable (`u16`, `u32`, `u64`).
  - Audit and replace raw `16` integer literals representing tile dimensions across the codebase with field / associated constants.


