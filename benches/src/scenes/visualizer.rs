//! Canonical multi-object animated scene from the damage visualizer.

use core::f32::consts::PI;

use embedded_graphics_core::geometry::{Point, Size};
use embedded_graphics_core::pixelcolor::Rgb888;
use embedded_graphics_core::primitives::Rectangle;
use yonder_core::Coord3;
use yonder_raster::{CompositingStyle, CompositingTarget, Conic, Triangle};

use crate::{BenchmarkPixel, Scene};

const CANVAS_WIDTH: usize = 360;

// Palette definitions (24-bit sRGB converted to target pixel format at runtime)
const COLOR_BG: Rgb888 = Rgb888::new(20, 22, 28);
const COLOR_CARD_BG: Rgb888 = Rgb888::new(32, 36, 48);
const COLOR_CARD_ACCENT: Rgb888 = Rgb888::new(50, 58, 78);
const COLOR_FLOOR: Rgb888 = Rgb888::new(40, 44, 58);
const COLOR_BALL: Rgb888 = Rgb888::new(245, 140, 40);
const COLOR_BALL_SPECULAR: Rgb888 = Rgb888::new(255, 220, 160);
const COLOR_BALL_SHADOW: Rgb888 = Rgb888::new(10, 12, 16);
const COLOR_NEEDLE: Rgb888 = Rgb888::new(240, 60, 80);
const COLOR_ACCENT_BLUE: Rgb888 = Rgb888::new(55, 145, 240);

/// Canonical animated scene featuring squash/stretch inertia ball, rotating triangle needle gauge,
/// oscillating translucent badge, and static backdrop card.
#[derive(Copy, Clone, Debug, Default)]
pub struct VisualizerScene;

impl<C: BenchmarkPixel> Scene<C> for VisualizerScene {
    fn name(&self) -> &str {
        "visualizer_scene"
    }

    fn render<T: CompositingTarget<Color = C>>(&self, target: &mut T, frame: usize) {
        // 1. Solid background clear
        let _ = target.clear(C::from(COLOR_BG));

        // 2. Static UI backdrop card
        let card_rect = Rectangle::new(Point::new(20, 20), Size::new(140, 95));
        let _ = target.fill_solid(&card_rect, C::from(COLOR_CARD_BG));
        let card_accent = Rectangle::new(Point::new(24, 24), Size::new(132, 4));
        let _ = target.fill_solid(&card_accent, C::from(COLOR_CARD_ACCENT));

        // 3. Ground floor line
        const FLOOR_Y: i32 = 300;
        let floor_rect = Rectangle::new(Point::new(0, FLOOR_Y), Size::new(CANVAS_WIDTH as u32, 2));
        let _ = target.fill_solid(&floor_rect, C::from(COLOR_FLOOR));

        // 4. Physics: Bouncing ball with squash-and-stretch
        const BALL_X: i32 = 100;
        const PEAK_Y: f32 = 70.0;
        const BASE_RADIUS: i32 = 26;

        let cycle_frame = frame % 70;
        let (center_y, rx, ry, shadow_rx, shadow_alpha) = if cycle_frame < 56 {
            let t = cycle_frame as f32 / 28.0;
            let h = 1.0 - (t - 1.0) * (t - 1.0);
            let y = FLOOR_Y as f32
                - (BASE_RADIUS as f32)
                - h * (FLOOR_Y as f32 - (BASE_RADIUS as f32) - PEAK_Y);
            let v = (2.0 * (1.0 - t)).abs();

            let (rx, ry) = if v > 1.6 {
                (23, 29)
            } else if v > 1.2 {
                (25, 27)
            } else {
                (BASE_RADIUS, BASE_RADIUS)
            };

            let shadow_rx = (18.0 + (1.0 - h) * 12.0) as i32;
            let shadow_alpha = (40.0 + (1.0 - h) * 80.0) as u8;
            (y.round() as i32, rx, ry, shadow_rx, shadow_alpha)
        } else {
            let s = (cycle_frame - 56) as f32 / 14.0;
            let squash_depth = (s * PI).sin();
            let ry = (26.0 - 13.0 * squash_depth).round() as i32;
            let rx = ((26.0 * 26.0) / (ry as f32)).round() as i32;
            let y = FLOOR_Y - ry;
            let shadow_rx = rx + 4;
            let shadow_alpha = (120.0 + squash_depth * 70.0) as u8;
            (y, rx, ry, shadow_rx, shadow_alpha)
        };

        // Shadow
        if shadow_rx > 0 {
            let _ = Conic::ellipse(BALL_X, FLOOR_Y + 4, shadow_rx, 5)
                .into_styled(CompositingStyle::with_fill_alpha(
                    C::from(COLOR_BALL_SHADOW),
                    shadow_alpha,
                ))
                .draw(target);
        }

        // Ball body
        let _ = Conic::ellipse(BALL_X, center_y, rx, ry)
            .into_styled(CompositingStyle::with_fill(C::from(COLOR_BALL)))
            .draw(target);

        // Specular highlight
        let spec_rx = (rx / 4).max(3);
        let spec_ry = (ry / 4).max(3);
        let _ = Conic::ellipse(BALL_X - rx / 3, center_y - ry / 3, spec_rx, spec_ry)
            .into_styled(CompositingStyle::with_fill_alpha(
                C::from(COLOR_BALL_SPECULAR),
                190,
            ))
            .draw(target);

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

        let _ = Triangle::new(
            Coord3::point(tip_x, tip_y),
            Coord3::point(b1_x, b1_y),
            Coord3::point(b2_x, b2_y),
        )
        .into_styled(CompositingStyle::with_fill(C::from(COLOR_NEEDLE)))
        .draw(target);

        let _ = Conic::circle(pivot_x, pivot_y, 7)
            .into_styled(CompositingStyle::with_fill(C::from(COLOR_ACCENT_BLUE)))
            .draw(target);

        // 6. Horizontally oscillating translucent card badge
        let badge_x = 200 + ((frame as f32 * 0.05).sin() * 25.0).round() as i32;
        let badge_rect = Rectangle::new(Point::new(badge_x, 235), Size::new(85, 28));
        let _ = target.blend_solid(&badge_rect, C::from(COLOR_ACCENT_BLUE), 130);
    }
}
