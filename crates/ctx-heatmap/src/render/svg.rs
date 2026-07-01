// crates/ctx-heatmap/src/render/svg.rs
//
// Rust port of internal/heatmap/render_svg.go. Byte-exact output is a
// hard requirement — the parity test compares raw bytes against the Go
// oracle.
//
// Parity notes:
//   - Go html.EscapeString replaces exactly five characters:
//       & → &amp;   < → &lt;   > → &gt;   ' → &#39;   " → &#34;
//     (Go's html package; same order as HTML5 spec minimum set.)
//   - Float formatting: Go "%.1f" always emits exactly one decimal digit,
//     using the system's round-half-to-even for the last digit. Rust's
//     `{:.1}` matches this behaviour on IEEE-754.
//   - budgetFlags iterates rects in slice order (greedy fill), matching
//     Go's range-over-slice which is deterministic and index-stable.
//   - svgCanvasSize: starts from opts.width/opts.height, then expands to
//     cover the union of all rect extents; defaults to (80, 20) if the
//     result would be ≤ 0.

use crate::aggregate::format_number;
use crate::types::Rect;

/// SvgOptions mirrors Go's heatmap.SVGOptions.
#[derive(Debug, Clone, Default)]
pub struct SvgOptions {
    pub width: i64,
    pub height: i64,
    pub by: String,
    pub root: String,
    /// 0 disables the budget highlight.
    pub budget: i64,
}

const SVG_STYLE: &str = r#"  <style>
    .cell rect { stroke: #172554; stroke-width: 0.14; }
    .cell text { fill: #111827; font-family: ui-sans-serif, system-ui, sans-serif; font-size: 1.2px; pointer-events: none; }
    .cell .metric { fill: #374151; font-size: 1px; }
    .over-budget rect { stroke-dasharray: 0.8 0.5; }
  </style>
"#;

/// escape_svg matches Go's html.EscapeString exactly: & < > ' "
fn escape_svg(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\'' => out.push_str("&#39;"),
            '"' => out.push_str("&#34;"),
            c => out.push(c),
        }
    }
    out
}

fn svg_path_label(path: &str) -> &str {
    if path == "." {
        "<root>"
    } else {
        path
    }
}

fn svg_canvas_size(rects: &[Rect], opts: &SvgOptions) -> (i64, i64) {
    let mut w = opts.width;
    let mut h = opts.height;
    for r in rects {
        if r.x + r.w > w {
            w = r.x + r.w;
        }
        if r.y + r.h > h {
            h = r.y + r.h;
        }
    }
    if w <= 0 {
        w = 80;
    }
    if h <= 0 {
        h = 20;
    }
    (w, h)
}

/// budget_flags returns a Vec<bool> parallel to rects indicating which
/// rects fit within the cumulative budget (greedy, preserving input order).
fn budget_flags(rects: &[Rect], budget: i64) -> Vec<bool> {
    let mut flags = vec![false; rects.len()];
    if budget <= 0 {
        return flags;
    }
    let mut used: i64 = 0;
    for (i, r) in rects.iter().enumerate() {
        if used + r.bucket.tokens <= budget {
            flags[i] = true;
            used += r.bucket.tokens;
        }
    }
    flags
}

/// render_svg produces a standalone SVG treemap identical byte-for-byte
/// with Go's heatmap.RenderSVG output.
pub fn render_svg(rects: &[Rect], opts: &SvgOptions) -> String {
    let (canvas_w, canvas_h) = svg_canvas_size(rects, opts);
    let root = if opts.root.is_empty() {
        "."
    } else {
        &opts.root
    };
    let by = if opts.by.is_empty() {
        "tokens"
    } else {
        &opts.by
    };

    let total_tokens: i64 = rects.iter().map(|r| r.bucket.tokens).sum();

    let pixel_w = canvas_w * 12;
    let pixel_h = canvas_h * 12;

    let mut out = String::with_capacity(1024 + rects.len() * 256);

    // Opening tag
    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" role=\"img\" aria-labelledby=\"title desc\" viewBox=\"0 0 {} {}\" width=\"{}\" height=\"{}\">\n",
        canvas_w, canvas_h, pixel_w, pixel_h
    ));

    // Title
    out.push_str(&format!(
        "  <title id=\"title\">Heatmap by {}</title>\n",
        escape_svg(by)
    ));

    // Desc
    if opts.budget > 0 {
        out.push_str(&format!(
            "  <desc id=\"desc\">root={}, total={} tokens, budget={}</desc>\n",
            escape_svg(root),
            format_number(total_tokens),
            format_number(opts.budget)
        ));
    } else {
        out.push_str(&format!(
            "  <desc id=\"desc\">root={}, total={} tokens</desc>\n",
            escape_svg(root),
            format_number(total_tokens)
        ));
    }

    // Style block
    out.push_str(SVG_STYLE);

    let in_budget = budget_flags(rects, opts.budget);
    let palette = [
        "#7dd3fc", "#fca5a5", "#86efac", "#fde68a", "#c4b5fd", "#f9a8d4", "#67e8f9", "#fdba74",
    ];

    for (i, r) in rects.iter().enumerate() {
        if r.w <= 0 || r.h <= 0 {
            continue;
        }
        let fill;
        let klass;
        if opts.budget > 0 {
            if in_budget[i] {
                fill = "#86efac";
                klass = "cell in-budget";
            } else {
                fill = "#fca5a5";
                klass = "cell over-budget";
            }
        } else {
            fill = palette[i % palette.len()];
            klass = "cell";
        }

        out.push_str(&format!(
            "  <g class=\"{}\" data-path=\"{}\">\n",
            klass,
            escape_svg(&r.bucket.path)
        ));

        out.push_str(&format!(
            "    <title>{} - {} tokens, {} files, {} symbols</title>\n",
            escape_svg(svg_path_label(&r.bucket.path)),
            format_number(r.bucket.tokens),
            r.bucket.files,
            r.bucket.symbols
        ));

        out.push_str(&format!(
            "    <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"/>\n",
            r.x, r.y, r.w, r.h, fill
        ));

        if r.w >= 8 && r.h >= 4 {
            out.push_str(&format!(
                "    <clipPath id=\"cell-clip-{}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/></clipPath>\n",
                i, r.x, r.y, r.w, r.h
            ));
            out.push_str(&format!("    <g clip-path=\"url(#cell-clip-{})\">\n", i));
            out.push_str(&format!(
                "      <text x=\"{:.1}\" y=\"{:.1}\">{}</text>\n",
                r.x as f64 + 0.7,
                r.y as f64 + 1.6,
                escape_svg(svg_path_label(&r.bucket.path))
            ));
            if r.h >= 6 {
                out.push_str(&format!(
                    "      <text class=\"metric\" x=\"{:.1}\" y=\"{:.1}\">{}t {}f {}s</text>\n",
                    r.x as f64 + 0.7,
                    r.y as f64 + 3.0,
                    format_number(r.bucket.tokens),
                    r.bucket.files,
                    r.bucket.symbols
                ));
            }
            out.push_str("    </g>\n");
        }

        out.push_str("  </g>\n");
    }

    out.push_str("</svg>\n");
    out
}
