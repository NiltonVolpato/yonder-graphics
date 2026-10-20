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

### 3. Damage Hash Architecture Implementation (Completed ✅)
- **Documented in:** [`damage-hash-architecture.md`](damage-hash-architecture.md)
- **Implemented:**
  - `hash_solid(color: u16) -> u16` for full-tile opaque overwrites (resets tile hash).
  - `TileHasher` zero-alloc builder for packaging `tool_id`, `u8`, `u16`, and `i32` into a 16-bit token.
  - `DamageGrid` methods: `set_tile_solid`, `accumulate_tile`, `fill_tiles_solid`, `clear_solid`.
  - Dropped naive `make_primitive_token`.
  - Comprehensive unit and integration tests, including `ground_truth_animation_tests.rs` running a multi-frame animation sequence and mathematically verifying that every differing pixel between successive frames is captured by the damage grid with zero underdraw.

### 4. Unified Primitives, Styling & `CompositingTarget` Pipeline (Completed ✅)
- **Implemented:**
  - **1:1 `CompositingTarget` Trait Symmetrical to `DrawTarget`:**
    - `blend_solid(&mut self, area: &Rectangle, color: Self::Color, alpha: u8)`
    - `blend_iter<I>(&mut self, pixels: I) where I: IntoIterator<Item = (Point, Self::Color, u8)>`
    - `blend_contiguous<I>(&mut self, area: &Rectangle, items: I) where I: IntoIterator<Item = (Self::Color, u8)>`
    - Completely eliminated ad-hoc `fill_span`, `blend_span`, and `blend_pixel`.
  - **`DrawTarget` Implemented in Terms of `blend_*` (SSOT):**
    - Both `Viewport` and `DamageGrid` implement `DrawTarget::fill_solid`, `draw_iter`, and `fill_contiguous` by delegating directly to `self.blend_*(..., 255)`.
    - Single source of truth for scissoring, clipping, coordinate translation, and tile hashing.
    - Zero performance cost: `alpha == 255` executes direct memory fills (memset) on `Viewport` and solid hash overwrites on `DamageGrid`.
  - **`embedded-graphics` 0.8 Integration:**
    - Standard primitives (`Circle`, `Rectangle`, `Line`, `Arc`, `Polyline`, `RoundedRectangle`) and styles render seamlessly onto both `DamageGrid` and `Viewport`.
  - **Styled Primitives in `yonder-raster`:**
    - `CompositingStyle<C>` supporting fill, stroke, and alpha blending factors with `From<PrimitiveStyle<C>>` conversion.
    - Oriented projective `Triangle` primitive with `Coord3` vertices, `.into_styled(...)`, and `.draw(&mut target)`.
    - Algebraic `Conic` primitive with `ConicMatrix`, bounds, `.into_styled(...)`, and `.draw(&mut target)`.
  - **Generic Pixel Blending & Viewport Pipeline:**
    - `BlendablePixel` trait supporting `Rgb565` (`u16`) and 32-bit `Rgb888` (`u32`, `0x00RRGGBB`).
    - `Viewport<'a, C: BlendablePixel = Rgb565>` generic over `&'a mut [C::Storage]`, enabling direct zero-copy compositing into desktop 32-bit buffers (`softbuffer`).
    - Scanline span deduplication (`rasterize_triangle_spans`, `rasterize_conic_spans`) for solid and alpha drawing paths.
    - Canonical tool ID constants (`TOOL_ID_SOLID_RECT`, `TOOL_ID_SHAPE`, `TOOL_ID_PIXEL_ITER`, `TOOL_ID_CONTIGUOUS`).
    - Full ground-truth test suite running both `Rgb565` and `Rgb888` across multi-frame animation sequences with zero underdraw.
  - **Canonical $360 \times 360$ Display & Testing:**
    - `DamageGridStorage360x360` ($23 \times 23$ tiles) exercising border tile clipping on the 8px-wide 23rd column and row.
    - Comprehensive ground-truth tests verifying 100% pixel coverage and zero underdraw across multi-frame animation sequences.
    - > 94% test line coverage across the workspace.

### 5. Sparse Traversal & Efficiency Optimization (Medium Priority)
- **Goals:**
  - **Pass 1 Scanline Coalescing:** When a convex polygon/triangle spans 16 vertical scanlines inside the same $16 \times 16$ tile, avoid calling `accumulate_tile` 16 separate times. Group scanlines into tile-level bounds or spans.
  - **Sparse Traversal Audit:** Ensure lines, curves, and arcs evaluate only along their actual drawn trajectory ($O(\text{perimeter})$) rather than sweeping the entire $O(W \times H)$ bounding box. Replace the naive bounding-box scan in `conic.rs` with midpoint/Bresenham stepping.

### 6. Interactive Damage Visualizer Tool (`tools/damage-visualizer`)
- **Status:** Deferred until core compositing targets and styled primitives are implemented.
- **Location:** Dedicated workspace crate at `tools/damage-visualizer`.
- **Dependencies:** `winit = "0.30"`, `softbuffer = "0.4"`.
- **Display & Scaling:**
  - Configurable integer scaling (e.g. 5x integer scale, $800 \times 640$ window for $160 \times 128$ native canvas).
  - Pixel-perfect presentation of the base RGB565 framebuffer expanded to 32-bit ARGB.
- **Visual Overlays & Inspection:**
  - Base rendered image from `Viewport`.
  - Tile grid lines marking $16 \times 16$ tile boundaries.
  - Translucent red/amber tint over tiles flagged dirty by `DamageGrid::is_tile_dirty`.
  - Bright bounding-box wireframe outlines for coalesced rectangles emitted by `coalesce_dirty_rects`.
  - Clean tiles rendered untinted to clearly visualize overdraw culling savings.
- **Interactive Controls:**
  - `Space` / `Right Arrow`: Step forward one frame.
  - `Left Arrow`: Step backward one frame.
  - `P`: Toggle play/pause continuous animation.
  - `G`: Toggle tile grid lines and dirty tile overlay.
  - `C`: Toggle coalesced rectangle wireframes.
  - `R`: Reset animation to Frame 0.
- **Animation Content:**
  - Complex continuous animated scenes: oscillating/rotating gauge needles, translating and bouncing translucent shapes, overlapping cards.
  - Window title showing frame number, dirty tile count, and coalesced rectangle count.

### 7. Configurable Tile Size & Hash Type (Low Priority)
- **Goal:**
  - Make tile size configurable (e.g. 8x8, 16x16, 32x32) rather than fixed 16x16.
  - Make hash type configurable (`u16`, `u32`, `u64`).
  - Audit and replace raw `16` integer literals representing tile dimensions across the codebase with field / associated constants.


