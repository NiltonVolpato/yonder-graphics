use yonder_geometry::screen::{Bounded, BoundingBox, Point};
use yonder_geometry::shapes::Rectangle;

use crate::{Coverage, Filled, Rasterizable, Stroked};

impl Rasterizable for Filled<'_, Rectangle> {
    fn rasterize(&self, clip: BoundingBox) -> impl Iterator<Item = Coverage> {
        self.0
            .bounds()
            .intersection(&clip)
            .map(Coverage::Interior)
            .into_iter()
    }
}

impl Rasterizable for Stroked<'_, Rectangle> {
    fn rasterize(&self, clip: BoundingBox) -> impl Iterator<Item = Coverage> {
        let bounds = self.shape.bounds();
        let width = i32::from(self.width);
        let xs = axis(bounds.top_left.x, bounds.bottom_right.x, width);
        let ys = axis(bounds.top_left.y, bounds.bottom_right.y, width);
        ys.flat_map(move |(y0, y1, outer_y, inner_y)| {
            xs.clone().filter_map(move |(x0, x1, outer_x, inner_x)| {
                // Stroke = outer box minus inner box, in quarter pixels (0..=4).
                let quarters = outer_x * outer_y - inner_x * inner_y;
                if quarters == 0 {
                    return None;
                }
                let area = BoundingBox::new(
                    Point::new(saturating_i16(x0), saturating_i16(y0)),
                    Point::new(saturating_i16(x1), saturating_i16(y1)),
                )
                .intersection(&clip)?;
                let coverage = ((quarters * 255 + 2) / 4) as u8;
                Some(match quarters {
                    4 => Coverage::Interior(area),
                    _ if area.width() == 1 && area.height() == 1 => {
                        Coverage::Boundary(area.top_left, coverage)
                    }
                    _ => Coverage::Partial(area, coverage),
                })
            })
        })
    }
}

/// Splits one axis of a centered stroke on `lo..hi` into pixel intervals of constant
/// coverage. Yields `(start, end, outer, inner)`: the interval, and how much of each of
/// its pixels the outer and inner spans cover, in half pixels (0..=2).
fn axis(lo: i16, hi: i16, width: i32) -> impl Iterator<Item = (i32, i32, i32, i32)> + Clone {
    // Half-pixel units, so odd widths land on exact values.
    let (lo, hi) = (2 * i32::from(lo), 2 * i32::from(hi));
    let outer = (lo - width, hi + width);
    let inner = (lo + width, (hi - width).max(lo + width));
    let mut cuts = [0; 8];
    for (i, edge) in [outer.0, outer.1, inner.0, inner.1].into_iter().enumerate() {
        cuts[2 * i] = edge.div_euclid(2);
        cuts[2 * i + 1] = (edge + 1).div_euclid(2);
    }
    cuts.sort_unstable();
    let halves = |(a, b): (i32, i32), p: i32| (b.min(2 * p + 2) - a.max(2 * p)).max(0);
    (0..7usize)
        .filter(move |&i| cuts[i] < cuts[i + 1])
        .map(move |i| {
            (
                cuts[i],
                cuts[i + 1],
                halves(outer, cuts[i]),
                halves(inner, cuts[i]),
            )
        })
}

fn saturating_i16(value: i32) -> i16 {
    value.clamp(i16::MIN.into(), i16::MAX.into()) as i16
}
