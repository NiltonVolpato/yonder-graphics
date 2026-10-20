//! Focused scene isolating the deformed conic ball and dynamic alpha shadow.

use core::f32::consts::PI;

use embedded_graphics_core::geometry::{Point, Size};
use embedded_graphics_core::pixelcolor::Rgb888;
use embedded_graphics_core::primitives::Rectangle;
use yonder_raster::{CompositingStyle, CompositingTarget, Conic};

use crate::{BenchmarkPixel, Scene};

const CANVAS_WIDTH: usize = 360;

const COLOR_BG: Rgb888 = Rgb888::new(20, 22, 28);
const COLOR_FLOOR: Rgb888 = Rgb888::new(40, 44, 58);
const COLOR_BALL: Rgb888 = Rgb888::new(245, 140, 40);
const COLOR_BALL_SPECULAR: Rgb888 = Rgb888::new(255, 220, 160);
const COLOR_BALL_SHADOW: Rgb888 = Rgb888::new(10, 12, 16);

/// Isolated scene featuring only the bouncing squash-and-stretch ball and alpha shadow.
#[derive(Copy, Clone, Debug, Default)]
pub struct BouncingBallScene;

impl<C: BenchmarkPixel> Scene<C> for BouncingBallScene {
    fn name(&self) -> &str {
        "bouncing_ball_scene"
    }

    fn render<T: CompositingTarget<Color = C>>(&self, target: &mut T, frame: usize) {
        let _ = target.clear(C::from(COLOR_BG));

        const FLOOR_Y: i32 = 300;
        let floor_rect = Rectangle::new(Point::new(0, FLOOR_Y), Size::new(CANVAS_WIDTH as u32, 2));
        let _ = target.fill_solid(&floor_rect, C::from(COLOR_FLOOR));

        const BALL_X: i32 = 180;
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

        if shadow_rx > 0 {
            let _ = Conic::ellipse(BALL_X, FLOOR_Y + 4, shadow_rx, 5)
                .into_styled(CompositingStyle::with_fill_alpha(
                    C::from(COLOR_BALL_SHADOW),
                    shadow_alpha,
                ))
                .draw(target);
        }

        let _ = Conic::ellipse(BALL_X, center_y, rx, ry)
            .into_styled(CompositingStyle::with_fill(C::from(COLOR_BALL)))
            .draw(target);

        let spec_rx = (rx / 4).max(3);
        let spec_ry = (ry / 4).max(3);
        let _ = Conic::ellipse(BALL_X - rx / 3, center_y - ry / 3, spec_rx, spec_ry)
            .into_styled(CompositingStyle::with_fill_alpha(
                C::from(COLOR_BALL_SPECULAR),
                190,
            ))
            .draw(target);
    }
}
