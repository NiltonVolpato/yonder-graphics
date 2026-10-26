use yonder_geometry::projective::PointT2;
use yonder_geometry::screen::{BoundingBox, Point};
use yonder_graphics::color::{Color, Rgb888};
use yonder_graphics::draw::Drawable;
use yonder_graphics::shapes::{Circle, Rectangle, Shape};
use yonder_graphics::style::{Paint, Stroke, Style};
use yonder_graphics::surface::{Footprint, Pixel, PixelBuffer, Surface, Viewport};

/// A whole-screen framebuffer whose viewport is the full screen.
struct Frame<const W: usize, const H: usize>([Rgb888; 64]);

impl<const W: usize, const H: usize> Frame<W, H> {
    fn new() -> Self {
        Self([Rgb888::BLACK; 64])
    }

    fn screen() -> BoundingBox {
        BoundingBox::new(Point::new(0, 0), Point::new(W as i16, H as i16))
    }

    /// One char per pixel, by red channel: `.` 0, `-` 64, `+` 128, `*` 191, `#` 255.
    fn picture(&self) -> Vec<String> {
        self.0[..W * H]
            .chunks(W)
            .map(|row| {
                row.iter()
                    .map(|p| match p.r() {
                        0 => '.',
                        64 => '-',
                        128 => '+',
                        191 => '*',
                        255 => '#',
                        _ => '?',
                    })
                    .collect()
            })
            .collect()
    }
}

impl<const W: usize, const H: usize> Surface for Frame<W, H> {
    type Color = Rgb888;

    fn viewport(&mut self, item: &impl Footprint) -> Option<impl PixelBuffer<Color = Rgb888>> {
        item.extent().intersection(&Self::screen())?;
        Some(Viewport::new(&mut self.0[..W * H], Self::screen()))
    }
}

fn rect(x0: i16, y0: i16, x1: i16, y1: i16) -> Rectangle {
    Rectangle::new(Point::new(x0, y0), Point::new(x1, y1))
}

fn gray(v: u8) -> Rgb888 {
    Rgb888::new(v, v, v)
}

#[test]
fn even_stroke_over_fill_clipped_to_surface() {
    let mut frame = Frame::<6, 6>::new();
    let style = Style::new()
        .fill(Paint::solid(gray(128)))
        .stroke(Stroke::new(2, Paint::solid(Rgb888::WHITE)));
    rect(0, 1, 4, 5).with_style(style).draw(&mut frame);
    assert_eq!(
        frame.picture(),
        ["#####.", "#####.", "#++##.", "#++##.", "#####.", "#####."]
    );
}

#[test]
fn odd_stroke_antialiases_half_pixels() {
    let mut frame = Frame::<7, 5>::new();
    rect(2, 1, 5, 4)
        .with_style(Style::new().stroke(Stroke::new(1, Paint::solid(Rgb888::WHITE))))
        .draw(&mut frame);
    assert_eq!(
        frame.picture(),
        [".-+++-.", ".+*+*+.", ".++.++.", ".+*+*+.", ".-+++-."]
    );
}

#[test]
fn translucent_fill_blends() {
    let mut frame = Frame::<2, 1>::new();
    rect(0, 0, 1, 1)
        .with_style(Style::new().fill(Paint::solid(Rgb888::WHITE).alpha(128)))
        .draw(&mut frame);
    assert_eq!(frame.picture(), ["+."]);
}

#[test]
fn off_surface_draws_nothing() {
    let mut frame = Frame::<4, 4>::new();
    rect(10, 10, 12, 12)
        .with_style(Style::new().fill(Paint::solid(Rgb888::WHITE)))
        .draw(&mut frame);
    Pixel {
        position: Point::new(9, 9),
        color: Rgb888::WHITE.with_alpha(255),
    }
    .draw(&mut frame);
    assert_eq!(frame.picture(), ["...."; 4]);
}

#[test]
fn pixel_draws_one_pixel() {
    let mut frame = Frame::<3, 1>::new();
    Pixel {
        position: Point::new(1, 0),
        color: Rgb888::WHITE.with_alpha(64),
    }
    .draw(&mut frame);
    assert_eq!(frame.picture(), [".-."]);
}

#[test]
fn styled_references_draw_too() {
    let mut frame = Frame::<2, 1>::new();
    fn draw_by_value<S: Surface>(item: impl Drawable<S>, surface: &mut S) {
        item.draw(surface);
    }
    let styled = rect(0, 0, 1, 1).with_style(Style::new().fill(Paint::solid(Rgb888::WHITE)));
    draw_by_value(&styled, &mut frame);
    assert_eq!(frame.picture(), ["#."]);
}

#[test]
fn extent_grows_by_half_stroke() {
    let styled = rect(0, 0, 4, 4)
        .with_style(Style::new().stroke(Stroke::new(3, Paint::solid(Rgb888::WHITE))));
    let e = styled.extent();
    assert_eq!(
        (e.top_left, e.bottom_right),
        (Point::new(-2, -2), Point::new(6, 6))
    );
}

fn corners(b: BoundingBox) -> (Point, Point) {
    (b.top_left, b.bottom_right)
}

#[test]
fn interior_needs_a_solid_fill() {
    let fill = |paint| {
        rect(0, 0, 10, 10)
            .with_style(Style::new().fill(paint))
            .interior()
    };
    let (inside, opaque) = fill(Paint::solid(Rgb888::WHITE)).unwrap();
    assert_eq!(corners(inside), (Point::new(0, 0), Point::new(10, 10)));
    assert!(opaque);
    assert!(!fill(Paint::solid(Rgb888::WHITE).alpha(200)).unwrap().1);

    let outline = Style::new().stroke(Stroke::new(9, Paint::solid(Rgb888::WHITE)));
    assert!(rect(0, 0, 10, 10).with_style(outline).interior().is_none());
}

#[test]
fn interior_leaves_room_for_the_stroke() {
    let style = Style::new()
        .fill(Paint::solid(Rgb888::WHITE))
        .stroke(Stroke::new(3, Paint::solid(Rgb888::RED)));
    let (inside, _) = rect(0, 0, 10, 10).with_style(style).interior().unwrap();
    assert_eq!(corners(inside), (Point::new(2, 2), Point::new(8, 8)));
}

#[test]
fn circle_interior_is_inside_the_circle() {
    for (circle, r) in [
        (Circle::new(PointT2::new(3, 3, 1), 4), 4.0),
        (
            Circle::new(PointT2::pixel_center(Point::new(-5, 7)), 9),
            9.0,
        ),
        (
            Circle::in_box(BoundingBox::new(Point::new(0, 0), Point::new(25, 25))),
            12.5,
        ),
    ] {
        let [x, y, w, _] = circle.homogeneous().map(f64::from);
        let inside = circle.interior().unwrap();
        let far =
            |p: Point| ((f64::from(p.x) - x / w).powi(2) + (f64::from(p.y) - y / w).powi(2)).sqrt();
        assert!(far(inside.top_left) <= r && far(inside.bottom_right) <= r);
        // The inscribed square has side r·√2; integer rounding may lose a pixel per side.
        assert!(
            f64::from(inside.width()) >= r * 2f64.sqrt() - 2.0,
            "{}",
            inside.width()
        );
    }
    assert!(Circle::new(PointT2::new(0, 0, 1), 0).interior().is_none());
}
