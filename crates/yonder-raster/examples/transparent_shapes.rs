use embedded_graphics_core::pixelcolor::{IntoStorage, Rgb565};
use yonder_core::{ConicMatrix, Coord3};
use yonder_damage::Rect;
use yonder_raster::{rasterize_conic, rasterize_triangle, ViewportScratchpad};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    const WIDTH: u32 = 1024;
    const HEIGHT: u32 = 1024;
    const PIXEL_COUNT: usize = (WIDTH * HEIGHT) as usize;

    // Allocate an in-memory RGB565 pixel buffer
    let mut buffer = vec![0u16; PIXEL_COUNT];
    let bounds = Rect::new(0, 0, WIDTH, HEIGHT);
    let mut scratch = ViewportScratchpad::new(&mut buffer, bounds);

    // 1. Fill background with white paper
    let white = Rgb565::new(31, 63, 31);
    scratch.buffer_mut().fill(white.into_storage());

    // 2. Draw transparent shapes on the white background:

    // Semi-transparent Cyan circle
    let cyan = Rgb565::new(0, 50, 31);
    let circle_cyan = ConicMatrix::circle(370, 440, 200);
    let bounds_cyan = Rect::new(370 - 200, 440 - 200, 401, 401);
    rasterize_conic(&mut scratch, &circle_cyan, bounds_cyan, cyan, Some(160))?;

    // Semi-transparent Magenta circle overlapping the first
    let magenta = Rgb565::new(31, 5, 20);
    let circle_magenta = ConicMatrix::circle(654, 440, 200);
    let bounds_magenta = Rect::new(654 - 200, 440 - 200, 401, 401);
    rasterize_conic(
        &mut scratch,
        &circle_magenta,
        bounds_magenta,
        magenta,
        Some(160),
    )?;

    // Semi-transparent Yellow/Amber triangle overlapping both circles
    let yellow = Rgb565::new(31, 58, 0);
    let p1 = Coord3::point(512, 170);
    let p2 = Coord3::point(220, 770);
    let p3 = Coord3::point(804, 770);
    rasterize_triangle(&mut scratch, p1, p2, p3, yellow, Some(180))?;

    // Semi-transparent Emerald ellipse intersecting the bottom
    let emerald = Rgb565::new(0, 55, 10);
    let ellipse = ConicMatrix::ellipse(512, 690, 280, 140);
    let bounds_ellipse = Rect::new(512 - 280, 690 - 140, 561, 281);
    rasterize_conic(&mut scratch, &ellipse, bounds_ellipse, emerald, Some(140))?;

    // 3. Convert 16-bit RGB565 to 24-bit RGB888 for PNG encoding
    let mut rgb888 = Vec::with_capacity(PIXEL_COUNT * 3);
    for &pixel in scratch.buffer() {
        // Expand 5-6-5 bits to 8-8-8
        let r = (((pixel >> 11) & 0x1F) as u32 * 255 / 31) as u8;
        let g = (((pixel >> 5) & 0x3F) as u32 * 255 / 63) as u8;
        let b = ((pixel & 0x1F) as u32 * 255 / 31) as u8;
        rgb888.push(r);
        rgb888.push(g);
        rgb888.push(b);
    }

    // 4. Save to transparent_shapes.png
    let output_path = "transparent_shapes.png";
    let file = std::fs::File::create(output_path)?;
    let buf_writer = &mut std::io::BufWriter::new(file);

    let mut encoder = png::Encoder::new(buf_writer, WIDTH, HEIGHT);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&rgb888)?;

    println!(
        "Rendered transparent shapes on white paper to {} ({}x{})",
        output_path, WIDTH, HEIGHT
    );

    Ok(())
}
