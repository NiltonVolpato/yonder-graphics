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

## Geometry

- The Euclidean/Cartesian layer was dropped. `screen` holds only points and bounding boxes.
- Projective types (`PointT2`, and later `PointT3`) keep their fields private. Homogeneous points are equal when one is a positive multiple of the other, so they must not get a componentwise `PartialEq`.
- Shapes carry no style and are not drawable by themselves.

## Regions and rasterization

- A shape is only a representation. What gets rasterized is a region: `Fill<S>` (the interior) or `Stroke<S>` (the band along the boundary). A stroke is the fill of a different region.
- Strokes are centered on the boundary, as in SVG. The bounds of a stroke include half its width, and more at miter joins.
- Miter limit: stored as an integer direction at the threshold angle, and compared against the corner's (dot, |cross|) with one cross product.
- Rasterization is monochrome. It produces coverage, not color:

  ```rust
  pub enum Coverage {
      Boundary(Point, u8),     // the cell whose min corner is the point
      Interior(BoundingBox),   // full coverage
  }

  pub trait Rasterizable {
      fn rasterize(&self, clip: BoundingBox) -> impl Iterator<Item = Coverage>;
  }
  ```

- Within one call, coverage items never overlap, so no pixel is blended twice by the same paint.
- The order of items is not specified. Shapes may use their symmetries.
- Rendering in bands is done through the clip box.

## Color, paint and style

- A pixel buffer's color type is the display color and has no alpha.
- Alpha is a separate `u8`, carried with a color. The opacity applied to a pixel is coverage × paint alpha.
- A paint describes what fills a region: a solid color with alpha for now, gradients and textures later.
- Images are generic over the color type, so an image in the buffer's format is copied row by row without conversion.
- Pixel buffers are stored as bytes internally. Typed slices of color newtypes measurably lost `memcpy`/`memset` and SIMD on Xtensa. The typed API stays at the method signatures.

## Drawing

- `Drawable<P: PixelBuffer>` takes the buffer type as a parameter, so one shape can be drawn into different buffers, and `dyn Drawable<P>` works for a fixed buffer.
- `Styled` gives a shape and its style. It returns the style by value, because the type often computes the style (state, theme) instead of storing it.
- There is a blanket `Drawable` impl for `T: Styled`, which draws the fill and then the stroke.
- `shape.with_style(style)` gives a `WithStyle<S>`, which stores the style. Themed types such as `WithTheme<Button>` store nothing and get their style from the type.
- Draw order: fill first, then stroke. The inner half of the stroke blends over the fill.

## Damage tracking

- No widget tree is required.
- The screen is divided into a grid of blocks. Each block keeps a hash of what was drawn into it.
- When a drawable covers a block completely with opaque paint, the block's hash is reset to that drawable's contribution, because nothing below it is visible there. Otherwise the drawable's contribution is combined into the existing hash. The combination has to depend on order, since draw order changes the pixels.
- A block whose hash differs from the previous frame is damaged. Damaged blocks are coalesced into flush rectangles.
- This is why drawables need `covers(block) -> bool`:
  - `false` is always safe.
  - It is only `true` for opaque paint.
  - For convex regions it is exact: a box is inside a convex region if and only if its four corners are.
  - For non-convex regions nothing can be concluded from the corners, so it returns `false` unless the shape has its own test.
- The surface decides, per drawable, whether to skip it, draw it clipped, or draw it unclipped. This uses the drawable's bounds and `covers`, which is why it takes the drawable itself, not only a box.

## Open questions

- Text in the damage grid. Glyphs are not convex, so `covers` is usually `false`, and text over a background may need its own rule.
- Antialiasing method. Zingl's antialiasing is not good enough. Marching squares is a candidate.
- Polygon strokes. Computing the offset edges exactly needs a square root per edge, which the arithmetic constraints do not allow. An approach is still needed.
- Exact `PixelBuffer` method set and names. It needs at least: opaque fill of a box, blend of a box with constant alpha, blend of a single pixel, and copy of image rows.
- Whether `Paint` is a closed enum or a trait.
- Name of the color-with-alpha type.
- Coherence of the blanket `Drawable` impl for `T: Styled` with `impl Drawable for &D`.
- Join types besides miter, and their defaults.
- Which tie rule to use on shared edges.
