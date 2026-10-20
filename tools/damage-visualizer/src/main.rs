//! Interactive Damage Visualizer for `yonder-graphics`.
//!
//! Visualizes spatial tile damage tracking, coalesced dirty rectangles,
//! and alpha-blended immediate-mode rendering using `winit` and `softbuffer`.

use std::io::Write;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant};

use embedded_graphics::mono_font::ascii::FONT_6X10;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::primitives::{Primitive, PrimitiveStyle};
use embedded_graphics::text::Text;
use embedded_graphics_core::geometry::{Point, Size};
use embedded_graphics_core::pixelcolor::Rgb888;
use embedded_graphics_core::primitives::Rectangle;
use embedded_graphics_core::Drawable;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowAttributes, WindowId};
use yonder_core::Coord3;
use yonder_damage::{coalesce_dirty_rects, DamageGridStorage360x360, Rect};
use yonder_raster::{CompositingStyle, CompositingTarget, Conic, Triangle, Viewport};

const CANVAS_WIDTH: usize = 360;
const CANVAS_HEIGHT: usize = 360;
const SCALE: usize = 2;
const WINDOW_WIDTH: u32 = (CANVAS_WIDTH * SCALE) as u32;
const WINDOW_HEIGHT: u32 = (CANVAS_HEIGHT * SCALE) as u32;

const COLS: usize = 23;
const ROWS: usize = 23;
const TOTAL_TILES: usize = COLS * ROWS;

// Visualizer Palette
const COLOR_BG: Rgb888 = Rgb888::new(20, 22, 28);
const COLOR_CARD_BG: Rgb888 = Rgb888::new(32, 36, 48);
const COLOR_CARD_ACCENT: Rgb888 = Rgb888::new(50, 58, 78);
const COLOR_FLOOR: Rgb888 = Rgb888::new(40, 44, 58);
const COLOR_BALL: Rgb888 = Rgb888::new(245, 140, 40);
const COLOR_BALL_SPECULAR: Rgb888 = Rgb888::new(255, 220, 160);
const COLOR_BALL_SHADOW: Rgb888 = Rgb888::new(10, 12, 16);
const COLOR_NEEDLE: Rgb888 = Rgb888::new(240, 60, 80);
const COLOR_ACCENT_BLUE: Rgb888 = Rgb888::new(55, 145, 240);
const COLOR_GRID_LINE: Rgb888 = Rgb888::new(200, 215, 245);
const COLOR_DIRTY_TINT: Rgb888 = Rgb888::new(255, 55, 55);
const COLOR_COALESCE_BORDER: Rgb888 = Rgb888::new(45, 255, 115);
const COLOR_HUD_BG: Rgb888 = Rgb888::new(14, 16, 22);
const COLOR_HUD_TEXT: Rgb888 = Rgb888::new(235, 242, 255);

/// Nearest-neighbor integer 2x blitter from $360 \times 360$ canvas to $720 \times 720$ window buffer.
fn blit_2x(src: &[u32], dst: &mut [u32]) {
    for y in 0..CANVAS_HEIGHT {
        let src_row = &src[y * CANVAS_WIDTH..(y + 1) * CANVAS_WIDTH];
        let y0_offset = (2 * y) * (WINDOW_WIDTH as usize);
        let y1_offset = (2 * y + 1) * (WINDOW_WIDTH as usize);
        for (x, &pixel) in src_row.iter().enumerate() {
            let x2 = 2 * x;
            dst[y0_offset + x2] = pixel;
            dst[y0_offset + x2 + 1] = pixel;
            dst[y1_offset + x2] = pixel;
            dst[y1_offset + x2 + 1] = pixel;
        }
    }
}

/// Immediate-mode scene renderer executed on both `DamageGrid` and `Viewport`.
fn render_scene<T: CompositingTarget<Color = Rgb888>>(
    target: &mut T,
    frame: usize,
) -> Result<(), T::Error> {
    // 1. Solid background clear
    target.clear(COLOR_BG)?;

    // 2. Static UI backdrop card (exercises zero-damage verification on static tiles)
    let card_rect = Rectangle::new(Point::new(20, 20), Size::new(140, 95));
    card_rect
        .into_styled(PrimitiveStyle::with_fill(COLOR_CARD_BG))
        .draw(target)?;

    let sub_bar = Rectangle::new(Point::new(30, 32), Size::new(80, 8));
    sub_bar
        .into_styled(PrimitiveStyle::with_fill(COLOR_CARD_ACCENT))
        .draw(target)?;

    let sub_card = Rectangle::new(Point::new(30, 48), Size::new(120, 20));
    sub_card
        .into_styled(PrimitiveStyle::with_fill(COLOR_CARD_ACCENT))
        .draw(target)?;

    // 3. Ground platform
    let floor_rect = Rectangle::new(Point::new(0, 290), Size::new(360, 70));
    floor_rect
        .into_styled(PrimitiveStyle::with_fill(COLOR_FLOOR))
        .draw(target)?;

    let floor_accent = Rectangle::new(Point::new(0, 290), Size::new(360, 2));
    floor_accent
        .into_styled(PrimitiveStyle::with_fill(COLOR_CARD_ACCENT))
        .draw(target)?;

    // 4. Squash-and-stretch bouncing ball physics
    const FLOOR_Y: i32 = 290;
    const CEILING_Y: i32 = 65;
    const BALL_X: i32 = 105;
    const BASE_RADIUS: i32 = 26;
    const PERIOD: usize = 70;

    let cycle_frame = frame % PERIOD;
    let (center_y, rx, ry, shadow_rx, shadow_alpha) = if cycle_frame < 56 {
        // Airborne phase: parabolic arc under simulated gravity
        let t = cycle_frame as f32 / 56.0;
        let h = 4.0 * t * (1.0 - t); // 0.0 -> 1.0 -> 0.0
        let v = 4.0 * (1.0 - 2.0 * t); // > 0 rising, < 0 falling

        let travel = (FLOOR_Y - CEILING_Y - BASE_RADIUS) as f32;
        let y = (FLOOR_Y - BASE_RADIUS) as f32 - h * travel;

        // Subtle inertial stretch along the velocity axis
        let (rx, ry) = if v < -1.2 {
            (24, 28) // Stretching vertically while falling
        } else if v > 1.2 {
            (25, 27) // Stretching slightly while rising
        } else {
            (BASE_RADIUS, BASE_RADIUS) // Symmetrical at peak
        };

        // Shadow expands and darkens as ball approaches floor
        let shadow_rx = (18.0 + (1.0 - h) * 12.0) as i32;
        let shadow_alpha = (40.0 + (1.0 - h) * 80.0) as u8;

        (y.round() as i32, rx, ry, shadow_rx, shadow_alpha)
    } else {
        // Ground impact & inertial squash phase (14 frames)
        let s = (cycle_frame - 56) as f32 / 14.0;
        let squash_depth = (s * std::f32::consts::PI).sin();

        let ry = (26.0 - 13.0 * squash_depth).round() as i32; // Drops down to 13px
        let rx = ((26.0 * 26.0) / (ry as f32)).round() as i32; // Stretches horizontally up to 52px
        let y = FLOOR_Y - ry; // Bottom edge anchored firmly at floor

        let shadow_rx = rx + 4;
        let shadow_alpha = (120.0 + squash_depth * 70.0) as u8;

        (y, rx, ry, shadow_rx, shadow_alpha)
    };

    // Ground contact shadow (alpha-blended Conic ellipse)
    if shadow_rx > 0 {
        Conic::ellipse(BALL_X, FLOOR_Y + 4, shadow_rx, 5)
            .into_styled(CompositingStyle::with_fill_alpha(
                COLOR_BALL_SHADOW,
                shadow_alpha,
            ))
            .draw(target)?;
    }

    // Main bouncing ball (Conic ellipse dynamically transitioning to circle)
    Conic::ellipse(BALL_X, center_y, rx, ry)
        .into_styled(CompositingStyle::with_fill(COLOR_BALL))
        .draw(target)?;

    // Specular highlight on ball
    let spec_rx = (rx / 4).max(3);
    let spec_ry = (ry / 4).max(3);
    Conic::ellipse(BALL_X - rx / 3, center_y - ry / 3, spec_rx, spec_ry)
        .into_styled(CompositingStyle::with_fill_alpha(COLOR_BALL_SPECULAR, 190))
        .draw(target)?;

    // 5. Rotating projective triangle needle
    let pivot_x = 270;
    let pivot_y = 135;
    let theta = (frame as f32) * 0.045;
    let r_tip = 55.0;
    let r_base = 12.0;

    let tip_x = pivot_x + (r_tip * theta.cos()).round() as i32;
    let tip_y = pivot_y + (r_tip * theta.sin()).round() as i32;

    let b1_x = pivot_x + (r_base * (theta + 2.5).cos()).round() as i32;
    let b1_y = pivot_y + (r_base * (theta + 2.5).sin()).round() as i32;

    let b2_x = pivot_x + (r_base * (theta - 2.5).cos()).round() as i32;
    let b2_y = pivot_y + (r_base * (theta - 2.5).sin()).round() as i32;

    Triangle::new(
        Coord3::point(tip_x, tip_y),
        Coord3::point(b1_x, b1_y),
        Coord3::point(b2_x, b2_y),
    )
    .into_styled(CompositingStyle::with_fill(COLOR_NEEDLE))
    .draw(target)?;

    // Needle pivot hub
    Conic::circle(pivot_x, pivot_y, 7)
        .into_styled(CompositingStyle::with_fill(COLOR_ACCENT_BLUE))
        .draw(target)?;

    // 6. Horizontally oscillating translucent card badge
    let badge_x = 200 + ((frame as f32 * 0.05).sin() * 25.0).round() as i32;
    let badge_rect = Rectangle::new(Point::new(badge_x, 235), Size::new(85, 28));
    target.blend_solid(&badge_rect, COLOR_ACCENT_BLUE, 130)?;

    Ok(())
}

struct VisualizerApp {
    window: Option<Arc<Window>>,
    context: Option<softbuffer::Context<Arc<Window>>>,
    surface: Option<softbuffer::Surface<Arc<Window>, Arc<Window>>>,

    // Application state
    frame: usize,
    playing: bool,
    show_grid: bool,
    show_coalesce: bool,
    last_frame_instant: Instant,
    last_fps_instant: Instant,
    frame_count_since_fps: usize,
    current_fps: f64,

    // Buffers and storage
    canvas: Vec<u32>,
    damage_storage: DamageGridStorage360x360,
}

impl VisualizerApp {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            window: None,
            context: None,
            surface: None,
            frame: 0,
            playing: true,
            show_grid: true,
            show_coalesce: true,
            last_frame_instant: now,
            last_fps_instant: now,
            frame_count_since_fps: 0,
            current_fps: 60.0,
            canvas: vec![0u32; CANVAS_WIDTH * CANVAS_HEIGHT],
            damage_storage: DamageGridStorage360x360::new(),
        }
    }

    fn step_forward(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    fn step_backward(&mut self) {
        self.frame = self.frame.saturating_sub(1);
    }

    fn reset(&mut self) {
        let now = Instant::now();
        self.frame = 0;
        self.last_frame_instant = now;
        self.last_fps_instant = now;
        self.frame_count_since_fps = 0;
        self.current_fps = 60.0;
        self.damage_storage = DamageGridStorage360x360::new();
    }

    fn render_and_present(&mut self) {
        let surface = match self.surface.as_mut() {
            Some(s) => s,
            None => return,
        };

        let mut grid = self.damage_storage.as_grid();
        let bounds = Rect::new(0, 0, CANVAS_WIDTH as u32, CANVAS_HEIGHT as u32);

        // Update rolling FPS counter
        self.frame_count_since_fps += 1;
        let now = Instant::now();
        let fps_elapsed = now.duration_since(self.last_fps_instant);
        if fps_elapsed >= Duration::from_millis(500) {
            self.current_fps = (self.frame_count_since_fps as f64) / fps_elapsed.as_secs_f64();
            self.frame_count_since_fps = 0;
            self.last_fps_instant = now;
        }

        // -----------------------------------------------------------------
        // PASS 1: Spatial Damage Pass
        // -----------------------------------------------------------------
        render_scene(&mut grid.as_target::<Rgb888>(), self.frame).unwrap();

        // -----------------------------------------------------------------
        // PASS 2: Viewport Base Scene Pass
        // -----------------------------------------------------------------
        let mut viewport = Viewport::<Rgb888>::new(&mut self.canvas, bounds);
        render_scene(&mut viewport, self.frame).unwrap();

        // Collect dirty tile coordinates
        let mut dirty_tiles = Vec::with_capacity(TOTAL_TILES);
        for r in 0..ROWS {
            for c in 0..COLS {
                if grid.is_tile_dirty(c, r) {
                    dirty_tiles.push((c, r));
                }
            }
        }
        let dirty_count = dirty_tiles.len();

        // -----------------------------------------------------------------
        // PASS 3: Coalesce Dirty Rectangles
        // -----------------------------------------------------------------
        let mut coalesced_rects = Vec::new();
        coalesce_dirty_rects(&mut grid, |rect| {
            coalesced_rects.push(rect);
        });
        let coalesced_count = coalesced_rects.len();

        // -----------------------------------------------------------------
        // PASS 4: Visualizer Overlays ("Yonder All the Way Down")
        // -----------------------------------------------------------------
        if self.show_grid {
            // Draw tile grid lines
            for col in 1..COLS {
                let x = (col * 16) as i32;
                let vline = Rectangle::new(Point::new(x, 0), Size::new(1, CANVAS_HEIGHT as u32));
                let _ = viewport.blend_solid(&vline, COLOR_GRID_LINE, 28);
            }
            for row in 1..ROWS {
                let y = (row * 16) as i32;
                let hline = Rectangle::new(Point::new(0, y), Size::new(CANVAS_WIDTH as u32, 1));
                let _ = viewport.blend_solid(&hline, COLOR_GRID_LINE, 28);
            }

            // Draw translucent red heatmap over dirty tiles
            for (c, r) in dirty_tiles {
                let x = (c * 16) as i32;
                let y = (r * 16) as i32;
                let w = (16).min(CANVAS_WIDTH as u32 - x as u32);
                let h = (16).min(CANVAS_HEIGHT as u32 - y as u32);
                let tile_rect = Rectangle::new(Point::new(x, y), Size::new(w, h));
                let _ = viewport.blend_solid(&tile_rect, COLOR_DIRTY_TINT, 75);
            }
        }

        if self.show_coalesce {
            // Draw bright wireframe borders around each coalesced rectangle
            for rect in coalesced_rects {
                let x = rect.x;
                let y = rect.y;
                let w = rect.width;
                let h = rect.height;

                // 2px top and bottom borders
                let top = Rectangle::new(Point::new(x, y), Size::new(w, 2));
                let bot = Rectangle::new(Point::new(x, (y + h as i32 - 2).max(y)), Size::new(w, 2));
                // 2px left and right borders
                let left = Rectangle::new(Point::new(x, y), Size::new(2, h));
                let right =
                    Rectangle::new(Point::new((x + w as i32 - 2).max(x), y), Size::new(2, h));

                let _ = viewport.blend_solid(&top, COLOR_COALESCE_BORDER, 255);
                let _ = viewport.blend_solid(&bot, COLOR_COALESCE_BORDER, 255);
                let _ = viewport.blend_solid(&left, COLOR_COALESCE_BORDER, 255);
                let _ = viewport.blend_solid(&right, COLOR_COALESCE_BORDER, 255);
            }
        }

        // Draw in-window HUD overlay banner
        let hud_banner = Rectangle::new(Point::new(6, 6), Size::new(CANVAS_WIDTH as u32 - 12, 16));
        let _ = viewport.blend_solid(&hud_banner, COLOR_HUD_BG, 220);

        let status = if self.playing { "PLAY" } else { "PAUSE" };
        let dirty_pct = (dirty_count as f64 / TOTAL_TILES as f64) * 100.0;
        let hud_text = format!(
            "F:{:04} {:4.1}FPS D:{}/{} ({:.0}%) R:{} [{}]",
            self.frame,
            self.current_fps,
            dirty_count,
            TOTAL_TILES,
            dirty_pct,
            coalesced_count,
            status
        );
        let text_style = MonoTextStyle::new(&FONT_6X10, COLOR_HUD_TEXT);
        let _ = Text::new(&hud_text, Point::new(12, 18), text_style).draw(&mut viewport);

        // Terminal telemetry line
        print!(
            "\r[Yonder Visualizer] Frame: {:04} | FPS: {:4.1} | Dirty: {:2}/{} ({:4.1}%) | Rects: {:2} | {:5}   ",
            self.frame,
            self.current_fps,
            dirty_count,
            TOTAL_TILES,
            dirty_pct,
            coalesced_count,
            status
        );
        let _ = std::io::stdout().flush();

        // -----------------------------------------------------------------
        // PASS 5: 2x Integer Scale Blit & Present
        // -----------------------------------------------------------------
        let mut window_buffer = surface.buffer_mut().unwrap();
        blit_2x(&self.canvas, &mut window_buffer);
        window_buffer.present().unwrap();
    }
}

impl ApplicationHandler for VisualizerApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attributes = WindowAttributes::default()
            .with_title("Yonder Damage Visualizer")
            .with_inner_size(LogicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT))
            .with_resizable(false);

        let window = Arc::new(event_loop.create_window(attributes).unwrap());
        let context = softbuffer::Context::new(window.clone()).unwrap();
        let mut surface = softbuffer::Surface::new(&context, window.clone()).unwrap();

        let width = NonZeroU32::new(WINDOW_WIDTH).unwrap();
        let height = NonZeroU32::new(WINDOW_HEIGHT).unwrap();
        surface.resize(width, height).unwrap();

        self.window = Some(window);
        self.context = Some(context);
        self.surface = Some(surface);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        state: ElementState::Pressed,
                        logical_key,
                        ..
                    },
                ..
            } => match logical_key {
                Key::Named(NamedKey::Space) => {
                    self.playing = !self.playing;
                    if self.playing {
                        self.last_frame_instant = Instant::now();
                    }
                    if let Some(w) = self.window.as_ref() {
                        w.request_redraw();
                    }
                }
                Key::Named(NamedKey::ArrowRight) => {
                    self.playing = false;
                    self.step_forward();
                    if let Some(w) = self.window.as_ref() {
                        w.request_redraw();
                    }
                }
                Key::Named(NamedKey::ArrowLeft) => {
                    self.playing = false;
                    self.step_backward();
                    if let Some(w) = self.window.as_ref() {
                        w.request_redraw();
                    }
                }
                Key::Character(ref ch) if ch.eq_ignore_ascii_case("p") => {
                    self.playing = !self.playing;
                    if self.playing {
                        self.last_frame_instant = Instant::now();
                    }
                    if let Some(w) = self.window.as_ref() {
                        w.request_redraw();
                    }
                }
                Key::Character(ref ch) if ch.eq_ignore_ascii_case("g") => {
                    self.show_grid = !self.show_grid;
                    if let Some(w) = self.window.as_ref() {
                        w.request_redraw();
                    }
                }
                Key::Character(ref ch) if ch.eq_ignore_ascii_case("c") => {
                    self.show_coalesce = !self.show_coalesce;
                    if let Some(w) = self.window.as_ref() {
                        w.request_redraw();
                    }
                }
                Key::Character(ref ch) if ch.eq_ignore_ascii_case("r") => {
                    self.reset();
                    if let Some(w) = self.window.as_ref() {
                        w.request_redraw();
                    }
                }
                _ => {}
            },
            WindowEvent::RedrawRequested => {
                self.render_and_present();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.playing {
            let target_frame_duration = Duration::from_micros(16_667); // 60.0 FPS
            let now = Instant::now();
            let elapsed = now.duration_since(self.last_frame_instant);

            if elapsed >= target_frame_duration {
                self.step_forward();
                self.last_frame_instant = now;
                if let Some(w) = self.window.as_ref() {
                    w.request_redraw();
                }
                event_loop.set_control_flow(ControlFlow::WaitUntil(now + target_frame_duration));
            } else {
                let remaining = target_frame_duration - elapsed;
                event_loop.set_control_flow(ControlFlow::WaitUntil(now + remaining));
            }
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }
}

fn main() {
    println!("=== Yonder Damage Visualizer ===");
    println!("Controls:");
    println!("  Space / P   : Play / Pause continuous animation");
    println!("  Right Arrow : Step forward 1 frame (pauses)");
    println!("  Left Arrow  : Step backward 1 frame (pauses)");
    println!("  G           : Toggle 16x16 tile grid & dirty heatmap");
    println!("  C           : Toggle coalesced rectangle wireframes");
    println!("  R           : Reset animation to frame 0");
    println!("================================");

    let event_loop = EventLoop::new().unwrap();
    let mut app = VisualizerApp::new();
    let _ = event_loop.run_app(&mut app);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_frames_coalesce_without_panic() {
        let mut storage = DamageGridStorage360x360::new();
        let mut grid = storage.as_grid();

        for frame in 0..200 {
            render_scene(&mut grid.as_target::<Rgb888>(), frame).unwrap();
            let mut count = 0;
            coalesce_dirty_rects(&mut grid, |_rect| {
                count += 1;
            });
            assert!(count > 0, "Frame {} should have coalesced damage", frame);
        }
    }

    #[test]
    fn test_blit_2x_pixel_duplication() {
        let mut src = vec![0u32; CANVAS_WIDTH * CANVAS_HEIGHT];
        src[0] = 0x00FF0000; // Red at (0, 0)
        src[1] = 0x0000FF00; // Green at (1, 0)
        src[CANVAS_WIDTH] = 0x000000FF; // Blue at (0, 1)

        let mut dst = vec![0u32; (WINDOW_WIDTH * WINDOW_HEIGHT) as usize];
        blit_2x(&src, &mut dst);

        let w = WINDOW_WIDTH as usize;
        // (0, 0) in src should map to (0, 0), (1, 0), (0, 1), (1, 1) in dst
        assert_eq!(dst[0], 0x00FF0000);
        assert_eq!(dst[1], 0x00FF0000);
        assert_eq!(dst[w], 0x00FF0000);
        assert_eq!(dst[w + 1], 0x00FF0000);

        // (1, 0) in src should map to (2, 0), (3, 0), (2, 1), (3, 1) in dst
        assert_eq!(dst[2], 0x0000FF00);
        assert_eq!(dst[3], 0x0000FF00);
        assert_eq!(dst[w + 2], 0x0000FF00);
        assert_eq!(dst[w + 3], 0x0000FF00);

        // (0, 1) in src should map to (0, 2), (1, 2), (0, 3), (1, 3) in dst
        assert_eq!(dst[2 * w], 0x000000FF);
        assert_eq!(dst[2 * w + 1], 0x000000FF);
        assert_eq!(dst[3 * w], 0x000000FF);
        assert_eq!(dst[3 * w + 1], 0x000000FF);
    }

    #[test]
    fn test_render_scene_airborne_and_impact_phases() {
        let bounds = Rect::new(0, 0, CANVAS_WIDTH as u32, CANVAS_HEIGHT as u32);
        let mut canvas = vec![0u32; CANVAS_WIDTH * CANVAS_HEIGHT];

        // Airborne frame (frame 20)
        {
            let mut viewport = Viewport::<Rgb888>::new(&mut canvas, bounds);
            render_scene(&mut viewport, 20).unwrap();
            assert_ne!(viewport.buffer()[0], 0); // Background cleared
        }

        // Impact & squash frame (frame 62)
        {
            let mut viewport = Viewport::<Rgb888>::new(&mut canvas, bounds);
            render_scene(&mut viewport, 62).unwrap();
            assert_ne!(viewport.buffer()[0], 0);
        }

        // DamageGrid pass on both frames
        let mut storage = DamageGridStorage360x360::new();
        let mut grid = storage.as_grid();
        render_scene(&mut grid.as_target::<Rgb888>(), 20).unwrap();
        render_scene(&mut grid.as_target::<Rgb888>(), 62).unwrap();
    }

    #[test]
    fn test_app_navigation_and_reset() {
        let mut app = VisualizerApp::new();
        assert_eq!(app.frame, 0);
        assert!(app.playing);

        app.step_forward();
        assert_eq!(app.frame, 1);

        app.step_backward();
        assert_eq!(app.frame, 0);

        app.step_backward(); // Saturating sub
        assert_eq!(app.frame, 0);

        app.frame = 100;
        app.reset();
        assert_eq!(app.frame, 0);
    }
}
