//! Anti-aliased filled circle in a D x D pixel box. The circle touches all four sides of the box.
//!
//! Odd D: the center is the center of a pixel. Even D: the center is a pixel corner.
//! Pixel (c, y) is the unit square with corners TL (c, y), TR (c+1, y), BR (c+1, y+1), BL (c, y+1).
//! Columns and rows run from -(d >> 1) to -(d >> 1) + d - 1, so the center is at 0 and y grows downward.
//!
//! The walk follows the left half of the boundary from the top pixel next to the vertical axis down to the
//! bottom one. It visits only pixels the circle cuts. It keeps one pixel and one integer, F at the next corner
//! to test, and it does one sign test per pixel. All updates are integer shifts and adds.
//! The right half is the mirror image, and the pixels between the two sides are filled with spans.
//!
//! Run the tests with:  rustc --edition 2021 --test circle_aa.rs -o circle_aa_test && ./circle_aa_test
//! Run the demo with:   rustc --edition 2021 -O circle_aa.rs -o circle_aa && ./circle_aa

use std::f64::consts::PI;

/// How the walk leaves a pixel.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Exit {
    Left,
    Bottom,
    Right,
    /// Through a corner that lies exactly on the circle, to the pixel on the other side of that corner.
    Diagonal,
}

/// A crossing: the edge of a pixel where the circle crosses it, and F at both ends of that edge.
/// `side` counts clockwise: 0 = top (TL to TR), 1 = right (TR to BR), 2 = bottom (BR to BL), 3 = left (BL to TL).
/// `f0` is 4F at the start corner of the edge and `f1` at its end corner. Exactly one of them is negative (inside).
/// A corner exactly on the circle has F = 0 and counts as outside.
/// The crossing is at the fraction f0 / (f0 - f1) of the way along the edge, if F is taken as linear along the edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Crossing {
    pub side: u8,
    pub f0: i32,
    pub f1: i32,
}

/// The same crossing seen from the pixel on the other side of the edge.
fn across(x: Crossing) -> Crossing {
    Crossing { side: (x.side + 2) % 4, f0: x.f1, f1: x.f0 }
}

/// One pixel of the walk.
#[derive(Clone, Copy, Debug)]
pub struct Step {
    pub c: i32,
    pub y: i32,
    /// Where the circle enters this pixel. This is the single crossing the walk carries from one pixel to the next.
    pub entry: Crossing,
    /// Where the circle leaves this pixel.
    pub exit: Crossing,
    /// How the walk leaves this pixel, or `None` for the last one.
    pub kind: Option<Exit>,
}

/// Walks the left half of the boundary. Calls `visit` once per pixel, in order.
///
/// With doubled coordinates a = 2i - o, b = 2j - o of a corner (i, j) relative to the center, where o = 1 for odd D
/// and 0 for even D, 4F = a^2 + b^2 - D^2. A step of a by +2 changes 4F by 4(a+1), and a step by -2 changes it by -4(a-1).
/// The same holds for b. `f` is 4F at the corner to test next. Every other F value is one add away from it.
/// The only multiplications are the setup, and the first and last pixel. D <= 20000 keeps `f` inside an i32.
pub fn trace_left(d: i32, mut visit: impl FnMut(Step)) {
    assert!((1..=20_000).contains(&d), "d must be in 1..=20000");
    let o = d & 1;
    let h = d >> 1;
    let (c_end, y_end) = (o - 1, d - 1 - h); // the last pixel: bottom row, next to the vertical axis
    let (mut c, mut y) = (o - 1, -h); //          the first pixel: top row, next to the vertical axis
    let (mut a, mut b) = (o - 2, 2 * (y + 1) - o); // the corner to test: bottom left of the first pixel
    let mut f = a * a + b * b - d * d;
    let none = Crossing { side: 0, f0: 0, f1: 0 };
    let mut entry = none; // the crossing carried from the previous pixel

    for step in 0..16 * d + 64 {
        if c == c_end && y == y_end {
            // The last pixel is the mirror image of the first. Its exit is the mirror image of its entry.
            let exit = if d == 1 {
                none
            } else if o == 1 {
                across(entry) // the entry is on the left edge, so the exit is on the right edge with the same values
            } else {
                // The circle leaves through the corner (0, y_end + 1), which is on the circle.
                Crossing { side: 1, f0: 4 * y_end * y_end - d * d, f1: 0 }
            };
            visit(Step { c, y, entry, exit, kind: None });
            return;
        }
        let (pc, py) = (c, y);
        let (kind, exit, next_entry) = if y < 0 {
            // Upper half: the arc runs down and left. Test the bottom left corner.
            if f < 0 {
                let x = Crossing { side: 3, f0: f, f1: f - ((b - 1) << 2) };
                f -= (a - 1) << 2;
                a -= 2;
                c -= 1;
                (Exit::Left, x, across(x))
            } else if f == 0 && b < 0 {
                // The corner is on the circle: go through it. F is 0 here, so only the two steps remain.
                let x = Crossing { side: 2, f0: (a + 1) << 2, f1: 0 };
                let n = Crossing { side: 1, f0: 0, f1: (b + 1) << 2 };
                f = -((a - 1) << 2);
                a -= 2;
                f += (b + 1) << 2;
                b += 2;
                c -= 1;
                y += 1;
                (Exit::Diagonal, x, n)
            } else {
                let x = Crossing { side: 2, f0: f + ((a + 1) << 2), f1: f };
                f += (b + 1) << 2;
                b += 2;
                if y + 1 == 0 {
                    // The next row is row 0. From there on test the bottom right corner.
                    f += (a + 1) << 2;
                    a += 2;
                }
                y += 1;
                (Exit::Bottom, x, across(x))
            }
        } else if f < 0 {
            // Lower half: the arc runs down and right. Test the bottom right corner.
            let x = Crossing { side: 2, f0: f, f1: f - ((a - 1) << 2) };
            f += (b + 1) << 2;
            b += 2;
            y += 1;
            (Exit::Bottom, x, across(x))
        } else if f == 0 {
            let x = Crossing { side: 1, f0: -((b - 1) << 2), f1: 0 };
            let n = Crossing { side: 0, f0: 0, f1: (a + 1) << 2 };
            f = (a + 1) << 2;
            a += 2;
            f += (b + 1) << 2;
            b += 2;
            c += 1;
            y += 1;
            (Exit::Diagonal, x, n)
        } else {
            let x = Crossing { side: 1, f0: f - ((b - 1) << 2), f1: f };
            f += (a + 1) << 2;
            a += 2;
            c += 1;
            (Exit::Right, x, across(x))
        };
        if step == 0 {
            // The first pixel is the mirror image of the last. Its entry is the mirror image of its exit.
            entry = if o == 1 {
                debug_assert_eq!(kind, Exit::Left);
                across(exit)
            } else {
                // The circle enters through the corner (0, -h), which is on the circle.
                Crossing { side: 1, f0: 0, f1: 4 * (h - 1) * (h - 1) - d * d }
            };
        }
        visit(Step { c: pc, y: py, entry, exit, kind: Some(kind) });
        entry = next_entry;
    }
    unreachable!("the walk did not reach its last pixel");
}

// ---- coverage ----
// This part uses floating point and is the "cheat": it computes the exact area of the disk inside a pixel.

fn antiderivative(r: f64, t: f64) -> f64 {
    0.5 * (t * (r * r - t * t).max(0.0).sqrt() + r * r * (t / r).clamp(-1.0, 1.0).asin())
}

/// Area of the disk of radius `r` centered at the origin inside [u0, u1] x [v0, v1], with 0 <= u0 < u1 and 0 <= v0 < v1.
fn rect_area(r: f64, u0: f64, u1: f64, v0: f64, v1: f64) -> f64 {
    let xa = (r * r - v0 * v0).max(0.0).sqrt().clamp(u0, u1); // where the arc meets the top edge, clamped to the pixel
    let xb = (r * r - v1 * v1).max(0.0).sqrt().clamp(u0, u1); // where it meets the bottom edge
    (v1 - v0) * (xb - u0) + antiderivative(r, xa) - antiderivative(r, xb) - v0 * (xa - xb)
}

/// The interval of pixel index `c` relative to the center, folded onto the positive side, and a multiplier.
fn fold(o: i32, c: i32) -> (f64, f64, f64) {
    let x0 = if o == 1 { c as f64 - 0.5 } else { c as f64 };
    let x1 = x0 + 1.0;
    if x0 < 0.0 && x1 > 0.0 {
        (0.0, x1, 2.0) // straddles the axis: odd D, c = 0
    } else if x1 <= 0.0 {
        (-x1, -x0, 1.0)
    } else {
        (x0, x1, 1.0)
    }
}

/// Exact coverage of pixel (c, y) by the circle.
pub fn coverage(d: i32, c: i32, y: i32) -> f64 {
    let o = d & 1;
    let (u0, u1, mu) = fold(o, c);
    let (v0, v1, mv) = fold(o, y);
    (mu * mv * rect_area(d as f64 / 2.0, u0, u1, v0, v1)).clamp(0.0, 1.0)
}

/// Coverage of a pixel the walk enters and leaves through opposite corners that are both on the circle.
/// The chord is the pixel diagonal, so the coverage is half a pixel plus the circular segment beyond the chord.
/// It is not exactly 0.5: the excess is about 0.236 / R.
pub fn double_diagonal_coverage(d: i32) -> f64 {
    let r = d as f64 / 2.0;
    let theta = 2.0 * (std::f64::consts::SQRT_2 / (2.0 * r)).asin();
    0.5 + 0.5 * r * r * (theta - theta.sin())
}

/// Corner k of the unit pixel: 0 = TL, 1 = TR, 2 = BR, 3 = BL. x grows to the right and y downward.
fn corner_xy(k: u8) -> (f64, f64) {
    match k % 4 {
        0 => (0.0, 0.0),
        1 => (1.0, 0.0),
        2 => (1.0, 1.0),
        _ => (0.0, 1.0),
    }
}

/// The point on the pixel edge where F, taken as linear along the edge, is 0.
fn crossing_point(x: Crossing) -> (f64, f64) {
    let (f0, f1) = (x.f0 as f64, x.f1 as f64);
    let p = f0 / (f0 - f1);
    let ((x0, y0), (x1, y1)) = (corner_xy(x.side), corner_xy(x.side + 1));
    (x0 + p * (x1 - x0), y0 + p * (y1 - y0))
}

/// Coverage of a pixel from its two crossings alone, with the circle taken as the straight chord between them.
/// The covered part is the polygon made of the entry point, the corners that are inside, and the exit point.
pub fn chord_coverage(entry: Crossing, exit: Crossing) -> f64 {
    debug_assert!(entry.side != exit.side);
    let mut pts = [(0.0f64, 0.0f64); 6];
    pts[0] = crossing_point(entry);
    let mut n = 1;
    let mut e = entry.side;
    if entry.f1 < 0 {
        // The corner at the end of the entry edge is inside. Go clockwise.
        loop {
            e = (e + 1) % 4; // the next corner is corner e, and the edge that starts there is edge e
            pts[n] = corner_xy(e);
            n += 1;
            if e == exit.side {
                break;
            }
        }
    } else {
        // The corner at the start of the entry edge is inside. Go counterclockwise.
        loop {
            pts[n] = corner_xy(e);
            n += 1;
            e = (e + 3) % 4; // the edge that ends at that corner
            if e == exit.side {
                break;
            }
        }
    }
    pts[n] = crossing_point(exit);
    n += 1;
    let mut twice_area = 0.0;
    for i in 0..n {
        let ((x0, y0), (x1, y1)) = (pts[i], pts[(i + 1) % n]);
        twice_area += x0 * y1 - x1 * y0;
    }
    (twice_area / 2.0).abs()
}

// ---- drawing ----

fn span(o: i32, y: i32, run_max: i32, put: &mut impl FnMut(i32, i32, f64)) {
    for c in run_max + 1..=-run_max - 2 + o {
        put(c, y, 1.0);
    }
}

/// Calls `put(c, y, v)` once for every pixel with nonzero coverage, in no particular order. The boundary pixels
/// get `coverage(&step)`, mirrored to the right half, and the pixels between the two sides of a row get 1.0.
fn draw(d: i32, mut coverage_of: impl FnMut(&Step) -> f64, mut put: impl FnMut(i32, i32, f64)) {
    let o = d & 1;
    let (mut row, mut run_max) = (i32::MIN, i32::MIN); // the walk visits each row in one run of pixels
    trace_left(d, |s| {
        if s.y != row {
            if row != i32::MIN {
                span(o, row, run_max, &mut put);
            }
            row = s.y;
            run_max = s.c;
        } else {
            run_max = run_max.max(s.c);
        }
        let v = coverage_of(&s);
        put(s.c, s.y, v);
        let mirror = -s.c - 1 + o;
        if mirror != s.c {
            put(mirror, s.y, v);
        }
    });
    if row != i32::MIN {
        span(o, row, run_max, &mut put);
    }
}

/// Coverage from the two crossings of each pixel, as straight chords. No floating point beyond one division
/// per crossing and the area of a small polygon.
pub fn circle_pixels_linear(d: i32, put: impl FnMut(i32, i32, f64)) {
    if d == 1 {
        // One pixel, no corner inside, no crossing on any edge.
        let mut put = put;
        put(0, 0, PI / 4.0);
        return;
    }
    draw(d, |s| chord_coverage(s.entry, s.exit), put);
}

/// Exact coverage of every boundary pixel, from its position alone. Does not use the crossings.
pub fn circle_pixels_exact(d: i32, put: impl FnMut(i32, i32, f64)) {
    let mut prev_diagonal = false;
    draw(
        d,
        |s| {
            let v = if prev_diagonal && s.kind == Some(Exit::Diagonal) {
                double_diagonal_coverage(d)
            } else {
                coverage(d, s.c, s.y)
            };
            prev_diagonal = s.kind == Some(Exit::Diagonal);
            v
        },
        put,
    );
}

fn image(d: i32, pixels: impl FnOnce(i32, &mut dyn FnMut(i32, i32, f64))) -> Vec<f64> {
    let h = d >> 1;
    let mut img = vec![0.0; (d * d) as usize];
    pixels(d, &mut |c, y, v| img[((y + h) * d + (c + h)) as usize] = v);
    img
}

/// Row-major images of size d * d.
pub fn render_linear(d: i32) -> Vec<f64> {
    image(d, |d, put| circle_pixels_linear(d, put))
}
pub fn render_exact(d: i32) -> Vec<f64> {
    image(d, |d, put| circle_pixels_exact(d, put))
}

fn main() {
    for d in [10, 11, 26] {
        let (lin, exact) = (render_linear(d), render_exact(d));
        let (mut visited, mut diagonal) = (0, 0);
        trace_left(d, |s| {
            visited += 1;
            diagonal += (s.kind == Some(Exit::Diagonal)) as i32;
        });
        let want = PI * (d as f64 / 2.0).powi(2);
        let worst = lin.iter().zip(&exact).map(|(p, q)| (p - q).abs()).fold(0.0, f64::max);
        println!(
            "D = {d}: left half visits {visited} pixels, {diagonal} diagonal steps; area {:.3} linear, {:.3} exact, {:.3} = pi R^2; largest pixel difference {:.3}",
            lin.iter().sum::<f64>(),
            exact.iter().sum::<f64>(),
            want,
            worst
        );
        for row in lin.chunks(d as usize) {
            let line: String = row
                .iter()
                .map(|&v| b" .:-=+*#%@"[((v * 9.0).round() as usize).min(9)] as char)
                .collect();
            println!("  {line}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f4(d: i32, i: i32, j: i32) -> i32 {
        let o = d & 1;
        (2 * i - o).pow(2) + (2 * j - o).pow(2) - d * d
    }

    /// 4F at corner k (0 = TL, 1 = TR, 2 = BR, 3 = BL) of pixel (c, y), computed with multiplications.
    fn corner_f(d: i32, c: i32, y: i32, k: u8) -> i32 {
        match k % 4 {
            0 => f4(d, c, y),
            1 => f4(d, c + 1, y),
            2 => f4(d, c + 1, y + 1),
            _ => f4(d, c, y + 1),
        }
    }

    fn steps(d: i32) -> Vec<Step> {
        let mut v = Vec::new();
        trace_left(d, |s| v.push(s));
        v
    }

    fn supersample(d: i32, c: i32, y: i32, n: u32) -> f64 {
        let o = d & 1;
        let r2 = (d as f64 / 2.0).powi(2);
        let x0 = if o == 1 { c as f64 - 0.5 } else { c as f64 };
        let y0 = if o == 1 { y as f64 - 0.5 } else { y as f64 };
        let mut hit = 0u32;
        for j in 0..n {
            for i in 0..n {
                let x = x0 + (i as f64 + 0.5) / n as f64;
                let yy = y0 + (j as f64 + 0.5) / n as f64;
                hit += (x * x + yy * yy <= r2) as u32;
            }
        }
        hit as f64 / (n * n) as f64
    }

    #[test]
    fn walk_matches_f_from_scratch() {
        for d in 1..=200 {
            let st = steps(d);
            for w in st.windows(2) {
                let (s, next) = (w[0], w[1]);
                let (c, y) = (s.c, s.y);
                let f = if y < 0 { f4(d, c, y + 1) } else { f4(d, c + 1, y + 1) };
                let want = if y < 0 {
                    if f < 0 { Exit::Left } else if f == 0 && y + 1 < 0 { Exit::Diagonal } else { Exit::Bottom }
                } else if f < 0 {
                    Exit::Bottom
                } else if f == 0 {
                    Exit::Diagonal
                } else {
                    Exit::Right
                };
                let e = s.kind.unwrap();
                assert_eq!(e, want, "d={d} pixel ({c},{y})");
                let (dc, dy) = (next.c - c, next.y - y);
                let ok = match e {
                    Exit::Left => (dc, dy) == (-1, 0),
                    Exit::Right => (dc, dy) == (1, 0),
                    Exit::Bottom => (dc, dy) == (0, 1),
                    Exit::Diagonal => dy == 1 && dc.abs() == 1,
                };
                assert!(ok, "d={d}: bad move from ({c},{y}) to ({},{})", next.c, next.y);
            }
        }
    }

    #[test]
    fn crossings_are_the_f_values_at_the_ends_of_their_edges() {
        for d in 2..=200 {
            let st = steps(d);
            for (k, s) in st.iter().enumerate() {
                for (name, x) in [("entry", s.entry), ("exit", s.exit)] {
                    let (f0, f1) = (corner_f(d, s.c, s.y, x.side), corner_f(d, s.c, s.y, x.side + 1));
                    assert_eq!((x.f0, x.f1), (f0, f1), "d={d} pixel ({},{}) {name}", s.c, s.y);
                    assert!((f0 < 0) != (f1 < 0), "d={d} pixel ({},{}) {name}: no sign change", s.c, s.y);
                }
                // the carried crossing: the next pixel enters where this one leaves
                if let (Some(e), Some(next)) = (s.kind, st.get(k + 1)) {
                    if e != Exit::Diagonal {
                        assert_eq!(next.entry, across(s.exit), "d={d} pixel ({},{})", s.c, s.y);
                    }
                }
            }
        }
    }

    #[test]
    fn every_pixel_written_once_with_exact_coverage() {
        for d in 1..=130 {
            let h = d >> 1;
            let mut count = vec![0u8; (d * d) as usize];
            let mut value = vec![0.0f64; (d * d) as usize];
            circle_pixels_exact(d, |c, y, v| {
                assert!(c >= -h && c < d - h && y >= -h && y < d - h, "d={d}: pixel ({c},{y}) outside the box");
                let k = ((y + h) * d + (c + h)) as usize;
                count[k] += 1;
                value[k] = v;
            });
            for y in -h..d - h {
                for c in -h..d - h {
                    let k = ((y + h) * d + (c + h)) as usize;
                    let exact = coverage(d, c, y);
                    assert!(count[k] <= 1, "d={d}: pixel ({c},{y}) written {} times", count[k]);
                    if count[k] == 1 {
                        assert!((value[k] - exact).abs() < 1e-9, "d={d} ({c},{y}): {} vs {exact}", value[k]);
                    } else {
                        assert!(exact < 1e-12, "d={d}: pixel ({c},{y}) not written but coverage {exact}");
                    }
                }
            }
        }
    }

    #[test]
    fn linear_version_writes_the_same_pixels() {
        for d in 1..=130 {
            let (lin, exact) = (render_linear(d), render_exact(d));
            for k in 0..lin.len() {
                // both fill the same pixels: a pixel is 0 in one only if it is 0 in the other, and 1 only where 1
                assert_eq!(lin[k] == 0.0, exact[k] == 0.0, "d={d} index {k}: {} vs {}", lin[k], exact[k]);
                assert_eq!(lin[k] == 1.0, exact[k] == 1.0, "d={d} index {k}: {} vs {}", lin[k], exact[k]);
            }
        }
    }

    #[test]
    fn area_is_pi_r_squared_for_exact_coverage() {
        for d in (1..=200).chain([255, 256, 1000, 4001]) {
            let total: f64 = render_exact(d).iter().sum();
            let want = PI * (d as f64 / 2.0).powi(2);
            assert!((total - want).abs() < 1e-8 * want.max(1.0), "d={d}: {total} vs {want}");
        }
    }

    #[test]
    fn coverage_matches_supersampling() {
        for d in 1..=24 {
            let h = d >> 1;
            for y in -h..d - h {
                for c in -h..d - h {
                    let diff = (coverage(d, c, y) - supersample(d, c, y, 200)).abs();
                    assert!(diff < 0.01, "d={d} ({c},{y}): differs by {diff}");
                }
            }
        }
    }

    #[test]
    fn visits_only_cut_pixels() {
        for d in 2..=130 {
            for s in steps(d) {
                let v = coverage(d, s.c, s.y);
                assert!(v > 1e-12 && v < 1.0 - 1e-12, "d={d}: ({},{}) has coverage {v}", s.c, s.y);
            }
        }
    }

    #[test]
    fn two_diagonal_steps_in_a_row_are_half_a_pixel_with_chords() {
        let mut seen = 0;
        for d in (2..=400).step_by(2) {
            let st = steps(d);
            for w in st.windows(2) {
                if w[0].kind == Some(Exit::Diagonal) && w[1].kind == Some(Exit::Diagonal) {
                    let v = chord_coverage(w[1].entry, w[1].exit);
                    assert!((v - 0.5).abs() < 1e-12, "d={d} ({},{}): {v}", w[1].c, w[1].y);
                    let exact = coverage(d, w[1].c, w[1].y);
                    assert!((double_diagonal_coverage(d) - exact).abs() < 1e-9, "d={d}");
                    seen += 1;
                }
            }
        }
        assert!(seen > 0);
        println!("checked {seen} pixels entered and left through corners");
    }

    #[test]
    fn linear_coverage_error_is_small_and_shrinks_with_size() {
        let mut worst_by_size = Vec::new();
        for d in [8, 16, 32, 64, 128, 256, 512] {
            let (lin, exact) = (render_linear(d), render_exact(d));
            let worst = lin.iter().zip(&exact).map(|(p, q)| (p - q).abs()).fold(0.0, f64::max);
            let area: f64 = lin.iter().sum();
            let want = PI * (d as f64 / 2.0).powi(2);
            println!("d={d:4}: largest pixel error {worst:.4}, area error {:+.4} %", (area / want - 1.0) * 100.0);
            worst_by_size.push((d, worst));
        }
        for &(d, worst) in &worst_by_size {
            assert!(worst < 3.0 / d as f64 + 0.02, "d={d}: error {worst}");
        }
    }
}
