use std::mem::size_of;

use yonder_gallery::{
    BACKGROUND, Band, Button, Dark, Framebuffer, WithTheme, basic, buttons, circles,
    render_in_bands, strokes,
};
use yonder_graphics::damage::DamageGrid;
use yonder_graphics::prelude::*;

fn same(a: &[Rgb888], b: &[Rgb888], width: i16) -> Result<(), String> {
    match a.iter().zip(b).position(|(p, q)| p != q) {
        None if a.len() == b.len() => Ok(()),
        None => Err(format!("lengths {} and {}", a.len(), b.len())),
        Some(i) => Err(format!(
            "first difference at ({}, {})",
            i % width as usize,
            i / width as usize
        )),
    }
}

fn check_bands(
    name: &str,
    width: i16,
    height: i16,
    full: fn(&mut Framebuffer<Rgb888>),
    band: fn(&mut Band<Rgb888>),
) {
    let mut frame = Framebuffer::new(width, height, BACKGROUND);
    full(&mut frame);
    for band_height in [1, 7, 16, 200] {
        let banded = render_in_bands(width, height, band_height, BACKGROUND, band);
        if let Err(e) = same(&frame.pixels, &banded, width) {
            panic!("{name} in bands of {band_height}: {e}");
        }
    }
}

#[test]
fn bands_match_full_frame() {
    check_bands("circles", 240, 120, circles, circles);
    check_bands("strokes", 240, 56, strokes, strokes);
    check_bands("buttons", 190, 44, buttons, buttons);
}

#[test]
fn basic_example_blends_on_rgb565() {
    let background = Rgb565::from(BACKGROUND);
    let mut frame = Framebuffer::new(120, 70, background);
    basic(&mut frame);
    let at = |x: usize, y: usize| frame.pixels[y * 120 + x];
    let mut fill = [background];
    Rgb565::blend_slice(&mut fill, Rgb565::RED.with_alpha(166));
    assert!(at(60, 35) == fill[0], "fill");
    assert!(at(5, 5) == background, "outside");
}

#[test]
fn themed_button_stores_no_style() {
    assert_eq!(size_of::<WithTheme<Button, Dark>>(), size_of::<Button>());
    let draw = |pressed| {
        let mut frame = Framebuffer::new(20, 20, BACKGROUND);
        let bounds = Rectangle::new(Point::new(2, 2), Point::new(18, 18));
        WithTheme::<_, Dark>::new(Button { bounds, pressed }).draw(&mut frame);
        frame.pixels[10 * 20 + 10]
    };
    assert!(draw(false) != draw(true));
}

#[test]
fn mixed_drawables_behind_dyn() {
    type Frame = Framebuffer<Rgb888>;
    let rect = Rectangle::new(Point::new(0, 0), Point::new(4, 4))
        .with_style(Style::new().fill(Paint::solid(Rgb888::RED)));
    let circle = Circle::new(PointT2::new(8, 2, 1), 2)
        .with_style(Style::new().fill(Paint::solid(Rgb888::GREEN)));
    let pixel = Pixel {
        position: Point::new(11, 0),
        color: Rgb888::BLUE.with_alpha(255),
    };
    let button = WithTheme::<_, Dark>::new(Button {
        bounds: Rectangle::new(Point::new(0, 6), Point::new(12, 10)),
        pressed: false,
    });
    let scene: [&dyn Drawable<Frame>; 4] = [&rect, &circle, &pixel, &button];
    let mut frame = Frame::new(12, 10, BACKGROUND);
    for item in scene {
        item.draw(&mut frame);
    }
    assert!(frame.pixels[0] == Rgb888::RED && frame.pixels[11] == Rgb888::BLUE);
}

type Grid = DamageGrid<Rgb888, 13, 13>;

/// A background card, then a circle over it.
fn frame(card: Paint<Rgb888>, circle: Paint<Rgb888>, center_x: i32) -> impl Fn(&mut Grid) {
    move |grid| {
        Rectangle::new(Point::new(0, 0), Point::new(208, 208))
            .with_style(Style::new().fill(card))
            .draw(grid);
        Circle::new(PointT2::new(center_x, 100, 1), 50)
            .with_style(Style::new().fill(circle))
            .draw(grid);
    }
}

/// The tiles damaged going from one frame to the next.
fn damaged(first: impl Fn(&mut Grid), second: impl Fn(&mut Grid)) -> Vec<(usize, usize)> {
    let mut grid = Grid::new();
    first(&mut grid);
    grid.next_frame();
    second(&mut grid);
    let tiles = (0..13).flat_map(|row| (0..13).map(move |column| (column, row)));
    tiles
        .filter(|&(column, row)| grid.is_damaged(column, row))
        .collect()
}

#[test]
fn damage_grid_hashes_color_and_position() {
    let white = Paint::solid(Rgb888::WHITE);
    let base = || frame(Paint::solid(BACKGROUND), white, 100);
    assert!(
        damaged(base(), base()).is_empty(),
        "same frame, same hashes"
    );

    // A color change damages every tile the circle touches, and nothing else.
    let recolored = damaged(
        base(),
        frame(Paint::solid(BACKGROUND), Paint::solid(Rgb888::RED), 100),
    );
    let extent = Circle::new(PointT2::new(100, 100, 1), 50).bounds();
    let touched: Vec<(usize, usize)> = (0..13)
        .flat_map(|row| (0..13).map(move |column| (column, row)))
        .filter(|&(column, row)| {
            let (x, y) = (column as i16 * 16, row as i16 * 16);
            let tile = BoundingBox::new(Point::new(x, y), Point::new(x + 16, y + 16));
            tile.intersection(&extent).is_some()
        })
        .collect();
    assert_eq!(recolored, touched);

    // Moving it damages only the tiles its edge crosses, not those inside the inscribed square.
    let moved = damaged(base(), frame(Paint::solid(BACKGROUND), white, 101));
    assert!(
        !moved.is_empty() && moved.len() < touched.len(),
        "{} of {}",
        moved.len(),
        touched.len()
    );

    // An opaque circle hides the card below it inside its interior.
    let card_changed = damaged(base(), frame(Paint::solid(Rgb888::BLUE), white, 100));
    assert!(card_changed.len() < 13 * 13);

    // A translucent one doesn't.
    let see_through = |card| frame(Paint::solid(card), white.alpha(200), 100);
    assert_eq!(
        damaged(see_through(BACKGROUND), see_through(Rgb888::BLUE)).len(),
        13 * 13
    );
}
