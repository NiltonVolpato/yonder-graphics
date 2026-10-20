use embedded_graphics::primitives::{Primitive, PrimitiveStyle};
use embedded_graphics_core::geometry::{Point, Size};
use embedded_graphics_core::pixelcolor::Rgb565;
use embedded_graphics_core::primitives::Rectangle;
use embedded_graphics_core::Drawable;
use yonder_core::Coord3;
use yonder_damage::{coalesce_dirty_rects, DamageGrid, DamageGridStorage360x360};
use yonder_raster::{CompositingStyle, CompositingTarget, Conic, Triangle, Viewport};

const WIDTH: usize = 360;
const HEIGHT: usize = 360;
const COLS: usize = 23;
const ROWS: usize = 23;

const WHITE: Rgb565 = Rgb565::new(31, 63, 31);
const RED: Rgb565 = Rgb565::new(31, 0, 0);
const GREEN: Rgb565 = Rgb565::new(0, 63, 0);
const BLUE: Rgb565 = Rgb565::new(0, 0, 31);
const YELLOW: Rgb565 = Rgb565::new(31, 63, 0);

/// Immediate-mode scene renderer executing the exact same drawing commands
/// on BOTH the damage grid (Pass 1) and the pixel viewport (Pass 2).
///
/// True immediate-mode: zero heap allocations, zero retained scene graph structs.
fn render_scene<T: CompositingTarget<Color = Rgb565>>(
    target: &mut T,
    frame: usize,
) -> Result<(), T::Error> {
    // 1. Clear background
    target.clear(WHITE)?;

    // 2. Static background card
    Rectangle::new(Point::new(20, 20), Size::new(120, 120))
        .into_styled(PrimitiveStyle::with_fill(BLUE))
        .draw(target)?;

    // 3. Projective triangle: static in frames 0 & 1, shifts by (4, 4) in frame 2+
    let (tx, ty) = if frame >= 2 { (54, 54) } else { (50, 50) };
    Triangle::new(
        Coord3::point(tx, ty),
        Coord3::point(tx + 70, ty),
        Coord3::point(tx, ty + 70),
    )
    .into_styled(CompositingStyle::with_fill(RED))
    .draw(target)?;

    // 4. Conic circle: center dial in frames 0-2, moves to partial border tile (345, 345) in frame 3
    let (cx, cy, r, color) = if frame >= 3 {
        (345, 345, 20, YELLOW)
    } else {
        (200, 200, 40, GREEN)
    };
    Conic::circle(cx, cy, r)
        .into_styled(CompositingStyle::with_fill_alpha(color, 180))
        .draw(target)?;

    Ok(())
}

fn assert_damage_covers_pixel_diff(
    grid: &mut DamageGrid<'_>,
    frame_prev: &[u16],
    frame_curr: &[u16],
    frame_name: &str,
) -> (usize, usize) {
    let mut changed_pixels_count = 0;

    for y in 0..HEIGHT {
        let row_start = y * WIDTH;
        for x in 0..WIDTH {
            let idx = row_start + x;
            if frame_curr[idx] != frame_prev[idx] {
                changed_pixels_count += 1;
                let col = x / (DamageGrid::TILE_SIZE as usize);
                let row = y / (DamageGrid::TILE_SIZE as usize);

                assert!(
                    grid.is_tile_dirty(col, row),
                    "[{}] UNDERDRAW: Pixel at ({}, {}) changed from 0x{:04X} to 0x{:04X}, \
                     but tile ({}, {}) was NOT flagged dirty in DamageGrid!",
                    frame_name,
                    x,
                    y,
                    frame_prev[idx],
                    frame_curr[idx],
                    col,
                    row
                );
            }
        }
    }

    let dirty_tile_count = (0..COLS * ROWS)
        .filter(|&i| grid.is_tile_dirty(i % COLS, i / COLS))
        .count();

    // Verify coalesced rects cover every single changed pixel
    let mut covered_by_coalesce = [false; WIDTH * HEIGHT];
    let mut emitted_rect_count = 0;

    coalesce_dirty_rects(grid, |rect| {
        emitted_rect_count += 1;
        for ry in rect.y..rect.bottom() {
            for rx in rect.x..rect.right() {
                if rx >= 0 && ry >= 0 && (rx as usize) < WIDTH && (ry as usize) < HEIGHT {
                    covered_by_coalesce[(ry as usize) * WIDTH + (rx as usize)] = true;
                }
            }
        }
    });

    if changed_pixels_count > 0 {
        assert!(
            emitted_rect_count > 0,
            "[{}] Expected coalesced rects for {} changed pixels, got 0",
            frame_name,
            changed_pixels_count
        );

        for y in 0..HEIGHT {
            let row_start = y * WIDTH;
            for x in 0..WIDTH {
                let idx = row_start + x;
                if frame_curr[idx] != frame_prev[idx] {
                    assert!(
                        covered_by_coalesce[idx],
                        "[{}] UNDERDRAW IN COALESCE: Pixel at ({}, {}) changed, but outside coalesced dirty rects!",
                        frame_name, x, y
                    );
                }
            }
        }
    } else {
        assert_eq!(
            emitted_rect_count, 0,
            "[{}] Expected 0 coalesced rects when 0 pixels changed, got {}",
            frame_name, emitted_rect_count
        );
    }

    (changed_pixels_count, dirty_tile_count)
}

#[test]
fn test_canonical_360x360_compositing_target_ground_truth() {
    let mut storage = DamageGridStorage360x360::new();
    let mut grid = storage.as_grid();

    let mut frame_prev = [0u16; WIDTH * HEIGHT];
    let mut frame_curr = [0u16; WIDTH * HEIGHT];

    let bounds = yonder_damage::Rect::new(0, 0, WIDTH as u32, HEIGHT as u32);

    // =========================================================================
    // Frame 0: Initial Scene Setup
    // =========================================================================
    {
        // Pass 1: Damage pass
        render_scene(&mut grid, 0).unwrap();

        // Pass 2: Viewport pixel pass
        let mut viewport = Viewport::new(&mut frame_curr, bounds);
        render_scene(&mut viewport, 0).unwrap();

        // Invariant check: All pixels changed (initial startup)
        let (changed, dirty_tiles) = assert_damage_covers_pixel_diff(
            &mut grid,
            &frame_prev,
            &frame_curr,
            "Frame 0 (Startup)",
        );
        assert_eq!(changed, WIDTH * HEIGHT);
        assert!(dirty_tiles > 0);

        frame_prev.copy_from_slice(&frame_curr);
    }

    // =========================================================================
    // Frame 1: Completely Static Frame
    // Redraw the exact same frame with both targets
    // =========================================================================
    {
        render_scene(&mut grid, 1).unwrap();

        let mut viewport = Viewport::new(&mut frame_curr, bounds);
        render_scene(&mut viewport, 1).unwrap();

        let (changed, dirty_tiles) = assert_damage_covers_pixel_diff(
            &mut grid,
            &frame_prev,
            &frame_curr,
            "Frame 1 (Static)",
        );
        assert_eq!(changed, 0, "Static frame must have 0 changed pixels");
        assert_eq!(dirty_tiles, 0, "Static frame must have 0 dirty tiles");

        frame_prev.copy_from_slice(&frame_curr);
    }

    // =========================================================================
    // Frame 2: Sub-tile Triangle Movement (shifts 4px right, 4px down)
    // =========================================================================
    {
        render_scene(&mut grid, 2).unwrap();

        let mut viewport = Viewport::new(&mut frame_curr, bounds);
        render_scene(&mut viewport, 2).unwrap();

        // Distant tiles (e.g. tile 22, 22 on the opposite corner) must remain clean
        assert!(!grid.is_tile_dirty(22, 22));

        let (changed, dirty_tiles) = assert_damage_covers_pixel_diff(
            &mut grid,
            &frame_prev,
            &frame_curr,
            "Frame 2 (Sub-tile Triangle Movement)",
        );
        assert!(changed > 0);
        assert!(dirty_tiles > 0);

        frame_prev.copy_from_slice(&frame_curr);
    }

    // =========================================================================
    // Frame 3: Boundary Tile Test (8-pixel partial border at 352..359)
    // A circle moves right up against the 360 border!
    // =========================================================================
    {
        render_scene(&mut grid, 3).unwrap();

        let mut viewport = Viewport::new(&mut frame_curr, bounds);
        render_scene(&mut viewport, 3).unwrap();

        // Tile (22, 22) (spanning pixels 352..360) MUST be dirty
        assert!(
            grid.is_tile_dirty(22, 22),
            "Tile (22, 22) crossing the partial border must be dirty!"
        );

        let (changed, dirty_tiles) = assert_damage_covers_pixel_diff(
            &mut grid,
            &frame_prev,
            &frame_curr,
            "Frame 3 (Boundary 8px Border Tile Test)",
        );
        assert!(changed > 0);
        assert!(dirty_tiles > 0);

        frame_prev.copy_from_slice(&frame_curr);
    }
}

#[cfg(debug_assertions)]
#[test]
fn test_accumulate_tile_invocation_count() {
    use embedded_graphics_core::draw_target::DrawTarget;
    let mut storage = DamageGridStorage360x360::new();
    let mut grid = storage.as_grid();

    // 1. Clear background (full-tile clear):
    grid.reset_accumulate_count();
    grid.clear_solid(0xFFFF);
    let count_after_clear = grid.accumulate_count();
    assert_eq!(
        count_after_clear, 0,
        "clear_solid should not call accumulate_tile"
    );

    // 2. Fill a solid 120x120 rectangle (covering ~8x8 = 64 tiles):
    grid.reset_accumulate_count();
    let card_rect = Rectangle::new(Point::new(20, 20), Size::new(120, 120));
    grid.fill_solid(&card_rect, RED).unwrap();
    let count_after_rect = grid.accumulate_count();
    println!(
        "accumulate_tile calls after 120x120 fill_solid: {}",
        count_after_rect
    );
    assert_eq!(count_after_rect, 28);

    // 3. Rasterize a triangle (70 pixels wide x 70 pixels high):
    grid.reset_accumulate_count();
    let p1 = Coord3::point(50, 50);
    let p2 = Coord3::point(120, 50);
    let p3 = Coord3::point(50, 120);
    let triangle = Triangle::new(p1, p2, p3).into_styled(CompositingStyle::with_fill(RED));
    triangle.draw(&mut grid).unwrap();
    let count_after_tri = grid.accumulate_count();
    println!(
        "accumulate_tile calls after 70x70 triangle rasterization: {}",
        count_after_tri
    );

    // 4. Rasterize a conic/circle (radius 40, 81x81 bounds):
    grid.reset_accumulate_count();
    let conic =
        Conic::circle(200, 200, 40).into_styled(CompositingStyle::with_fill_alpha(GREEN, 180));
    conic.draw(&mut grid).unwrap();
    let count_after_conic = grid.accumulate_count();
    println!(
        "accumulate_tile calls after 81x81 conic rasterization: {}",
        count_after_conic
    );

    // 5. Total count for full scene:
    grid.reset_accumulate_count();
    render_scene(&mut grid, 0).unwrap();
    let total_scene_count = grid.accumulate_count();
    println!(
        "TOTAL accumulate_tile calls for full scene (Pass 1): {}",
        total_scene_count
    );
}

#[test]
fn test_embedded_graphics_standard_primitives_on_targets() {
    use embedded_graphics::primitives::{Circle, Line};

    let mut storage = DamageGridStorage360x360::new();
    let mut grid = storage.as_grid();

    let mut mem = [0u16; 360 * 360];
    let bounds = yonder_damage::Rect::new(0, 0, 360, 360);
    let mut viewport = Viewport::new(&mut mem, bounds);

    // Circle from embedded-graphics
    let circle = Circle::new(Point::new(50, 50), 30).into_styled(PrimitiveStyle::with_fill(RED));
    circle.draw(&mut grid).unwrap();
    circle.draw(&mut viewport).unwrap();

    // Line from embedded-graphics
    let line = Line::new(Point::new(10, 10), Point::new(100, 100))
        .into_styled(PrimitiveStyle::with_stroke(BLUE, 1));
    line.draw(&mut grid).unwrap();
    line.draw(&mut viewport).unwrap();

    // Both DamageGrid and Viewport receive the standard embedded-graphics primitives cleanly!
    assert!(grid.is_tile_dirty(3, 3));
}

const WHITE_888: embedded_graphics_core::pixelcolor::Rgb888 =
    embedded_graphics_core::pixelcolor::Rgb888::new(255, 255, 255);
const RED_888: embedded_graphics_core::pixelcolor::Rgb888 =
    embedded_graphics_core::pixelcolor::Rgb888::new(255, 0, 0);
const GREEN_888: embedded_graphics_core::pixelcolor::Rgb888 =
    embedded_graphics_core::pixelcolor::Rgb888::new(0, 255, 0);
const BLUE_888: embedded_graphics_core::pixelcolor::Rgb888 =
    embedded_graphics_core::pixelcolor::Rgb888::new(0, 0, 255);
const YELLOW_888: embedded_graphics_core::pixelcolor::Rgb888 =
    embedded_graphics_core::pixelcolor::Rgb888::new(255, 255, 0);

fn render_scene_rgb888<T: CompositingTarget<Color = embedded_graphics_core::pixelcolor::Rgb888>>(
    target: &mut T,
    frame: usize,
) -> Result<(), T::Error> {
    // 1. Clear background
    target.clear(WHITE_888)?;

    // 2. Static background card
    Rectangle::new(Point::new(20, 20), Size::new(120, 120))
        .into_styled(PrimitiveStyle::with_fill(BLUE_888))
        .draw(target)?;

    // 3. Projective triangle: static in frames 0 & 1, shifts by (4, 4) in frame 2+
    let (tx, ty) = if frame >= 2 { (54, 54) } else { (50, 50) };
    Triangle::new(
        Coord3::point(tx, ty),
        Coord3::point(tx + 70, ty),
        Coord3::point(tx, ty + 70),
    )
    .into_styled(CompositingStyle::with_fill(RED_888))
    .draw(target)?;

    // 4. Conic circle: center dial in frames 0-2, moves to partial border tile (345, 345) in frame 3
    let (cx, cy, r, color) = if frame >= 3 {
        (345, 345, 20, YELLOW_888)
    } else {
        (200, 200, 40, GREEN_888)
    };
    Conic::circle(cx, cy, r)
        .into_styled(CompositingStyle::with_fill_alpha(color, 180))
        .draw(target)?;

    Ok(())
}

fn assert_damage_covers_pixel_diff_rgb888(
    grid: &mut DamageGrid<'_>,
    frame_prev: &[u32],
    frame_curr: &[u32],
    frame_name: &str,
) -> (usize, usize) {
    let mut changed_pixels_count = 0;
    let mut covered_by_dirty_tiles = [false; WIDTH * HEIGHT];

    let mut dirty_tile_count = 0;
    for r in 0..ROWS {
        for c in 0..COLS {
            if grid.is_tile_dirty(c, r) {
                dirty_tile_count += 1;
                let start_x = c * 16;
                let end_x = (start_x + 16).min(WIDTH);
                let start_y = r * 16;
                let end_y = (start_y + 16).min(HEIGHT);

                for y in start_y..end_y {
                    let row_start = y * WIDTH;
                    for x in start_x..end_x {
                        covered_by_dirty_tiles[row_start + x] = true;
                    }
                }
            }
        }
    }

    for y in 0..HEIGHT {
        let row_start = y * WIDTH;
        for x in 0..WIDTH {
            let idx = row_start + x;
            if frame_curr[idx] != frame_prev[idx] {
                changed_pixels_count += 1;
                assert!(
                    covered_by_dirty_tiles[idx],
                    "[{}] UNDERDRAW: Pixel at ({}, {}) changed from 0x{:08X} to 0x{:08X}, but tile ({}, {}) was NOT flagged dirty!",
                    frame_name, x, y, frame_prev[idx], frame_curr[idx], x / 16, y / 16
                );
            }
        }
    }

    let mut covered_by_coalesce = [false; WIDTH * HEIGHT];
    let mut emitted_rect_count = 0;

    coalesce_dirty_rects(grid, |rect| {
        emitted_rect_count += 1;
        let start_x = rect.x.max(0) as usize;
        let end_x = (rect.right().max(0) as usize).min(WIDTH);
        let start_y = rect.y.max(0) as usize;
        let end_y = (rect.bottom().max(0) as usize).min(HEIGHT);

        for y in start_y..end_y {
            let row_start = y * WIDTH;
            for x in start_x..end_x {
                covered_by_coalesce[row_start + x] = true;
            }
        }
    });

    if changed_pixels_count > 0 {
        assert!(
            emitted_rect_count > 0,
            "[{}] Expected coalesced rects for {} changed pixels, got 0",
            frame_name,
            changed_pixels_count
        );

        for y in 0..HEIGHT {
            let row_start = y * WIDTH;
            for x in 0..WIDTH {
                let idx = row_start + x;
                if frame_curr[idx] != frame_prev[idx] {
                    assert!(
                        covered_by_coalesce[idx],
                        "[{}] UNDERDRAW IN COALESCE: Pixel at ({}, {}) changed, but outside coalesced dirty rects!",
                        frame_name, x, y
                    );
                }
            }
        }
    } else {
        assert_eq!(
            emitted_rect_count, 0,
            "[{}] Expected 0 coalesced rects when 0 pixels changed, got {}",
            frame_name, emitted_rect_count
        );
    }

    (changed_pixels_count, dirty_tile_count)
}

#[test]
fn test_canonical_360x360_rgb888_ground_truth() {
    use embedded_graphics_core::pixelcolor::Rgb888;

    let mut storage = DamageGridStorage360x360::new();
    let mut grid = storage.as_grid();

    let mut frame_prev = vec![0u32; WIDTH * HEIGHT];
    let mut frame_curr = vec![0u32; WIDTH * HEIGHT];

    let bounds = yonder_damage::Rect::new(0, 0, WIDTH as u32, HEIGHT as u32);

    // Frame 0: Initial scene setup
    {
        render_scene_rgb888(&mut grid.as_target::<Rgb888>(), 0).unwrap();
        let mut viewport = Viewport::<Rgb888>::new(&mut frame_curr, bounds);
        render_scene_rgb888(&mut viewport, 0).unwrap();

        let (changed, dirty_tiles) = assert_damage_covers_pixel_diff_rgb888(
            &mut grid,
            &frame_prev,
            &frame_curr,
            "Frame 0 (RGB888 Startup)",
        );
        assert_eq!(changed, WIDTH * HEIGHT);
        assert!(dirty_tiles > 0);

        frame_prev.copy_from_slice(&frame_curr);
    }

    // Frame 1: Completely static frame
    {
        render_scene_rgb888(&mut grid.as_target::<Rgb888>(), 1).unwrap();
        let mut viewport = Viewport::<Rgb888>::new(&mut frame_curr, bounds);
        render_scene_rgb888(&mut viewport, 1).unwrap();

        let (changed, dirty_tiles) = assert_damage_covers_pixel_diff_rgb888(
            &mut grid,
            &frame_prev,
            &frame_curr,
            "Frame 1 (RGB888 Static)",
        );
        assert_eq!(changed, 0, "No pixels should change on static frame");
        assert_eq!(dirty_tiles, 0, "Zero dirty tiles on static frame");
    }

    // Frame 2: Moving triangle
    {
        render_scene_rgb888(&mut grid.as_target::<Rgb888>(), 2).unwrap();
        let mut viewport = Viewport::<Rgb888>::new(&mut frame_curr, bounds);
        render_scene_rgb888(&mut viewport, 2).unwrap();

        let (changed, dirty_tiles) = assert_damage_covers_pixel_diff_rgb888(
            &mut grid,
            &frame_prev,
            &frame_curr,
            "Frame 2 (RGB888 Triangle Movement)",
        );
        assert!(changed > 0);
        assert!(dirty_tiles > 0);

        frame_prev.copy_from_slice(&frame_curr);
    }

    // Frame 3: Conic circle moves to partial border tile
    {
        render_scene_rgb888(&mut grid.as_target::<Rgb888>(), 3).unwrap();
        let mut viewport = Viewport::<Rgb888>::new(&mut frame_curr, bounds);
        render_scene_rgb888(&mut viewport, 3).unwrap();

        let (changed, dirty_tiles) = assert_damage_covers_pixel_diff_rgb888(
            &mut grid,
            &frame_prev,
            &frame_curr,
            "Frame 3 (RGB888 Border Conic)",
        );
        assert!(changed > 0);
        assert!(dirty_tiles > 0);
        assert!(grid.is_tile_dirty(22, 22));
    }
}
