use std::fs::File;
use std::io::{BufWriter, Write};

use embedded_graphics::primitives::{
    Circle as EgCircle, Ellipse as EgEllipse, Primitive, PrimitiveStyle, Triangle as EgTriangle,
};
use embedded_graphics_core::draw_target::DrawTarget;
use embedded_graphics_core::geometry::{Point, Size};
use embedded_graphics_core::pixelcolor::Rgb888;
use embedded_graphics_core::Drawable;
use yonder_core::Coord3;
use yonder_damage::Rect;
use yonder_raster::{CompositingStyle, Conic, Triangle as YonderTriangle, Viewport};

// -----------------------------------------------------------------------------
// Scene Specification (Single Source of Truth)
// -----------------------------------------------------------------------------
const WIDTH: u32 = 1024;
const HEIGHT: u32 = 1024;
const PIXEL_COUNT: usize = (WIDTH * HEIGHT) as usize;

#[derive(Copy, Clone)]
struct ColorRgb {
    r: u8,
    g: u8,
    b: u8,
}

impl ColorRgb {
    const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    const fn to_rgb888(self) -> Rgb888 {
        Rgb888::new(self.r, self.g, self.b)
    }
}

// Background
const COLOR_WHITE: ColorRgb = ColorRgb::new(255, 255, 255);

// Cyan circle
const COLOR_CYAN: ColorRgb = ColorRgb::new(0, 205, 255);
const ALPHA_CYAN: u8 = 160;
const CYAN_CENTER_X: i32 = 370;
const CYAN_CENTER_Y: i32 = 440;
const CYAN_RADIUS: i32 = 200;

// Magenta circle
const COLOR_MAGENTA: ColorRgb = ColorRgb::new(255, 30, 160);
const ALPHA_MAGENTA: u8 = 160;
const MAGENTA_CENTER_X: i32 = 654;
const MAGENTA_CENTER_Y: i32 = 440;
const MAGENTA_RADIUS: i32 = 200;

// Yellow triangle
const COLOR_YELLOW: ColorRgb = ColorRgb::new(255, 235, 0);
const ALPHA_TRIANGLE: u8 = 180;
const TRI_P1: (i32, i32) = (512, 170);
const TRI_P2: (i32, i32) = (220, 770);
const TRI_P3: (i32, i32) = (804, 770);

// Emerald ellipse
const COLOR_EMERALD: ColorRgb = ColorRgb::new(0, 220, 80);
const ALPHA_ELLIPSE: u8 = 140;
const ELLIPSE_CENTER_X: i32 = 512;
const ELLIPSE_CENTER_Y: i32 = 690;
const ELLIPSE_RX: i32 = 280;
const ELLIPSE_RY: i32 = 140;

// -----------------------------------------------------------------------------
// Exporters
// -----------------------------------------------------------------------------

/// Serializes a 32-bit XRGB8888 pixel buffer to a 24-bit RGB PNG file.
fn save_png(
    path: &str,
    buffer: &[u32],
    width: u32,
    height: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut rgb888_bytes = Vec::with_capacity((width * height * 3) as usize);
    for &pixel in buffer {
        rgb888_bytes.extend_from_slice(&[
            ((pixel >> 16) & 0xFF) as u8,
            ((pixel >> 8) & 0xFF) as u8,
            (pixel & 0xFF) as u8,
        ]);
    }

    let file = File::create(path)?;
    let buf_writer = &mut BufWriter::new(file);

    let mut encoder = png::Encoder::new(buf_writer, width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&rgb888_bytes)?;

    println!("Saved PNG: {} ({}x{})", path, width, height);
    Ok(())
}

/// Generates an exact SVG document using the scene constants.
fn save_svg(path: &str, width: u32, height: u32) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);

    writeln!(
        writer,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" width="{width}" height="{height}">"#
    )?;

    // 1. White paper background
    writeln!(
        writer,
        r#"  <rect width="{width}" height="{height}" fill="rgb({}, {}, {})"/>"#,
        COLOR_WHITE.r, COLOR_WHITE.g, COLOR_WHITE.b
    )?;

    // 2. Cyan circle
    let opacity_cyan = ALPHA_CYAN as f32 / 255.0;
    writeln!(
        writer,
        r#"  <circle cx="{CYAN_CENTER_X}" cy="{CYAN_CENTER_Y}" r="{CYAN_RADIUS}" fill="rgb({}, {}, {})" fill-opacity="{opacity_cyan:.5}"/>"#,
        COLOR_CYAN.r, COLOR_CYAN.g, COLOR_CYAN.b
    )?;

    // 3. Magenta circle
    let opacity_magenta = ALPHA_MAGENTA as f32 / 255.0;
    writeln!(
        writer,
        r#"  <circle cx="{MAGENTA_CENTER_X}" cy="{MAGENTA_CENTER_Y}" r="{MAGENTA_RADIUS}" fill="rgb({}, {}, {})" fill-opacity="{opacity_magenta:.5}"/>"#,
        COLOR_MAGENTA.r, COLOR_MAGENTA.g, COLOR_MAGENTA.b
    )?;

    // 4. Yellow triangle
    let opacity_tri = ALPHA_TRIANGLE as f32 / 255.0;
    writeln!(
        writer,
        r#"  <polygon points="{},{} {},{} {},{}" fill="rgb({}, {}, {})" fill-opacity="{opacity_tri:.5}"/>"#,
        TRI_P1.0,
        TRI_P1.1,
        TRI_P2.0,
        TRI_P2.1,
        TRI_P3.0,
        TRI_P3.1,
        COLOR_YELLOW.r,
        COLOR_YELLOW.g,
        COLOR_YELLOW.b
    )?;

    // 5. Emerald ellipse
    let opacity_ellipse = ALPHA_ELLIPSE as f32 / 255.0;
    writeln!(
        writer,
        r#"  <ellipse cx="{ELLIPSE_CENTER_X}" cy="{ELLIPSE_CENTER_Y}" rx="{ELLIPSE_RX}" ry="{ELLIPSE_RY}" fill="rgb({}, {}, {})" fill-opacity="{opacity_ellipse:.5}"/>"#,
        COLOR_EMERALD.r, COLOR_EMERALD.g, COLOR_EMERALD.b
    )?;

    writeln!(writer, "</svg>")?;

    println!("Saved SVG: {} ({}x{})", path, width, height);
    Ok(())
}

// -----------------------------------------------------------------------------
// Renderers
// -----------------------------------------------------------------------------

/// Renders the scene with `yonder-graphics`: subpixel antialiasing and alpha blending.
fn render_yonder(viewport: &mut Viewport<Rgb888>) -> Result<(), Box<dyn std::error::Error>> {
    viewport.clear(COLOR_WHITE.to_rgb888())?;

    // 1. Cyan circle
    Conic::circle(CYAN_CENTER_X, CYAN_CENTER_Y, CYAN_RADIUS)
        .into_styled(
            CompositingStyle::with_fill_alpha(COLOR_CYAN.to_rgb888(), ALPHA_CYAN)
                .with_antialiasing(true),
        )
        .draw(viewport)?;

    // 2. Magenta circle
    Conic::circle(MAGENTA_CENTER_X, MAGENTA_CENTER_Y, MAGENTA_RADIUS)
        .into_styled(
            CompositingStyle::with_fill_alpha(COLOR_MAGENTA.to_rgb888(), ALPHA_MAGENTA)
                .with_antialiasing(true),
        )
        .draw(viewport)?;

    // 3. Yellow triangle
    let p1 = Coord3::point(TRI_P1.0, TRI_P1.1);
    let p2 = Coord3::point(TRI_P2.0, TRI_P2.1);
    let p3 = Coord3::point(TRI_P3.0, TRI_P3.1);
    YonderTriangle::new(p1, p2, p3)
        .into_styled(CompositingStyle::with_fill_alpha(
            COLOR_YELLOW.to_rgb888(),
            ALPHA_TRIANGLE,
        ))
        .draw(viewport)?;

    // 4. Emerald ellipse
    Conic::ellipse(ELLIPSE_CENTER_X, ELLIPSE_CENTER_Y, ELLIPSE_RX, ELLIPSE_RY)
        .into_styled(
            CompositingStyle::with_fill_alpha(COLOR_EMERALD.to_rgb888(), ALPHA_ELLIPSE)
                .with_antialiasing(true),
        )
        .draw(viewport)?;

    Ok(())
}

/// Renders the scene with standard `embedded-graphics`: opaque fills, non-antialiased.
fn render_embedded_graphics(
    viewport: &mut Viewport<Rgb888>,
) -> Result<(), Box<dyn std::error::Error>> {
    viewport.clear(COLOR_WHITE.to_rgb888())?;

    // 1. Cyan circle
    EgCircle::with_center(
        Point::new(CYAN_CENTER_X, CYAN_CENTER_Y),
        (CYAN_RADIUS * 2) as u32,
    )
    .into_styled(PrimitiveStyle::with_fill(COLOR_CYAN.to_rgb888()))
    .draw(viewport)?;

    // 2. Magenta circle
    EgCircle::with_center(
        Point::new(MAGENTA_CENTER_X, MAGENTA_CENTER_Y),
        (MAGENTA_RADIUS * 2) as u32,
    )
    .into_styled(PrimitiveStyle::with_fill(COLOR_MAGENTA.to_rgb888()))
    .draw(viewport)?;

    // 3. Yellow triangle
    EgTriangle::new(
        Point::new(TRI_P1.0, TRI_P1.1),
        Point::new(TRI_P2.0, TRI_P2.1),
        Point::new(TRI_P3.0, TRI_P3.1),
    )
    .into_styled(PrimitiveStyle::with_fill(COLOR_YELLOW.to_rgb888()))
    .draw(viewport)?;

    // 4. Emerald ellipse
    EgEllipse::with_center(
        Point::new(ELLIPSE_CENTER_X, ELLIPSE_CENTER_Y),
        Size::new((ELLIPSE_RX * 2) as u32, (ELLIPSE_RY * 2) as u32),
    )
    .into_styled(PrimitiveStyle::with_fill(COLOR_EMERALD.to_rgb888()))
    .draw(viewport)?;

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bounds = Rect::new(0, 0, WIDTH, HEIGHT);

    // 1. Render Yonder (antialiased + translucent alpha compositing)
    let mut buffer_yonder = vec![0u32; PIXEL_COUNT];
    let mut viewport_yonder = Viewport::<Rgb888>::new(&mut buffer_yonder, bounds);
    render_yonder(&mut viewport_yonder)?;
    save_png(
        "transparent_shapes_yonder.png",
        viewport_yonder.buffer(),
        WIDTH,
        HEIGHT,
    )?;

    // 2. Render embedded-graphics (standard primitives: opaque, non-antialiased)
    let mut buffer_eg = vec![0u32; PIXEL_COUNT];
    let mut viewport_eg = Viewport::<Rgb888>::new(&mut buffer_eg, bounds);
    render_embedded_graphics(&mut viewport_eg)?;
    save_png(
        "transparent_shapes_embedded_graphics.png",
        viewport_eg.buffer(),
        WIDTH,
        HEIGHT,
    )?;

    // 3. Export SVG vector document
    save_svg("transparent_shapes.svg", WIDTH, HEIGHT)?;

    Ok(())
}
