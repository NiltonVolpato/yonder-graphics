use yonder_damage::{coalesce_dirty_rects, DamageGrid, DamageGridStorage, TileHasher};

const WIDTH: usize = 160;
const HEIGHT: usize = 128;
const COLS: usize = WIDTH / 16; // 10
const ROWS: usize = HEIGHT / 16; // 8

const WHITE: u16 = 0xFFFF;
const GRAY: u16 = 0x8410;
const AMBER: u16 = 0xFD20;
const CYAN: u16 = 0x07FF;
const RED: u16 = 0xF800;

const TOOL_BOX: u8 = 20;
const TOOL_LINE: u8 = 30;

fn fill_rect_pixels(frame: &mut [u16], x: usize, y: usize, w: usize, h: usize, color: u16) {
    for py in y..(y + h).min(HEIGHT) {
        let row_start = py * WIDTH;
        for px in x..(x + w).min(WIDTH) {
            frame[row_start + px] = color;
        }
    }
}

fn draw_line_pixels(frame: &mut [u16], x0: i32, y0: i32, x1: i32, y1: i32, color: u16) {
    let mut x = x0;
    let mut y = y0;
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    loop {
        if x >= 0 && y >= 0 && (x as usize) < WIDTH && (y as usize) < HEIGHT {
            frame[(y as usize) * WIDTH + (x as usize)] = color;
        }
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

/// Helper that steps a line and touches all intersected tiles during Damage Pass 1.
fn step_line_tiles(grid: &mut DamageGrid<'_>, x0: i32, y0: i32, x1: i32, y1: i32, token: u16) {
    let mut x = x0;
    let mut y = y0;
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    let mut last_tile = (usize::MAX, usize::MAX);

    loop {
        if x >= 0 && y >= 0 && (x as usize) < WIDTH && (y as usize) < HEIGHT {
            let col = (x as usize) / (DamageGrid::TILE_SIZE as usize);
            let row = (y as usize) / (DamageGrid::TILE_SIZE as usize);
            if (col, row) != last_tile {
                grid.accumulate_tile(col, row, token);
                last_tile = (col, row);
            }
        }
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

/// Mathematical ground-truth invariant validator:
/// 1. Every changed pixel (curr != prev) MUST be in a tile marked dirty in DamageGrid.
/// 2. Every changed pixel MUST be enclosed inside at least one coalesced dirty rectangle.
/// 3. Executes `coalesce_dirty_rects` to perform the automatic buffer transfer for the next frame.
fn assert_damage_covers_all_pixel_changes(
    grid: &mut DamageGrid<'_>,
    frame_prev: &[u16],
    frame_curr: &[u16],
    frame_name: &str,
) -> (usize, usize) {
    let mut changed_pixels_count = 0;
    let mut changed_tiles = [false; COLS * ROWS];

    for y in 0..HEIGHT {
        let row_start = y * WIDTH;
        for x in 0..WIDTH {
            let idx = row_start + x;
            if frame_curr[idx] != frame_prev[idx] {
                changed_pixels_count += 1;
                let col = x / (DamageGrid::TILE_SIZE as usize);
                let row = y / (DamageGrid::TILE_SIZE as usize);
                changed_tiles[row * COLS + col] = true;

                assert!(
                    grid.is_tile_dirty(col, row),
                    "[{}] UNDERDRAW: Pixel at ({}, {}) changed from 0x{:04X} to 0x{:04X}, \
                     but tile ({}, {}) was NOT flagged dirty!",
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

    // Now run coalesce_dirty_rects to verify full pixel coverage and transfer hashes to prev_hashes
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
fn test_multi_frame_ground_truth_pixel_animation() {
    let mut storage = DamageGridStorage::<COLS, ROWS>::new();
    let mut grid = storage.as_grid();

    let mut frame_prev = [0u16; WIDTH * HEIGHT];
    let mut frame_curr = [0u16; WIDTH * HEIGHT];

    // =========================================================================
    // Frame 0: Initial Scene Setup
    // Background: White
    // Card: Gray 64x64 at (16, 16)
    // Box: Amber 16x16 at (32, 32)
    // =========================================================================
    {
        // Pass 1: Damage Grid
        grid.clear_solid(WHITE);
        // Card covers tiles (1..5, 1..5) completely -> fill_tiles_solid
        grid.fill_tiles_solid(1, 5, 1, 5, GRAY);
        // Box at (32, 32, 16, 16) is exactly tile (2, 2)
        grid.set_tile_solid(2, 2, AMBER);

        // Pass 2: Physical Pixel Buffer
        frame_curr.fill(WHITE);
        fill_rect_pixels(&mut frame_curr, 16, 16, 64, 64, GRAY);
        fill_rect_pixels(&mut frame_curr, 32, 32, 16, 16, AMBER);

        // Ground-truth check
        let (changed, dirty_tiles) = assert_damage_covers_all_pixel_changes(
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
    // Redraw the exact same scene
    // =========================================================================
    {
        // Pass 1: Damage Grid
        grid.clear_solid(WHITE);
        grid.fill_tiles_solid(1, 5, 1, 5, GRAY);
        grid.set_tile_solid(2, 2, AMBER);

        // Pass 2: Physical Pixel Buffer
        frame_curr.fill(WHITE);
        fill_rect_pixels(&mut frame_curr, 16, 16, 64, 64, GRAY);
        fill_rect_pixels(&mut frame_curr, 32, 32, 16, 16, AMBER);

        // Ground-truth check: exactly 0 pixels changed, exactly 0 dirty tiles!
        let (changed, dirty_tiles) = assert_damage_covers_all_pixel_changes(
            &mut grid,
            &frame_prev,
            &frame_curr,
            "Frame 1 (Static)",
        );
        assert_eq!(changed, 0, "Static frame must have 0 changed pixels");
        assert_eq!(dirty_tiles, 0, "Static frame must produce ZERO dirty tiles");

        frame_prev.copy_from_slice(&frame_curr);
    }

    // =========================================================================
    // Frame 2: Sub-Tile Translation
    // Amber box translates 4px right: (32, 32) -> (36, 32).
    // Now crosses tiles (2, 2) and (3, 2).
    // Surrounding tiles (0, 0), (1, 1), background, etc. must remain clean!
    // =========================================================================
    {
        // Pass 1: Damage Grid
        grid.clear_solid(WHITE);
        grid.fill_tiles_solid(1, 5, 1, 5, GRAY);
        // The box at (36, 32) only partially covers tiles (2, 2) and (3, 2)
        let token = TileHasher::new(TOOL_BOX)
            .write_i32(36)
            .write_i32(32)
            .write_u16(AMBER)
            .finish();
        grid.accumulate_tile(2, 2, token);
        grid.accumulate_tile(3, 2, token);

        // Pass 2: Physical Pixel Buffer
        frame_curr.fill(WHITE);
        fill_rect_pixels(&mut frame_curr, 16, 16, 64, 64, GRAY);
        fill_rect_pixels(&mut frame_curr, 36, 32, 16, 16, AMBER);

        let (changed, dirty_tiles) = assert_damage_covers_all_pixel_changes(
            &mut grid,
            &frame_prev,
            &frame_curr,
            "Frame 2 (Sub-tile Motion)",
        );
        // Pixels changed: 4 columns reverted to GRAY (4 * 16 = 64) + 4 columns turned AMBER (64) = 128
        assert_eq!(changed, 128);
        // Exactly tiles (2, 2) and (3, 2) are dirty!
        assert_eq!(
            dirty_tiles, 2,
            "Only tiles (2, 2) and (3, 2) should be dirty"
        );

        frame_prev.copy_from_slice(&frame_curr);
    }

    // =========================================================================
    // Frame 3: Color Change in Place
    // Box at (36, 32) changes color from AMBER to CYAN.
    // =========================================================================
    {
        // Pass 1: Damage Grid
        grid.clear_solid(WHITE);
        grid.fill_tiles_solid(1, 5, 1, 5, GRAY);
        let token = TileHasher::new(TOOL_BOX)
            .write_i32(36)
            .write_i32(32)
            .write_u16(CYAN)
            .finish();
        grid.accumulate_tile(2, 2, token);
        grid.accumulate_tile(3, 2, token);

        // Pass 2: Physical Pixel Buffer
        frame_curr.fill(WHITE);
        fill_rect_pixels(&mut frame_curr, 16, 16, 64, 64, GRAY);
        fill_rect_pixels(&mut frame_curr, 36, 32, 16, 16, CYAN);

        let (changed, dirty_tiles) = assert_damage_covers_all_pixel_changes(
            &mut grid,
            &frame_prev,
            &frame_curr,
            "Frame 3 (Color Change)",
        );
        assert_eq!(changed, 16 * 16); // 256 pixels turned CYAN
        assert_eq!(dirty_tiles, 2);

        frame_prev.copy_from_slice(&frame_curr);
    }

    // =========================================================================
    // Frame 4: Large Solid Modal Overwrite
    // A solid RED overlay covers (0, 0, 64, 64) -> 4x4 tiles (0..4, 0..4).
    // =========================================================================
    {
        // Pass 1: Damage Grid
        grid.clear_solid(WHITE);
        grid.fill_tiles_solid(0, 4, 0, 4, RED);

        // Pass 2: Physical Pixel Buffer
        frame_curr.fill(WHITE);
        fill_rect_pixels(&mut frame_curr, 0, 0, 64, 64, RED);

        let (changed, dirty_tiles) = assert_damage_covers_all_pixel_changes(
            &mut grid,
            &frame_prev,
            &frame_curr,
            "Frame 4 (Solid Modal Overwrite)",
        );
        assert!(changed > 0);
        assert_eq!(
            dirty_tiles, 23,
            "Must flag 16 tiles covered by RED + 7 tiles where GRAY card disappeared"
        );

        frame_prev.copy_from_slice(&frame_curr);
    }

    // =========================================================================
    // Frame 5: Redundant Solid Overwrite
    // The same solid RED overlay is drawn again on top.
    // =========================================================================
    {
        // Pass 1: Damage Grid
        grid.clear_solid(WHITE);
        grid.fill_tiles_solid(0, 4, 0, 4, RED);

        // Pass 2: Physical Pixel Buffer
        frame_curr.fill(WHITE);
        fill_rect_pixels(&mut frame_curr, 0, 0, 64, 64, RED);

        let (changed, dirty_tiles) = assert_damage_covers_all_pixel_changes(
            &mut grid,
            &frame_prev,
            &frame_curr,
            "Frame 5 (Redundant Solid Overwrite)",
        );
        assert_eq!(changed, 0);
        assert_eq!(
            dirty_tiles, 0,
            "Redundant solid fill MUST produce 0 dirty tiles"
        );

        frame_prev.copy_from_slice(&frame_curr);
    }

    // =========================================================================
    // Frame 6: Diagonal Needle Rotation
    // A needle line sweeps from (80, 64) -> (120, 20) in Frame 6a,
    // then to (120, 40) in Frame 6b.
    // =========================================================================
    {
        // 6a: Draw needle at angle 1
        grid.clear_solid(WHITE);
        let token_needle_a = TileHasher::new(TOOL_LINE)
            .write_i32(80)
            .write_i32(64)
            .write_i32(120)
            .write_i32(20)
            .write_u16(CYAN)
            .finish();
        step_line_tiles(&mut grid, 80, 64, 120, 20, token_needle_a);

        frame_curr.fill(WHITE);
        draw_line_pixels(&mut frame_curr, 80, 64, 120, 20, CYAN);

        assert_damage_covers_all_pixel_changes(
            &mut grid,
            &frame_prev,
            &frame_curr,
            "Frame 6a (Needle Angle 1)",
        );

        frame_prev.copy_from_slice(&frame_curr);

        // 6b: Draw needle rotated to angle 2
        grid.clear_solid(WHITE);
        let token_needle_b = TileHasher::new(TOOL_LINE)
            .write_i32(80)
            .write_i32(64)
            .write_i32(120)
            .write_i32(40)
            .write_u16(CYAN)
            .finish();
        step_line_tiles(&mut grid, 80, 64, 120, 40, token_needle_b);

        frame_curr.fill(WHITE);
        draw_line_pixels(&mut frame_curr, 80, 64, 120, 40, CYAN);

        let (changed, dirty_tiles) = assert_damage_covers_all_pixel_changes(
            &mut grid,
            &frame_prev,
            &frame_curr,
            "Frame 6b (Needle Angle 2)",
        );
        assert!(changed > 0);
        assert!(dirty_tiles > 0);
    }
}
