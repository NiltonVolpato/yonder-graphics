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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Crossing {
    Top(i32, i32),
    Right(i32, i32),
    Bottom(i32, i32),
    Left(i32, i32),

    /// Through a corner that lies exactly on the circle, to the pixel on the other side of that corner.
    Diagonal,
}

impl Crossing {
    /// The same crossing seen from the pixel on the other side of the edge.
    pub fn across(self) -> Self {
        match self {
            Crossing::Top(f0, f1) => Crossing::Bottom(f1, f0),
            Crossing::Right(f0, f1) => Crossing::Left(f1, f0),
            Crossing::Bottom(f0, f1) => Crossing::Top(f1, f0),
            Crossing::Left(f0, f1) => Crossing::Right(f1, f0),
            Crossing::Diagonal => Crossing::Diagonal,
        }
    }
}

/// A crossing: the edge of a pixel where the circle crosses it, and F at both ends of that edge.
/// `side` counts clockwise: 0 = top (TL to TR), 1 = right (TR to BR), 2 = bottom (BR to BL), 3 = left (BL to TL).
/// `f0` is 4F at the start corner of the edge and `f1` at its end corner. Exactly one of them is negative (inside).
/// A corner exactly on the circle has F = 0 and counts as outside.
/// The crossing is at the fraction f0 / (f0 - f1) of the way along the edge, if F is taken as linear along the edge.

/// One pixel of the walk.
#[derive(Clone, Copy, Debug)]
pub struct Step {
    pub c: i32,
    pub y: i32,
    pub entry: Option<Crossing>,
    pub exit: Option<Crossing>,
}

/// Walks the left half of the boundary of a circle that touches all four sides of a `diameter` x `diameter` box,
/// from the top pixel next to the vertical axis down to the bottom pixel next to it, and calls `visit` for each pixel.
/// Pixel (x, y) is the unit square with corners at the integer points (x, y) to (x + 1, y + 1), and y grows downward.
/// The center of the circle is the center of a pixel when the diameter is odd, and a pixel corner when it is even.
///
/// The walk keeps only the current pixel and the crossing where the circle entered it. A crossing holds F at both
/// ends of its edge: 4 * (distance^2 - radius^2) at that corner, where the distance is measured from the center
/// of the circle. It is negative inside the circle, positive outside, and 0 on the circle.
pub fn trace_left(diameter: i32, mut visit: impl FnMut(Step)) {
    assert!(
        (1..=20_000).contains(&diameter),
        "diameter must be in 1..=20000"
    );

    let center_offset = diameter % 2; // 1 if the center is the center of a pixel, 0 if it is a pixel corner
    let axis_x = center_offset - 1; //   the column of the first and last pixel: on the vertical axis, or just left of it
    let first_y = -(diameter / 2);
    let last_y = first_y + diameter - 1;

    if diameter == 1 {
        // One pixel with the whole circle inside it and no corner inside the circle: no edge is crossed.
        visit(Step {
            x: axis_x,
            y: first_y,
            entry: None,
            exit: None,
        });
        return;
    }

    // F at the top-left corner of pixel (corner_x, corner_y), computed with multiplications. Only the first and last pixel use it.
    let corner_f = |corner_x: i32, corner_y: i32| {
        let dx2 = 2 * corner_x - center_offset; // the distances from the center, doubled
        let dy2 = 2 * corner_y - center_offset;
        dx2 * dx2 + dy2 * dy2 - diameter * diameter
    };
    // How much F grows when a corner moves from index `index` to `index + 1`, along either axis. It does not depend
    // on the other coordinate. This is how the walk gets from one corner of a pixel to the next: one add.
    let f_growth = |index: i32| (2 * index + 1 - center_offset) << 2;

    let (mut x, mut y) = (axis_x, first_y);

    // The circle enters the first pixel and leaves the last pixel through their right edges, which are
    // on the vertical axis, or next to it.
    let mut entry = Crossing::Right(corner_f(x + 1, y), corner_f(x + 1, y + 1));

    // Upper half. The circle runs down and to the left, so it leaves each pixel through its left edge or
    // its bottom edge, and F at the pixel's bottom-left corner tells which.
    while y < 0 {
        let bottom_left = match entry {
            Crossing::Right(_, bottom_right) => bottom_right - f_growth(x),
            Crossing::Top(top_left, _) => top_left + f_growth(y),
            Crossing::Diagonal => f_growth(y) - f_growth(x), // entered at the top-right corner, where F is 0
            Crossing::Left(..) | Crossing::Bottom(..) => unreachable!(),
        };
        let exit = if bottom_left < 0 {
            Crossing::Left(bottom_left, bottom_left - f_growth(y))
        } else if bottom_left == 0 && y + 1 < 0 {
            // The bottom-left corner is exactly on the circle, and not on the horizontal axis. The pixels on the
            // other sides of the corner are covered completely or not at all, so go through the corner.
            // (On the horizontal axis the circle is vertical, and the next pixel down is cut.)
            Crossing::Diagonal
        } else {
            Crossing::Bottom(bottom_left + f_growth(x), bottom_left)
        };
        visit(Step {
            x,
            y,
            entry: Some(entry),
            exit: Some(exit),
        });
        entry = exit.across();
        (x, y) = exit.next_pixel(x, y, -1);
    }

    // Lower half. The circle runs down and to the right, so it leaves each pixel through its bottom edge or
    // its right edge, and F at the pixel's bottom-right corner tells which.
    while (x, y) != (axis_x, last_y) {
        let bottom_right = match entry {
            Crossing::Left(bottom_left, _) => bottom_left + f_growth(x),
            Crossing::Top(_, top_right) => top_right + f_growth(y),
            Crossing::Diagonal => f_growth(x) + f_growth(y), // entered at the top-left corner, where F is 0
            Crossing::Right(..) | Crossing::Bottom(..) => unreachable!(),
        };
        let exit = if bottom_right < 0 {
            Crossing::Bottom(bottom_right, bottom_right - f_growth(x))
        } else if bottom_right == 0 {
            Crossing::Diagonal // as above
        } else {
            Crossing::Right(bottom_right - f_growth(y), bottom_right)
        };
        visit(Step {
            x,
            y,
            entry: Some(entry),
            exit: Some(exit),
        });
        entry = exit.across();
        (x, y) = exit.next_pixel(x, y, 1);
    }

    visit(Step {
        x,
        y,
        entry: Some(entry),
        exit: Some(Crossing::Right(corner_f(x + 1, y), corner_f(x + 1, y + 1))),
    });
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

    let mut next_entry = None; // the crossing carried from the previous pixel

    for _ in 0..16 * d + 64 {
        if c == c_end && y == y_end {
            // The last pixel is the mirror image of the first. Its exit is the mirror image of its entry.
            let exit = if d == 1 {
                None
            } else if o == 1 {
                entry.map(Crossing::across) // the entry is on the left edge, so the exit is on the right edge with the same values
            } else {
                // The circle leaves through the corner (0, y_end + 1), which is on the circle.
                Some(Crossing::Right(4 * y_end * y_end - d * d, 0))
            };
            visit(Step { c, y, entry, exit });
            return;
        }
        let (pc, py) = (c, y);
        let (exit, next_entry) = if y < 0 {
            // Upper half: the arc runs down and left. Test the bottom left corner.
            if f < 0 {
                let x = Crossing::Left(f, f - ((b - 1) << 2));
                f -= (a - 1) << 2;
                a -= 2;
                c -= 1;
                (x.clone(), x.across())
            } else if f == 0 && b < 0 {
                // The corner is on the circle: go through it. F is 0 here, so only the two steps remain.
                let x = Crossing {
                    side: 2,
                    f0: (a + 1) << 2,
                    f1: 0,
                };
                let n = Crossing {
                    side: 1,
                    f0: 0,
                    f1: (b + 1) << 2,
                };
                f = -((a - 1) << 2);
                a -= 2;
                f += (b + 1) << 2;
                b += 2;
                c -= 1;
                y += 1;
                (Exit::Diagonal, x, n)
            } else {
                let x = Crossing {
                    side: 2,
                    f0: f + ((a + 1) << 2),
                    f1: f,
                };
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
            let x = Crossing {
                side: 2,
                f0: f,
                f1: f - ((a - 1) << 2),
            };
            f += (b + 1) << 2;
            b += 2;
            y += 1;
            (Exit::Bottom, x, across(x))
        } else if f == 0 {
            let x = Crossing {
                side: 1,
                f0: -((b - 1) << 2),
                f1: 0,
            };
            let n = Crossing {
                side: 0,
                f0: 0,
                f1: (a + 1) << 2,
            };
            f = (a + 1) << 2;
            a += 2;
            f += (b + 1) << 2;
            b += 2;
            c += 1;
            y += 1;
            (Exit::Diagonal, x, n)
        } else {
            let x = Crossing {
                side: 1,
                f0: f - ((b - 1) << 2),
                f1: f,
            };
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
                Crossing {
                    side: 1,
                    f0: 0,
                    f1: 4 * (h - 1) * (h - 1) - d * d,
                }
            };
        }
        visit(Step {
            c: pc,
            y: py,
            entry,
            exit,
        });
        entry = next_entry;
    }
    unreachable!("the walk did not reach its last pixel");
}

#[cfg(test)]
mod tests {
    extern crate alloc;
    use super::*;
    use alloc::vec::Vec;

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

    #[test]
    fn walk_matches_f_from_scratch() {
        for d in 1..=200 {
            let st = steps(d);
            for w in st.windows(2) {
                let (s, next) = (w[0], w[1]);
                let (c, y) = (s.c, s.y);
                let f = if y < 0 {
                    f4(d, c, y + 1)
                } else {
                    f4(d, c + 1, y + 1)
                };
                let want = if y < 0 {
                    if f < 0 {
                        Exit::Left
                    } else if f == 0 && y + 1 < 0 {
                        Exit::Diagonal
                    } else {
                        Exit::Bottom
                    }
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
                assert!(
                    ok,
                    "d={d}: bad move from ({c},{y}) to ({},{})",
                    next.c, next.y
                );
            }
        }
    }

    #[test]
    fn crossings_are_the_f_values_at_the_ends_of_their_edges() {
        for d in 2..=200 {
            let st = steps(d);
            for (k, s) in st.iter().enumerate() {
                for (name, x) in [("entry", s.entry), ("exit", s.exit)] {
                    let (f0, f1) = (
                        corner_f(d, s.c, s.y, x.side),
                        corner_f(d, s.c, s.y, x.side + 1),
                    );
                    assert_eq!(
                        (x.f0, x.f1),
                        (f0, f1),
                        "d={d} pixel ({},{}) {name}",
                        s.c,
                        s.y
                    );
                    assert!(
                        (f0 < 0) != (f1 < 0),
                        "d={d} pixel ({},{}) {name}: no sign change",
                        s.c,
                        s.y
                    );
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
}
