//! Our antialiased circle next to Zingl's `plotCircleAA`, and the cells corner sampling
//! can't see. Investigation output, written by the gallery binary.

use std::collections::HashMap;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use yonder_graphics::prelude::*;

use crate::Framebuffer;

/// Coverage 0..=255 per pixel.
pub type Coverage = HashMap<(i32, i32), u8>;

/// Zingl's antialiased circle, ported line for line from `plotCircleAA` in `bresenham.c`
/// (Alois Zingl, MIT license, <https://github.com/zingl/Bresenham>). A 1 px outline of
/// radius `r` around the center of pixel `(xm, ym)`. Zingl's `i` is 255 − coverage; where
/// quadrants meet a pixel is set twice, and the larger coverage is kept.
pub fn zingl_circle_aa(xm: i32, ym: i32, r: i32) -> Coverage {
    let mut out = Coverage::new();
    let mut set_pixel_aa = |x: i32, y: i32, i: i32| {
        let coverage = (255 - i.clamp(0, 255)) as u8;
        let pixel = out.entry((x, y)).or_insert(0);
        *pixel = (*pixel).max(coverage);
    };
    let (mut x, mut y) = (-r, 0);
    let mut err = 2 - 2 * r;
    let r = 1 - err;
    loop {
        let i = 255 * (err - 2 * (x + y) - 2).abs() / r;
        set_pixel_aa(xm - x, ym + y, i);
        set_pixel_aa(xm - y, ym - x, i);
        set_pixel_aa(xm + x, ym - y, i);
        set_pixel_aa(xm + y, ym + x, i);
        let (e2, x2) = (err, x);
        if err + y > 0 {
            let i = 255 * (err - 2 * x - 1) / r;
            if i < 256 {
                set_pixel_aa(xm - x, ym + y + 1, i);
                set_pixel_aa(xm - y - 1, ym - x, i);
                set_pixel_aa(xm + x, ym - y - 1, i);
                set_pixel_aa(xm + y + 1, ym + x, i);
            }
            x += 1;
            err += x * 2 + 1;
        }
        if e2 + x2 <= 0 {
            let i = 255 * (2 * y + 3 - e2) / r;
            if i < 256 {
                set_pixel_aa(xm - x2 - 1, ym + y, i);
                set_pixel_aa(xm - y, ym - x2 - 1, i);
                set_pixel_aa(xm + x2 + 1, ym - y, i);
                set_pixel_aa(xm + y, ym + x2 + 1, i);
            }
            y += 1;
            err += y * 2 + 1;
        }
        if x >= 0 {
            break;
        }
    }
    out
}

/// Draws `shape` white on black through the public API: each pixel's value is its coverage.
fn render<T>(side: i16, shape: T) -> Coverage
where
    T: Drawable<Framebuffer<Rgb888>>,
{
    let mut frame = Framebuffer::new(side, side, Rgb888::new(0, 0, 0));
    shape.draw(&mut frame);
    let mut out = Coverage::new();
    for (index, pixel) in frame.pixels.iter().enumerate() {
        if pixel.r() > 0 {
            let (x, y) = (index as i32 % i32::from(side), index as i32 / i32::from(side));
            out.insert((x, y), pixel.r());
        }
    }
    out
}

/// Our 1 px stroke of radius `r` around the center of pixel `(xm, ym)`: Zingl's circle.
fn ours(side: i16, xm: i16, ym: i16, r: i16) -> Coverage {
    let style = Style::new().stroke(Stroke::new(1, Paint::solid(Rgb888::WHITE)));
    render(
        side,
        Circle::at_pixel_center(Point::new(xm, ym), r).with_style(style),
    )
}

/// Exact coverage of the ring between `inner` and `outer` around `center`, supersampled.
fn exact(side: i32, center: (f64, f64), inner: f64, outer: f64) -> Coverage {
    const SAMPLES: i32 = 32;
    let mut out = Coverage::new();
    for y in 0..side {
        for x in 0..side {
            let mut hits = 0;
            for j in 0..SAMPLES {
                for i in 0..SAMPLES {
                    let dx = f64::from(x) + (f64::from(i) + 0.5) / f64::from(SAMPLES) - center.0;
                    let dy = f64::from(y) + (f64::from(j) + 0.5) / f64::from(SAMPLES) - center.1;
                    let d = (dx * dx + dy * dy).sqrt();
                    hits += i32::from(d >= inner && d <= outer);
                }
            }
            let coverage = (f64::from(hits) * 255.0 / f64::from(SAMPLES * SAMPLES)).round() as u8;
            if coverage > 0 {
                out.insert((x, y), coverage);
            }
        }
    }
    out
}

/// An RGB image drawn at `zoom` pixels per screen pixel.
struct Canvas {
    width: usize,
    height: usize,
    zoom: usize,
    data: Vec<[u8; 3]>,
}

impl Canvas {
    fn new(width: usize, height: usize, zoom: usize) -> Self {
        Self {
            width: width * zoom,
            height: height * zoom,
            zoom,
            data: vec![[255, 255, 255]; width * zoom * height * zoom],
        }
    }

    fn put(&mut self, x: f64, y: f64, color: [u8; 3]) {
        let (x, y) = (x.floor(), y.floor());
        if x >= 0.0 && y >= 0.0 && (x as usize) < self.width && (y as usize) < self.height {
            self.data[y as usize * self.width + x as usize] = color;
        }
    }

    /// Fills screen pixel `(x, y)` of a panel whose top-left screen pixel is `origin`.
    fn pixel(&mut self, origin: (usize, usize), x: i32, y: i32, color: [u8; 3]) {
        let z = self.zoom;
        for j in 0..z {
            for i in 0..z {
                let grid = i == 0 || j == 0;
                let color = if grid { color.map(|c| c / 2 + 40) } else { color };
                let px = (origin.0 + x as usize) * z + i;
                let py = (origin.1 + y as usize) * z + j;
                self.data[py * self.width + px] = color;
            }
        }
    }

    /// A line in screen coordinates (fractional), in a panel at `origin`.
    fn line(&mut self, origin: (usize, usize), a: (f64, f64), b: (f64, f64), color: [u8; 3]) {
        let z = self.zoom as f64;
        let (ax, ay) = ((origin.0 as f64 + a.0) * z, (origin.1 as f64 + a.1) * z);
        let (bx, by) = ((origin.0 as f64 + b.0) * z, (origin.1 as f64 + b.1) * z);
        let steps = ((bx - ax).abs().max((by - ay).abs()) * 2.0).ceil().max(1.0) as usize;
        for step in 0..=steps {
            let t = step as f64 / steps as f64;
            let (x, y) = (ax + (bx - ax) * t, ay + (by - ay) * t);
            for (dx, dy) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
                self.put(x + dx, y + dy, color);
            }
        }
    }

    fn save(&self, path: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(path.parent().unwrap())?;
        let mut encoder = png::Encoder::new(
            BufWriter::new(File::create(path)?),
            self.width as u32,
            self.height as u32,
        );
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()?
            .write_image_data(self.data.concat().as_slice())?;
        Ok(())
    }
}

fn gray(coverage: u8) -> [u8; 3] {
    [255 - coverage; 3]
}

/// The ideal circle and its gradient `∇F = 2·(x − xm, y − ym)` at 16 points, drawn as
/// short arrows pointing outward.
fn overlay(canvas: &mut Canvas, origin: (usize, usize), center: (f64, f64), r: f64) {
    const RED: [u8; 3] = [230, 40, 40];
    const BLUE: [u8; 3] = [30, 110, 230];
    let point = |angle: f64, radius: f64| {
        (
            center.0 + radius * angle.cos(),
            center.1 + radius * angle.sin(),
        )
    };
    for step in 0..720 {
        let (a, b) = (
            f64::from(step) * std::f64::consts::TAU / 720.0,
            f64::from(step + 1) * std::f64::consts::TAU / 720.0,
        );
        canvas.line(origin, point(a, r), point(b, r), RED);
    }
    for step in 0..16 {
        let angle = (f64::from(step) + 0.5) * std::f64::consts::TAU / 16.0;
        let (start, end) = (point(angle, r), point(angle, r + 1.2));
        canvas.line(origin, start, end, BLUE);
        for side in [-0.5, 0.5] {
            let head = (
                end.0 - 0.35 * (angle + side).cos(),
                end.1 - 0.35 * (angle + side).sin(),
            );
            canvas.line(origin, end, head, BLUE);
        }
    }
}

/// Mean and largest |a − b| over the pixels either one covers.
fn difference(a: &Coverage, b: &Coverage) -> (f64, u8) {
    let mut keys: Vec<_> = a.keys().chain(b.keys()).collect();
    keys.sort();
    keys.dedup();
    let differences: Vec<u8> = keys
        .iter()
        .map(|key| {
            let (x, y) = (a.get(key).copied(), b.get(key).copied());
            x.unwrap_or(0).abs_diff(y.unwrap_or(0))
        })
        .collect();
    let mean = differences.iter().map(|&d| f64::from(d)).sum::<f64>() / differences.len() as f64;
    (mean, differences.iter().copied().max().unwrap_or(0))
}

/// Ours, Zingl's, exact, and ours − Zingl (red: ours higher, blue: Zingl's higher, ×4),
/// one row per radius.
pub fn compare(path: &Path) {
    const ZOOM: usize = 16;
    let radii = [3i16, 6, 12];
    let sides: Vec<usize> = radii.iter().map(|&r| 2 * r as usize + 5).collect();
    let width = 4 * sides.iter().max().unwrap() + 3;
    let height = sides.iter().sum::<usize>() + radii.len() - 1;
    let mut canvas = Canvas::new(width, height, ZOOM);
    let mut top = 0;
    println!("radius | mean/max |ours − exact| | mean/max |Zingl − exact| | mean/max |ours − Zingl|");
    for (&r, &side) in radii.iter().zip(&sides) {
        let c = (side / 2) as i16;
        let ours = ours(side as i16, c, c, r);
        let zingl = zingl_circle_aa(c.into(), c.into(), r.into());
        let center = (f64::from(c) + 0.5, f64::from(c) + 0.5);
        let exact = exact(side as i32, center, f64::from(r) - 0.5, f64::from(r) + 0.5);
        let (ours_mean, ours_max) = difference(&ours, &exact);
        let (zingl_mean, zingl_max) = difference(&zingl, &exact);
        let (both_mean, both_max) = difference(&ours, &zingl);
        println!(
            "{r:>6} | {ours_mean:>6.2} / {ours_max:>3} | {zingl_mean:>6.2} / {zingl_max:>3} | {both_mean:>6.2} / {both_max:>3}"
        );
        let ink = |coverage: &Coverage| coverage.values().map(|&c| f64::from(c) / 255.0).sum::<f64>();
        println!(
            "       ink in px²: ours {:.2}, Zingl {:.2}, exact {:.2} (2πr = {:.2})",
            ink(&ours),
            ink(&zingl),
            ink(&exact),
            std::f64::consts::TAU * f64::from(r)
        );
        let panels: [&Coverage; 3] = [&ours, &zingl, &exact];
        for (column, coverage) in panels.iter().enumerate() {
            let origin = (column * (sides.iter().max().unwrap() + 1), top);
            for y in 0..side as i32 {
                for x in 0..side as i32 {
                    let value = coverage.get(&(x, y)).copied().unwrap_or(0);
                    canvas.pixel(origin, x, y, gray(value));
                }
            }
            overlay(&mut canvas, origin, center, f64::from(r));
        }
        let origin = (3 * (sides.iter().max().unwrap() + 1), top);
        for y in 0..side as i32 {
            for x in 0..side as i32 {
                let (a, b) = (
                    i32::from(ours.get(&(x, y)).copied().unwrap_or(0)),
                    i32::from(zingl.get(&(x, y)).copied().unwrap_or(0)),
                );
                let d = ((a - b) * 4).clamp(-255, 255);
                let color = if d >= 0 {
                    [255, (255 - d) as u8, (255 - d) as u8]
                } else {
                    [(255 + d) as u8, (255 + d) as u8, 255]
                };
                canvas.pixel(origin, x, y, color);
            }
        }
        top += side + 1;
    }
    canvas.save(path).expect("write png");
}

/// Small filled circles with the cells corner sampling can't see outlined in red: all four
/// corners outside, yet the circle reaches into the cell. One row per center kind.
pub fn invisible_cells(path: &Path) {
    const ZOOM: usize = 14;
    const SIDE: usize = 25;
    let sizes = 1..=10i16;
    let kinds = ["lattice point, radius r", "pixel center, radius r", "pixel center, radius r − ½"];
    let mut canvas = Canvas::new(sizes.len() * (SIDE + 1), kinds.len() * (SIDE + 1), ZOOM);
    println!("cells with every corner outside but some coverage (exact, then ours):");
    for (row, kind) in kinds.iter().enumerate() {
        let mut report = String::new();
        for (column, r) in sizes.clone().enumerate() {
            let c = (SIDE / 2) as i16;
            let white = Style::new().fill(Paint::solid(Rgb888::WHITE));
            let (circle, center, radius) = match row {
                0 => (
                    Circle::in_box(BoundingBox::new(
                        Point::new(c - r, c - r),
                        Point::new(c + r, c + r),
                    )),
                    (f64::from(c), f64::from(c)),
                    f64::from(r),
                ),
                1 => (
                    Circle::at_pixel_center(Point::new(c, c), r),
                    (f64::from(c) + 0.5, f64::from(c) + 0.5),
                    f64::from(r),
                ),
                _ => (
                    Circle::in_box(BoundingBox::new(
                        Point::new(c - r + 1, c - r + 1),
                        Point::new(c + r, c + r),
                    )),
                    (f64::from(c) + 0.5, f64::from(c) + 0.5),
                    f64::from(r) - 0.5,
                ),
            };
            let ours = render(SIDE as i16, circle.with_style(white));
            let exact = exact(SIDE as i32, center, -1.0, radius);
            let origin = (column * (SIDE + 1), row * (SIDE + 1));
            let mut invisible = Vec::new();
            for y in 0..SIDE as i32 {
                for x in 0..SIDE as i32 {
                    let value = ours.get(&(x, y)).copied().unwrap_or(0);
                    canvas.pixel(origin, x, y, gray(value));
                    let corners = [(0, 0), (1, 0), (0, 1), (1, 1)].map(|(i, j)| {
                        let (dx, dy) = (f64::from(x + i) - center.0, f64::from(y + j) - center.1);
                        (dx * dx + dy * dy).sqrt()
                    });
                    let nearest = (
                        center.0.clamp(f64::from(x), f64::from(x + 1)) - center.0,
                        center.1.clamp(f64::from(y), f64::from(y + 1)) - center.1,
                    );
                    let reaches = (nearest.0 * nearest.0 + nearest.1 * nearest.1).sqrt() < radius;
                    if corners.iter().all(|&d| d > radius) && reaches {
                        invisible.push((exact.get(&(x, y)).copied().unwrap_or(0), value));
                        let (x0, y0) = (f64::from(x), f64::from(y));
                        for (a, b) in [
                            ((x0, y0), (x0 + 1.0, y0)),
                            ((x0 + 1.0, y0), (x0 + 1.0, y0 + 1.0)),
                            ((x0 + 1.0, y0 + 1.0), (x0, y0 + 1.0)),
                            ((x0, y0 + 1.0), (x0, y0)),
                        ] {
                            canvas.line(origin, a, b, [230, 40, 40]);
                        }
                    }
                }
            }
            if !invisible.is_empty() {
                report += &format!(" r={r}: {invisible:?}");
            }
        }
        println!("  {kind}:{}", if report.is_empty() { " none" } else { &report });
    }
    canvas.save(path).expect("write png");
}
