// crates/ctx-heatmap/src/squarify.rs
//
// Rust port of internal/heatmap/squarify.go.
//
// FLOATING-POINT PARITY: this is the critical port. We mirror Go's
// formula structure exactly — same comparison order, same rounding via
// `f64::round` (matches math.Round's "round half away from zero"
// semantics, since both implementations agree on IEEE-754 ties).
//
// f64::round on stable Rust returns the nearest integer, rounding half
// AWAY from zero — identical to Go's math.Round.

use crate::types::{Bucket, Rect};

#[derive(Clone, Copy)]
struct LayoutState {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// Squarify computes a Squarified treemap layout for the given buckets
/// inside the rectangle [0, w) x [0, h).
pub fn squarify(buckets: &[Bucket], w: i64, h: i64) -> Vec<Rect> {
    if buckets.is_empty() || w <= 0 || h <= 0 {
        return Vec::new();
    }

    let mut clean: Vec<Bucket> = Vec::with_capacity(buckets.len());
    let mut total: f64 = 0.0;
    for b in buckets {
        if b.weight <= 0.0 {
            continue;
        }
        clean.push(b.clone());
        total += b.weight;
    }
    if clean.is_empty() || total <= 0.0 {
        return Vec::new();
    }

    let area = (w as f64) * (h as f64);
    let scaled: Vec<f64> = clean.iter().map(|b| b.weight / total * area).collect();

    let mut rects: Vec<Rect> = Vec::with_capacity(clean.len());
    let mut state = LayoutState {
        x: 0.0,
        y: 0.0,
        w: w as f64,
        h: h as f64,
    };
    let mut row: Vec<usize> = Vec::with_capacity(clean.len());

    for i in 0..clean.len() {
        if row.is_empty() {
            // First element of a fresh row always joins.
            row.push(i);
            continue;
        }
        let short_side = state.w.min(state.h);
        let cur_areas = row_areas(&row, &scaled);
        let mut next_row = row.clone();
        next_row.push(i);
        let next_areas = row_areas(&next_row, &scaled);
        if worst(&cur_areas, short_side) <= worst(&next_areas, short_side) {
            // Adding i made the worst aspect ratio worse — flush + restart.
            state = flush_row(&row, &scaled, state, &mut rects, &clean);
            row.clear();
            row.push(i);
        } else {
            row = next_row;
        }
    }
    if !row.is_empty() {
        flush_row(&row, &scaled, state, &mut rects, &clean);
    }
    rects
}

fn row_areas(row: &[usize], scaled: &[f64]) -> Vec<f64> {
    row.iter().map(|&i| scaled[i]).collect()
}

/// worst returns the maximum aspect ratio (always >= 1) that would
/// result from laying the row of areas out along the short side. Same
/// formula as Go's heatmap.worst — preserves order of multiplications
/// to keep float results bit-identical.
fn worst(areas: &[f64], w: f64) -> f64 {
    if areas.is_empty() || w <= 0.0 {
        return f64::INFINITY;
    }
    let mut sum = 0.0_f64;
    let mut rmax = 0.0_f64;
    let mut rmin = f64::INFINITY;
    for &a in areas {
        sum += a;
        if a > rmax {
            rmax = a;
        }
        if a < rmin {
            rmin = a;
        }
    }
    if sum <= 0.0 || rmin <= 0.0 {
        return f64::INFINITY;
    }
    let w2 = w * w;
    let s2 = sum * sum;
    // math.Max(w2*rmax/s2, s2/(w2*rmin))
    (w2 * rmax / s2).max(s2 / (w2 * rmin))
}

/// flush_row mirrors Go's flushRow exactly:
///   - vertical row when state.w >= state.h (pack along H)
///   - cumulative int rounding so the last cell consumes the remainder
fn flush_row(
    row: &[usize],
    scaled: &[f64],
    mut state: LayoutState,
    rects: &mut Vec<Rect>,
    buckets: &[Bucket],
) -> LayoutState {
    if row.is_empty() {
        return state;
    }
    let mut row_sum = 0.0_f64;
    for &i in row {
        row_sum += scaled[i];
    }
    if row_sum <= 0.0 {
        return state;
    }

    if state.w >= state.h {
        let row_w = row_sum / state.h;
        let mut int_w = row_w.round() as i64;
        if int_w < 1 {
            int_w = 1;
        }
        // Don't run past the right edge.
        let max_w = (state.x + state.w).round() as i64 - state.x.round() as i64;
        if int_w > max_w {
            int_w = max_w;
        }
        let mut y_pos = state.y;
        let x0 = state.x.round() as i64;
        let y_start = state.y.round() as i64;
        let h_remain = (state.y + state.h).round() as i64 - y_start;
        let mut used_h: i64 = 0;
        for (k, &idx) in row.iter().enumerate() {
            let cell_h = scaled[idx] / row_w;
            y_pos += cell_h;
            let mut y_end = y_pos.round() as i64;
            if k == row.len() - 1 {
                y_end = y_start + h_remain;
            }
            let mut cell_h_int = y_end - (y_start + used_h);
            if cell_h_int < 1 {
                cell_h_int = 1;
            }
            rects.push(Rect {
                bucket: buckets[idx].clone(),
                x: x0,
                y: y_start + used_h,
                w: int_w,
                h: cell_h_int,
            });
            used_h += cell_h_int;
        }
        state.x += int_w as f64;
        state.w -= int_w as f64;
    } else {
        let row_h = row_sum / state.w;
        let mut int_h = row_h.round() as i64;
        if int_h < 1 {
            int_h = 1;
        }
        let max_h = (state.y + state.h).round() as i64 - state.y.round() as i64;
        if int_h > max_h {
            int_h = max_h;
        }
        let mut x_pos = state.x;
        let y0 = state.y.round() as i64;
        let x_start = state.x.round() as i64;
        let w_remain = (state.x + state.w).round() as i64 - x_start;
        let mut used_w: i64 = 0;
        for (k, &idx) in row.iter().enumerate() {
            let cell_w = scaled[idx] / row_h;
            x_pos += cell_w;
            let mut x_end = x_pos.round() as i64;
            if k == row.len() - 1 {
                x_end = x_start + w_remain;
            }
            let mut cell_w_int = x_end - (x_start + used_w);
            if cell_w_int < 1 {
                cell_w_int = 1;
            }
            rects.push(Rect {
                bucket: buckets[idx].clone(),
                x: x_start + used_w,
                y: y0,
                w: cell_w_int,
                h: int_h,
            });
            used_w += cell_w_int;
        }
        state.y += int_h as f64;
        state.h -= int_h as f64;
    }
    state
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bk(path: &str, weight: f64) -> Bucket {
        Bucket {
            path: path.into(),
            weight,
            ..Default::default()
        }
    }

    #[test]
    fn area_conservation() {
        let buckets = vec![
            bk("a", 50.0),
            bk("b", 30.0),
            bk("c", 15.0),
            bk("d", 5.0),
        ];
        let (w, h) = (80, 20);
        let rects = squarify(&buckets, w, h);
        assert_eq!(rects.len(), buckets.len());
        let mut total_area: i64 = 0;
        let mut covered = vec![vec![false; w as usize]; h as usize];
        for r in &rects {
            assert!(r.x >= 0 && r.y >= 0 && r.x + r.w <= w && r.y + r.h <= h, "{r:?}");
            total_area += r.w * r.h;
            for y in r.y..r.y + r.h {
                for x in r.x..r.x + r.w {
                    assert!(!covered[y as usize][x as usize], "overlap at {x},{y}");
                    covered[y as usize][x as usize] = true;
                }
            }
        }
        assert_eq!(total_area, w * h);
    }

    #[test]
    fn aspect_ratio_reasonable() {
        let buckets = vec![
            bk("a", 40.0),
            bk("b", 30.0),
            bk("c", 20.0),
            bk("d", 10.0),
        ];
        let rects = squarify(&buckets, 60, 20);
        for r in &rects {
            assert!(r.w > 0 && r.h > 0);
            let mut ratio = r.w as f64 / r.h as f64;
            if ratio < 1.0 {
                ratio = 1.0 / ratio;
            }
            assert!(ratio <= 10.0, "ratio {ratio} for {r:?}");
        }
    }

    #[test]
    fn empty_and_degenerate() {
        assert!(squarify(&[], 80, 20).is_empty());
        assert!(squarify(&[bk("a", 1.0)], 0, 20).is_empty());
        assert!(squarify(&[bk("a", 0.0)], 80, 20).is_empty());
    }
}
