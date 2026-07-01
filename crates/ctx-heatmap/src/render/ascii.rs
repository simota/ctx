// crates/ctx-heatmap/src/render/ascii.rs
//
// Rust port of internal/heatmap/render_ascii.go. Byte-exact output is a
// hard requirement — the goldens compare on the raw rendered string.

use crate::aggregate::format_number;
use crate::types::{AsciiOptions, Rect};

/// render_ascii draws a Squarified treemap as a fixed-grid ASCII canvas.
pub fn render_ascii(rects: &[Rect], opts: &AsciiOptions) -> String {
    let w = if opts.width <= 0 { 80 } else { opts.width };
    let h = if opts.height <= 0 { 20 } else { opts.height };

    let mut header = String::new();
    let root = if opts.root.is_empty() {
        ".".to_string()
    } else {
        opts.root.clone()
    };
    let total_tokens: i64 = rects.iter().map(|r| r.bucket.tokens).sum();
    let by = if opts.by.is_empty() {
        "tokens".to_string()
    } else {
        opts.by.clone()
    };

    if opts.budget > 0 {
        header.push_str(&format!(
            "Heatmap (by {}, root={}, total={} tokens, budget={})\n\n",
            by,
            root,
            format_number(total_tokens),
            format_number(opts.budget)
        ));
    } else {
        header.push_str(&format!(
            "Heatmap (by {}, root={}, total={} tokens)\n\n",
            by,
            root,
            format_number(total_tokens)
        ));
    }

    // Row-major byte canvas initialised to spaces.
    let hu = h as usize;
    let wu = w as usize;
    let mut canvas: Vec<Vec<u8>> = vec![vec![b' '; wu]; hu];

    // Greedy budget tracking — stable because Aggregate already sorted.
    let mut used: i64 = 0;
    let mut in_budget: Vec<bool> = Vec::with_capacity(rects.len());
    for r in rects {
        if opts.budget <= 0 {
            in_budget.push(false);
            continue;
        }
        if used + r.bucket.tokens <= opts.budget {
            in_budget.push(true);
            used += r.bucket.tokens;
        } else {
            in_budget.push(false);
        }
    }

    for (i, r) in rects.iter().enumerate() {
        draw_cell(&mut canvas, r, w, h, opts.budget > 0, in_budget[i]);
    }

    let mut out = String::with_capacity(header.len() + (wu + 1) * hu + 64);
    out.push_str(&header);
    for row in &canvas {
        out.push_str(std::str::from_utf8(row).unwrap());
        out.push('\n');
    }
    if opts.budget > 0 {
        out.push_str("\nLegend: # = within budget, . = over budget\n");
    }
    out
}

/// draw_cell mirrors the Go drawCell behaviour byte-for-byte.
fn draw_cell(
    canvas: &mut [Vec<u8>],
    r: &Rect,
    canvas_w: i64,
    canvas_h: i64,
    budget_mode: bool,
    in_budget: bool,
) {
    let mut x = r.x;
    let mut y = r.y;
    let mut w = r.w;
    let mut h = r.h;
    if x >= canvas_w || y >= canvas_h || w <= 0 || h <= 0 {
        return;
    }
    if x + w > canvas_w {
        w = canvas_w - x;
    }
    if y + h > canvas_h {
        h = canvas_h - y;
    }
    // Guards: at this point x, y must be >= 0 (Squarify guarantees this).
    if x < 0 {
        x = 0;
    }
    if y < 0 {
        y = 0;
    }

    let (corner, edge_h, edge_v) = if budget_mode && !in_budget {
        (b'.', b'.', b'.')
    } else {
        (b'+', b'-', b'|')
    };

    // Top + bottom borders.
    for i in 0..w {
        let xi = (x + i) as usize;
        canvas[y as usize][xi] = edge_h;
        if h > 1 {
            canvas[(y + h - 1) as usize][xi] = edge_h;
        }
    }
    // Left + right borders.
    for j in 0..h {
        let yj = (y + j) as usize;
        canvas[yj][x as usize] = edge_v;
        if w > 1 {
            canvas[yj][(x + w - 1) as usize] = edge_v;
        }
    }
    // Corners.
    canvas[y as usize][x as usize] = corner;
    if w > 1 {
        canvas[y as usize][(x + w - 1) as usize] = corner;
    }
    if h > 1 {
        canvas[(y + h - 1) as usize][x as usize] = corner;
        if w > 1 {
            canvas[(y + h - 1) as usize][(x + w - 1) as usize] = corner;
        }
    }

    if w < 3 || h < 3 {
        return;
    }
    let inner_w = w - 2;

    let label = display_label(&r.bucket.path, inner_w);
    write_inside(canvas, x + 1, y + 1, &label, inner_w);

    if h >= 4 {
        let footer = format!(
            "{}t {}f {}s",
            format_number(r.bucket.tokens),
            r.bucket.files,
            r.bucket.symbols
        );
        write_inside(canvas, x + 1, y + 2, &clip(&footer, inner_w), inner_w);
    }

    if budget_mode && !in_budget && h >= 5 {
        write_inside(canvas, x + 1, y + 3, &clip("[OVER]", inner_w), inner_w);
    }
}

fn display_label(path: &str, width: i64) -> String {
    if path == "." {
        return clip("<root>", width);
    }
    if (path.len() as i64) <= width {
        return path.to_string();
    }
    // basename
    let base = match path.rfind('/') {
        Some(idx) => &path[idx + 1..],
        None => path,
    };
    clip(base, width)
}

fn clip(s: &str, width: i64) -> String {
    if width <= 0 {
        return String::new();
    }
    if (s.len() as i64) <= width {
        return s.to_string();
    }
    s[..(width as usize)].to_string()
}

fn write_inside(canvas: &mut [Vec<u8>], x: i64, y: i64, s: &str, width: i64) {
    if y < 0 || y as usize >= canvas.len() {
        return;
    }
    let row = &mut canvas[y as usize];
    let bytes = s.as_bytes();
    let limit = (width as usize).min(bytes.len());
    for i in 0..limit {
        let xi = (x + i as i64) as usize;
        if xi >= row.len() {
            return;
        }
        row[xi] = bytes[i];
    }
}
