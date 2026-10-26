//! Renders every scene to `target/gallery/*.png`.

use std::path::PathBuf;

use yonder_gallery::{BACKGROUND, Framebuffer, basic, buttons, circles, save_png, strokes};
use yonder_graphics::color::{Rgb565, Rgb888};

fn render(width: i16, height: i16, zoom: usize, name: &str, scene: fn(&mut Framebuffer<Rgb888>)) {
    let mut frame = Framebuffer::new(width, height, BACKGROUND);
    scene(&mut frame);
    write(name, width, &frame.pixels, zoom);
}

fn write(name: &str, width: i16, pixels: &[Rgb888], zoom: usize) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../target/gallery")
        .join(name);
    save_png(&path, width, pixels, zoom).expect("write png");
    println!("{}", path.canonicalize().unwrap().display());
}

fn main() {
    let mut frame = Framebuffer::new(120, 70, Rgb565::from(BACKGROUND));
    basic(&mut frame);
    let pixels: Vec<Rgb888> = frame.pixels.into_iter().map(Rgb888::from).collect();
    write("basic.png", 120, &pixels, 4);

    render(240, 120, 4, "circles.png", circles);
    render(60, 40, 14, "circles-zoom.png", circles);
    render(240, 56, 4, "strokes.png", strokes);
    render(56, 44, 14, "strokes-zoom.png", strokes);
    render(190, 44, 4, "buttons.png", buttons);
}
