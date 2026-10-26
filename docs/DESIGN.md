# Design

This file records decisions that have been made, with a short reason for each. Anything not decided is listed under **Open questions** at the end. If something is not written here, it has not been decided.

## Motivation: Why This Changes the Paradigm

Traditional embedded graphics stacks are built on an unstable compromise. They typically choose between two flawed extremes:

1. **Low-level naive blitters (e.g., standard `embedded-graphics` drivers):** They lack scene graphs, have zero awareness of occlusion, and treat display buses as memory-mapped targets. They either generate massive bus overdraw by redrawing entire rectangular AABBs or degrade into pathological single-pixel command loops. Furthermore, they provide no unified mathematical framework for curves, perspective, or blending.

2. **Heavy retained-mode frameworks (e.g., LVGL, Skia subsets):** They drag in floating-point math, complex memory allocators, dynamic scene trees, and fragmented rasterizers that treat circles, lines, beziers, and affine transforms as completely unrelated geometric pipelines.

## Arithmetic constraints

- Integers only. No floats, no fixed point, no trigonometric functions, no square roots.
- One exception: `isqrt` is allowed to start rasterizing a shape at a given row (for a band or a clip box) instead of walking from the top. It runs once per shape and replaces many iterations of the incremental algorithm.
- Prefer incremental algorithms (Bresenham, midpoint, Zingl, forward differencing). They work with the gradient of the implicit function, so they lower the degree instead of raising it, and the intermediate values stay narrow.
- Prefer comparisons that avoid normalization. Example: an angle threshold is stored as an integer direction and compared with a cross product, not as a ratio or a cosine.

## Coordinates

- Screen coordinates are `i16`. The documented range is 14 bits, symmetric around zero, checked with debug assertions.
- A `Point` is a lattice point: an intersection of the grid, not a pixel. Pixel (i, j) is the cell [i, i+1) × [j, j+1). On a 10×10 screen, (10, 10) is a valid point but not a pixel.
- `BoundingBox` stores `min` and `max` and is half-open. The full 10×10 screen is (0, 0)–(10, 10). It is empty when min ≥ max on either axis.
- Shared edges between shapes need a consistent tie rule so that a pixel on the edge belongs to exactly one shape.
- `Point` derives `Clone, Copy, PartialEq, Eq`. `BoundingBox` derives only `Clone, Copy`: empty boxes are not normalized, so field equality would call two empty boxes different.
- No `Debug` by default, because it takes flash on MCUs. `Debug` comes with the `std` feature, and `defmt::Format` with the `defmt` feature.

## Crates

```
geometry/   no dependencies
    screen/       Point, BoundingBox, Bounded
    projective/   PointT2 (oriented projective plane, homogeneous coordinates)
    shapes/       Circle, Rectangle, ... (pure geometry)
raster/     → geometry      Coverage, Rasterizable, impls for regions
api/        → geometry, raster
                            pixel buffers, colors, paint, style, drawing, themes
```

- Shapes are geometry and live in `geometry`, so `raster` can implement its traits for them without depending on `api`. Cargo does not allow cycles between crates.
- There is no display trait. Sending data to the panel (SPI, DMA) is platform code outside the library.
- `api` re-exports `Point`, `BoundingBox`, `Bounded` and `PointT2`, and has a `prelude` with everything a scene needs, so users depend on one crate. Surface implementations also import `surface`.

## Geometry

- The Euclidean/Cartesian layer was dropped. `screen` holds only points and bounding boxes.
- Projective types (`PointT2`, and later `PointT3`) keep their fields private. Homogeneous points are equal when one is a positive multiple of the other, so they must not get a componentwise `PartialEq`.
- Shapes carry no style and are not drawable by themselves.

## Circles

- `Circle` has a homogeneous center (`PointT2`), restricted to a lattice point (`[x, y, 1]`) or a pixel center (`[2x + 1, 2y + 1, 2]`, `PointT2::pixel_center`). Rasterization relies on the center being on the half-pixel grid (horizontal symmetry). A circle on a pixel center with an even diameter ends in the middle of pixels.
- Users make circles with `Circle::in_box` or `Circle::at_pixel_center(pixel, radius)`. The general `Circle::new(center, radius)` is `#[doc(hidden)]` and debug-asserts the restriction.
- `Circle::in_box(bounds)` is the circle inscribed in a box. A non-square box uses the shorter side, centered. Its center may or may not fall on a pixel center.
- Internally a circle is `[x, y, w, r]`: center `(x / w, y / w)`, radius `r / w`, `w` 1 or 2. The rasterizer converts it to half pixels, where centers and stroke radii are all integers.
- Rasterization is marching squares on the circle's implicit function (`Disk::implicit_value`: negative inside, zero on the circle), evaluated at pixel corners. Coverage is the area of the polygon cut by the zero crossings on the four edges, with the function taken as linear along each edge.
  - The lattice points inside on each line are walked incrementally from the previous line, like Zingl's circle algorithm: the implicit function at the chord's left end is updated with shifts and additions. One `isqrt` starts the walk at the clip's first row. The right end is the left end mirrored around the center.
  - Corner values along a row of edge cells are updated the same way. Positions are `i16`, the implicit function is `i32` (it is only evaluated near the edge), and `i64` is only used for the starting values.
  - Only cells near the edge are visited, and interiors are one span per row.
  - A row still needs checking when none of its lattice points are inside: caps and tiny circles. Those cells, and the one extra cell the widest row can bulge into, are refined into 2×2 half-pixel cells.
- The linear crossings sit slightly inside the convex edge. A filled circle comes out about 1 px² short in total, whatever its radius, which is like a radius 1/(2πr) smaller. In rings the inner and outer losses cancel.

## Regions and rasterization

- A shape is only a representation. What gets rasterized is a region: `Filled<S>` (the interior) or `Stroked<S>` (the band along the boundary). A stroke is the fill of a different region. The names are adjectives so that `Stroke` is free for the style type.
- Strokes are centered on the boundary, as in SVG. The bounds of a stroke include half its width, rounded up, and more at miter joins.
- Miter limit: stored as an integer direction at the threshold angle, and compared against the corner's (dot, |cross|) with one cross product.
- Rasterization is monochrome. It produces coverage, not color:

  ```rust
  pub enum Coverage {
      Boundary(Point, u8),          // the cell whose min corner is the point
      Partial(BoundingBox, u8),     // every cell has the same partial coverage
      Interior(BoundingBox),        // full coverage
  }

  pub trait Rasterizable {
      fn rasterize(&self, clip: BoundingBox) -> impl Iterator<Item = Coverage>;
  }
  ```

- `Partial` exists because a horizontal or vertical edge that cuts pixels in half gives a whole row of equal coverage (for example a centered 1-pixel stroke on a grid-aligned rectangle). It becomes one blend call instead of one per pixel. Emitting single pixels as 1×1 boxes would be inefficient, so `Boundary` stays.
- `Coverage` is not `#[non_exhaustive]`. A new variant must break compilation in `api`, not fall into a `_ =>` arm and get dropped.
- Within one call, coverage items never overlap, so no pixel is blended twice by the same paint.
- The order of items is not specified. Shapes may use their symmetries.
- Rendering in bands is done through the clip box.

## Color, paint and style

- A pixel buffer's color type is the display color and has no alpha.
- Alpha is a separate `u8`, carried with a color as `WithAlpha<C> { color, alpha }`. The opacity applied to a pixel is coverage × paint alpha. `draw` folds coverage into the alpha, so pixel buffers only ever see a `WithAlpha`.
- `color.with_alpha(a)` builds one. Not `alpha()`, which reads like a getter.
- Color types for now: `Rgb565`, and `Rgb888` for running on the host and generating images. They live in `api::color`. A separate crate would also take paint and style.
- Named colors (`RED`, `WHITE`, ...) are associated constants on `Color`, so generic code can write `C::RED`. Each is written once as 8-bit RGB and converted to each type by a `const fn`, because `From::from` can't be called in a `const` on stable Rust.
- Conversions between color types are `From` impls between concrete types. No trait-level channel accessors until generic code needs them (gradients).
- A paint describes what fills a region: a solid color with alpha for now, gradients and textures later. It is `#[non_exhaustive] enum Paint<C> { Solid(WithAlpha<C>) }`, without a lifetime until a variant needs one. It is built with `Paint::solid(color).alpha(a)`.
- `Stroke<C> { width, paint: Paint<C> }`. Strokes take a `Paint`, not only a color, because gradient borders are common in UIs.
- `Style<C> { fill: Option<Paint<C>>, stroke: Option<Stroke<C>> }`, built with `Style::new().fill(paint).stroke(Stroke::new(width, paint))`. Its fields are public: harmless next to the builder.
- The builders and `Paint::solid(..).alpha(..)` are `const fn`, so a theme can define its styles as constants. That is why they take `Paint<C>` and not `impl Into<Paint<C>>`: `const fn` can't call trait methods.
- Types built with builders are `#[non_exhaustive]`, so fields can be added without breaking users: `Style`, `Stroke`, `Paint`, later `Join`. Plain data (`WithAlpha`, `Pixel`) is not.
- Images are generic over the color type, so an image in the buffer's format is copied row by row without conversion.
- Pixel buffers are stored as bytes internally. Typed slices of color newtypes measurably lost `memcpy`/`memset` and SIMD on Xtensa. The typed API stays at the method signatures.

## Surfaces and pixel buffers

- Displays can't be read back, or it is too slow. Drawing always goes into a buffer in memory.
- `Surface::viewport(&mut self, item: &impl Footprint) -> Option<impl PixelBuffer<Color = Self::Color>>`. The surface has already decided what its viewport is. The item is passed so the surface can skip it (`None`), clip it, or update its damage grid from it.
- `Footprint` is what a surface sees of an item: `extent()` (every pixel it may touch, including stroke and antialiasing), `interior()`, `hash_paint()` and `hash_tile(tile)`. See Damage tracking. Every `Styled` type gets it through a blanket impl. `Pixel` implements it too. The method is not called `bounds()`, so it doesn't clash with `Bounded::bounds` on types that implement both.
- `Viewport<'a, C>` is the library's pixel buffer for a region, and it carries the color type.
- `PixelBuffer` is the device-dependent part. Its methods take whole regions. The rasterizer must not produce work one pixel at a time for regions, or the SIMD in the buffer methods is wasted. Single-pixel calls are only for isolated antialiased pixels.
- `Pixel<C> { position, color: WithAlpha<C> }` is kept, and it is `Drawable`. Drawing a pixel as a 1×1 rectangle would be inefficient.

## Drawing

- `Drawable<S: Surface>` takes the surface type as a parameter. `dyn Drawable<S>` works for a fixed surface. It can't take the buffer type instead (`Drawable<P: PixelBuffer>`): the buffer that `viewport` returns is an opaque type, and stable Rust can't name it in a bound such as `D: Drawable<…>` (return type notation is experimental, and not allowed in that position).
- `Styled` gives a shape and its style. It returns the style by value, because the type often computes the style (state, theme) instead of storing it.
- There is a blanket `Drawable` impl for `T: Styled`, which draws the fill and then the stroke.
- References: `impl<T: Styled + ?Sized> Styled for &T` forwards, so the blanket impl covers `&T`. A separate `impl Drawable for &D` would conflict with the blanket impl, because `&T` is fundamental and downstream crates could implement `Styled` for `&TheirType`. Drawables that are not styled (text, images) don't get a `&D` impl. That only matters for generic functions that take `impl Drawable<S>` by value.
- `shape.with_style(style)` gives a `WithStyle<S, C>`, which stores the style. It needs the color parameter because it stores a `Style<C>` and shapes have no color type. It is built only by `with_style`. Themed types such as `WithTheme<Button>` store nothing and get their style from the type.
- Draw order: fill first, then stroke. The inner half of the stroke blends over the fill.

## Damage tracking

- No widget tree is required.
- The screen is divided into a grid of blocks. Each block keeps a hash of what was drawn into it.
- When a drawable covers a block completely with opaque paint, the block's hash is reset to that drawable's contribution, because nothing below it is visible there. Otherwise the drawable's contribution is combined into the existing hash. The combination has to depend on order, since draw order changes the pixels.
- A block whose hash differs from the previous frame is damaged. Damaged blocks are coalesced into flush rectangles.
- Updating the grid must be fast: a couple of shape evaluations per drawable, not one per tile. `Footprint` gives:
  - `extent()`: the tiles it touches.
  - `interior()`: a box whose pixels are all painted with one paint, and whether it is opaque. `None` is always safe. For styled shapes it is `Shape::interior()` (a box fully inside the shape) shrunk by half the stroke, and only with a solid fill. For a circle it is the inscribed square, with 70/99 (just under 1/√2) in place of a square root.
  - `hash_paint()`: the paint inside the interior.
  - `hash_tile(tile)`: everything that affects the tile's pixels. For circles and rectangles that is the geometry and the style. Arcs, paths and text can later hash only their part of the tile.
- Tiles inside the interior get an assignment: an opaque paint resets the hash to the paint, a translucent one is combined. Other tiles in the extent combine `hash_tile`. Tiles outside are not visited.
- Tiles between the interior box and the edge count as edge tiles, so moving a circle also damages some tiles that stay fully inside it. That is correct, only more damage than necessary. Exact classification would cost four evaluations per tile.
- Hashing uses `core::hash::Hash`, so colors, `WithAlpha`, `Paint`, `Stroke`, `Style` and shapes implement it. `Circle` hashes its `[x, y, w, r]` representation. An equal circle at a different scale hashes differently, which can only cause extra damage.
- The surface decides, per drawable, whether to skip it, draw it clipped, or draw it unclipped. This uses the drawable's extent and interior, which is why it takes the drawable itself, not only a box.
- Users make their own drawables by grouping primitives: they implement `Drawable<S>` and draw the parts. Each part calls `viewport` itself, so the damage grid sees the parts and groups need nothing more.

## Open questions

- Text in the damage grid. Per tile, hash the shaped clusters whose ink bounds touch the tile, with their positions, plus font, size and color. Not implemented yet.
- Antialiasing method. Zingl's antialiasing is not good enough. Marching squares is a candidate.
- Polygon strokes. Computing the offset edges exactly needs a square root per edge, which the arithmetic constraints do not allow. An approach is still needed.
- Exact `PixelBuffer` method set and names. It needs at least: opaque fill of a box, blend of a box with constant alpha, blend of a single pixel, and copy of image rows. The current `blend_iter`, `blend_color` and `blend_colors` have no row copy that can become a `memcpy`.
- How a color maps to bytes in a buffer (layout, endianness), and where per-format blending lives once buffers are bytes.
- Join types besides miter, and their defaults. Miter limit postponed.
- New primitives from users. A downstream crate can't implement `Rasterizable for Filled<'_, TheirShape>` (orphan rule, E0117). Groups of primitives don't need it. If user primitives are wanted, `Rasterizable` would move onto the shape (`fill(clip)`, `stroke(width, clip)`).
- `Circle` exposes `homogeneous()` so that `raster` can read it. Once ellipses or transformed circles come, `geometry` could provide the implicit function instead.
- Mixed integer types. `Point` is `i16`, `PointT2` is `i32`, and `Circle::new` takes an `i16` radius.
- Which tie rule to use on shared edges.
