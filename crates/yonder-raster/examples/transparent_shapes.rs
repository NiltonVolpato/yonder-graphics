use embedded_graphics_core::draw_target::DrawTarget;
use embedded_graphics_core::pixelcolor::Rgb888;
use yonder_core::Coord3;
use yonder_damage::Rect;
use yonder_raster::{CompositingStyle, Conic, Triangle, Viewport};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    const WIDTH: u32 = 1024;
    const HEIGHT: u32 = 1024;
    const PIXEL_COUNT: usize = (WIDTH * HEIGHT) as usize;

    // Allocate an in-memory 32-bit RGB888 pixel buffer
    let mut buffer = vec![0u32; PIXEL_COUNT];
    let bounds = Rect::new(0, 0, WIDTH, HEIGHT);
    let mut viewport = Viewport::<Rgb888>::new(&mut buffer, bounds);

    // 1. Fill background with white paper
    let white = Rgb888::new(255, 255, 255);
    viewport.clear(white)?;

    // 2. Draw transparent, antialiased shapes using styled primitives:

    // Semi-transparent Cyan circle
    let cyan = Rgb888::new(0, 205, 255);
    Conic::circle(370, 440, 200)
        .into_styled(CompositingStyle::with_fill_alpha(cyan, 160).with_antialiasing(true))
        .draw(&mut viewport)?;

    // Semi-transparent Magenta circle overlapping the first
    let magenta = Rgb888::new(255, 30, 160);
    Conic::circle(654, 440, 200)
        .into_styled(CompositingStyle::with_fill_alpha(magenta, 160).with_antialiasing(true))
        .draw(&mut viewport)?;

    // Semi-transparent Yellow triangle overlapping both circles
    let yellow = Rgb888::new(255, 235, 0);
    let p1 = Coord3::point(512, 170);
    let p2 = Coord3::point(220, 770);
    let p3 = Coord3::point(804, 770);
    Triangle::new(p1, p2, p3)
        .into_styled(CompositingStyle::with_fill_alpha(yellow, 180))
        .draw(&mut viewport)?;

    // Semi-transparent Emerald ellipse intersecting the bottom
    let emerald = Rgb888::new(0, 220, 80);
    Conic::ellipse(512, 690, 280, 140)
        .into_styled(CompositingStyle::with_fill_alpha(emerald, 140).with_antialiasing(true))
        .draw(&mut viewport)?;

    // 3. Serialize 32-bit XRGB8888 buffer to 24-bit RGB bytes for PNG encoding
    let mut rgb888_bytes = Vec::with_capacity(PIXEL_COUNT * 3);
    for &pixel in viewport.buffer() {
        rgb888_bytes.extend_from_slice(&[
            ((pixel >> 16) & 0xFF) as u8,
            ((pixel >> 8) & 0xFF) as u8,
            (pixel & 0xFF) as u8,
        ]);
    }

    // 4. Save to transparent_shapes.png
    let output_path = "transparent_shapes.png";
    let file = std::fs::File::create(output_path)?;
    let buf_writer = &mut std::io::BufWriter::new(file);

    let mut encoder = png::Encoder::new(buf_writer, WIDTH, HEIGHT);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&rgb888_bytes)?;

    println!(
        "Rendered transparent shapes on white paper to {} ({}x{})",
        output_path, WIDTH, HEIGHT
    );

    Ok(())
}
