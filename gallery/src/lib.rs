//! Surfaces and scenes written the way a user of the library would write them.

use std::fs::File;
use std::io::BufWriter;
use std::marker::PhantomData;
use std::path::Path;

use yonder_graphics::prelude::*;
use yonder_graphics::surface::{Footprint, PixelBuffer, Surface, Viewport};

pub const BACKGROUND: Rgb888 = Rgb888::new(20, 22, 28);

/// A whole screen in memory. Its viewport is the whole screen.
pub struct Framebuffer<C> {
    pub width: i16,
    pub height: i16,
    pub pixels: Vec<C>,
}

impl<C: Color> Framebuffer<C> {
    pub fn new(width: i16, height: i16, background: C) -> Self {
        Self {
            width,
            height,
            pixels: vec![background; width as usize * height as usize],
        }
    }
}

impl<C: Color> Surface for Framebuffer<C> {
    type Color = C;

    fn viewport(&mut self, item: &impl Footprint) -> Option<impl PixelBuffer<Color = C>> {
        let screen = BoundingBox::new(Point::new(0, 0), Point::new(self.width, self.height));
        item.extent().intersection(&screen)?;
        Some(Viewport::new(&mut self.pixels, screen))
    }
}

/// A few rows of the screen, the way a device with a small buffer renders.
pub struct Band<C> {
    bounds: BoundingBox,
    pixels: Vec<C>,
}

impl<C: Color> Surface for Band<C> {
    type Color = C;

    fn viewport(&mut self, item: &impl Footprint) -> Option<impl PixelBuffer<Color = C>> {
        item.extent().intersection(&self.bounds)?;
        Some(Viewport::new(&mut self.pixels, self.bounds))
    }
}

/// Draws `scene` once per band of `band_height` rows and stitches the bands together.
pub fn render_in_bands<C: Color>(
    width: i16,
    height: i16,
    band_height: i16,
    background: C,
    scene: impl Fn(&mut Band<C>),
) -> Vec<C> {
    let mut out = Vec::with_capacity(width as usize * height as usize);
    for top in (0..height).step_by(band_height as usize) {
        let bottom = (top + band_height).min(height);
        let bounds = BoundingBox::new(Point::new(0, top), Point::new(width, bottom));
        let mut band = Band {
            bounds,
            pixels: vec![background; width as usize * (bottom - top) as usize],
        };
        scene(&mut band);
        out.extend(band.pixels);
    }
    out
}

/// The example from the design discussion, as a user would write it.
pub fn basic<S: Surface<Color = Rgb565>>(surface: &mut S) {
    let style = Style::new()
        .fill(Paint::solid(Rgb565::RED).alpha(166)) // 0.65 × 255 ≈ 166
        .stroke(Stroke::new(4, Paint::solid(Rgb565::YELLOW).alpha(128)));

    Rectangle::new(Point::new(10, 10), Point::new(110, 60))
        .with_style(style)
        .draw(surface);
}

/// Circles on lattice points and pixel centers, from bounding boxes, filled and stroked.
pub fn circles<S: Surface>(surface: &mut S)
where
    Rgb888: Into<S::Color>,
{
    let white = Style::new().fill(Paint::solid(Rgb888::WHITE));
    for r in 1..=8i16 {
        let x = 4 + r * (r + 1);
        Circle::new(PointT2::new(i32::from(x), 12, 1), r)
            .with_style(white)
            .draw(surface);
        let center = PointT2::pixel_center(Point::new(x, 34));
        Circle::new(center, r).with_style(white).draw(surface);
        let square = BoundingBox::new(Point::new(x - r, 48), Point::new(x + r + 1, 49 + 2 * r));
        Circle::in_box(square).with_style(white).draw(surface);
    }
    for (i, width) in (1..=4u16).enumerate() {
        let x = 20 + 40 * i as i32;
        let ring = Style::new().stroke(Stroke::new(width, Paint::solid(Rgb888::new(90, 200, 250))));
        Circle::new(PointT2::new(x, 92, 1), 14)
            .with_style(ring)
            .draw(surface);
    }
    let translucent = |c: Rgb888| Style::new().fill(Paint::solid(c).alpha(150));
    Circle::new(PointT2::new(200, 92, 1), 18)
        .with_style(translucent(Rgb888::RED))
        .draw(surface);
    Circle::new(PointT2::new(216, 92, 1), 18)
        .with_style(translucent(Rgb888::GREEN))
        .draw(surface);
    Circle::new(PointT2::new(208, 106, 1), 18)
        .with_style(translucent(Rgb888::BLUE))
        .draw(surface);
}

/// Rectangles with even and odd strokes over translucent fills.
pub fn strokes<S: Surface>(surface: &mut S)
where
    Rgb888: Into<S::Color>,
{
    for (i, width) in (0..=5u16).enumerate() {
        let x = 8 + 40 * i as i16;
        let style = Style::new()
            .fill(Paint::solid(Rgb888::new(245, 140, 40)).alpha(200))
            .stroke(Stroke::new(width, Paint::solid(Rgb888::WHITE).alpha(220)));
        Rectangle::new(Point::new(x, 8), Point::new(x + 28, 36))
            .with_style(style)
            .draw(surface);
    }
    for (i, alpha) in [64u8, 128, 192, 255].into_iter().enumerate() {
        let position = Point::new(10 + 6 * i as i16, 48);
        Pixel {
            position,
            color: Rgb888::WHITE.with_alpha(alpha),
        }
        .draw(surface);
    }
}

/// A widget whose style comes from its type, so it stores no style.
pub struct Button {
    pub bounds: Rectangle,
    pub pressed: bool,
}

pub trait Theme {
    fn button(pressed: bool) -> Style<Rgb888>;
}

pub struct Dark;

impl Dark {
    const RAISED: Style<Rgb888> = Style::new()
        .fill(Paint::solid(Rgb888::new(55, 145, 240)))
        .stroke(Stroke::new(2, Paint::solid(Rgb888::new(200, 220, 255))));
    const PRESSED: Style<Rgb888> = Style::new()
        .fill(Paint::solid(Rgb888::new(30, 90, 170)))
        .stroke(Stroke::new(2, Paint::solid(Rgb888::new(200, 220, 255))));
}

impl Theme for Dark {
    fn button(pressed: bool) -> Style<Rgb888> {
        if pressed { Self::PRESSED } else { Self::RAISED }
    }
}

pub struct WithTheme<W, T> {
    pub widget: W,
    theme: PhantomData<T>,
}

impl<W, T> WithTheme<W, T> {
    pub fn new(widget: W) -> Self {
        Self {
            widget,
            theme: PhantomData,
        }
    }
}

impl<T: Theme> Styled for WithTheme<Button, T> {
    type Shape = Rectangle;
    type Color = Rgb888;

    fn shape(&self) -> &Rectangle {
        &self.widget.bounds
    }

    fn style(&self) -> Style<Rgb888> {
        T::button(self.widget.pressed)
    }
}

pub fn buttons<S: Surface>(surface: &mut S)
where
    Rgb888: Into<S::Color>,
{
    for (i, pressed) in [false, true].into_iter().enumerate() {
        let x = 10 + 90 * i as i16;
        let bounds = Rectangle::new(Point::new(x, 10), Point::new(x + 80, 34));
        WithTheme::<_, Dark>::new(Button { bounds, pressed }).draw(surface);
    }
}

/// Writes `pixels` as a PNG, each pixel scaled to `zoom`×`zoom`, with a grid between
/// pixels when zoomed.
pub fn save_png(path: &Path, width: i16, pixels: &[Rgb888], zoom: usize) -> std::io::Result<()> {
    let (w, h) = (width as usize, pixels.len() / width as usize);
    let mut data = Vec::with_capacity(w * h * zoom * zoom * 3);
    for y in 0..h * zoom {
        for x in 0..w * zoom {
            let p = pixels[(y / zoom) * w + x / zoom];
            let grid = zoom > 1 && (x % zoom == 0 || y % zoom == 0);
            let shade = |c: u8| if grid { c / 2 + 20 } else { c };
            data.extend([shade(p.r()), shade(p.g()), shade(p.b())]);
        }
    }
    std::fs::create_dir_all(path.parent().unwrap())?;
    let mut encoder = png::Encoder::new(
        BufWriter::new(File::create(path)?),
        (w * zoom) as u32,
        (h * zoom) as u32,
    );
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&data)?;
    Ok(())
}
