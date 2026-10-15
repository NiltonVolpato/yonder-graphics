use yonder_damage::{
    make_primitive_token, stream_dirty_bands, DamageGrid320x240, Rect, SettleTimer,
};

#[test]
fn test_collision_absorption_via_closing() {
    let mut grid = DamageGrid320x240::new();
    grid.prev_hashes.fill(0);
    grid.active_hashes.fill(0);

    // Simulate an artificial collision in tile (5, 5):
    // Tiles (4, 5) and (6, 5) are dirty. Tile (5, 5) collided and remained 0.
    grid.touch_tile(4, 5, 0xAAAA);
    // tile (5, 5) left as 0 (simulating collision)
    grid.touch_tile(6, 5, 0xBBBB);

    let mut emitted_rects = [Rect::ZERO; 4];
    let mut count = 0;

    // Scratchpad capacity = 16 KB (8192 pixels)
    stream_dirty_bands(&grid, 8192, |rect| {
        emitted_rects[count] = rect;
        count += 1;
    });

    assert_eq!(count, 1, "The collision should be bridged into 1 rectangle");
    // Bridged span covers cols 4, 5, 6: x from 4*16=64 to 7*16=112 (width = 48)
    let rect = emitted_rects[0];
    assert_eq!(rect.x, 64);
    assert_eq!(rect.y, 80); // row 5 * 16 = 80
    assert_eq!(rect.width, 48); // 3 tiles * 16 = 48
    assert_eq!(rect.height, 16);
}

#[test]
fn test_multi_frame_damage_lifecycle() {
    let mut grid = DamageGrid320x240::new();
    let mut settle_timer = SettleTimer::new(3);

    // Frame 1: Initial startup -> all tiles dirty
    let mut frame1_count = 0;
    stream_dirty_bands(&grid, 8192, |_| {
        frame1_count += 1;
    });
    assert!(frame1_count > 0, "Initial frame must emit dirty rects");
    grid.swap_buffers();

    // Frame 2: Static scene with zero changes
    grid.reset_active();
    // Re-draw same scene state: touch same primitive in same place
    let token = make_primitive_token(1, 100, 100, 0xF800);
    grid.touch_rect(Rect::new(100, 100, 32, 32), token);
    // Since prev had same hashes (after swap), let's test zero diff:
    // If scene is completely static:
    grid.active_hashes.copy_from_slice(&grid.prev_hashes);

    let mut frame2_count = 0;
    stream_dirty_bands(&grid, 8192, |_| {
        frame2_count += 1;
    });
    assert_eq!(frame2_count, 0, "Static frame must produce ZERO dirty rects");

    let need_settle = settle_timer.update(false); // idle frame 1
    assert!(!need_settle);
    grid.swap_buffers();

    // Frame 3: Move a 16x16 widget
    grid.reset_active();
    let moved_token = make_primitive_token(1, 110, 100, 0xF800);
    grid.touch_rect(Rect::new(110, 100, 16, 16), moved_token);

    let mut frame3_count = 0;
    let mut frame3_rect = Rect::ZERO;
    stream_dirty_bands(&grid, 8192, |rect| {
        frame3_rect = rect;
        frame3_count += 1;
    });
    assert_eq!(frame3_count, 1);
    assert!(frame3_rect.width <= 48, "Only affected tiles should be dirty");
}

#[test]
fn test_band_streaming_capacity_invariants() {
    let mut grid = DamageGrid320x240::new();
    // Dirty entire 320x240 screen (76,800 pixels)
    grid.active_hashes.fill(0x1234);
    grid.prev_hashes.fill(0);

    const CAPACITY: usize = 4096; // 8 KB at RGB565

    let mut total_band_pixels = 0;
    let mut band_count = 0;

    stream_dirty_bands(&grid, CAPACITY, |band| {
        assert!(
            band.pixel_count() <= CAPACITY,
            "Band exceeds scratchpad capacity: {} > {}",
            band.pixel_count(),
            CAPACITY
        );
        total_band_pixels += band.pixel_count();
        band_count += 1;
    });

    assert_eq!(
        total_band_pixels,
        320 * 240,
        "Total streamed band pixels must equal screen area"
    );
    assert!(band_count >= (320 * 240) / CAPACITY);
}
