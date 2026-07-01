// crates/ctx-heatmap/src/render/plain.rs
//
// Rust port of internal/heatmap/render_plain.go. Byte-exact output is
// required for parity goldens.

use crate::aggregate::{format_number, total_tokens};
use crate::types::{Bucket, PlainOptions};

/// render_plain writes a hierarchical list view suitable for screen-
/// reader narration. Sentences end with periods so TTS engines apply a
/// falling terminal pitch.
pub fn render_plain(buckets: &[Bucket], opts: &PlainOptions) -> String {
    let root = if opts.root.is_empty() {
        ".".to_string()
    } else {
        opts.root.clone()
    };
    let by = if opts.by.is_empty() {
        "tokens".to_string()
    } else {
        opts.by.clone()
    };
    let total = total_tokens(buckets);

    let mut out = String::with_capacity(256 + buckets.len() * 80);
    if opts.budget > 0 {
        out.push_str(&format!(
            "Heatmap (by {}, root={}, total={} tokens, budget={})\n\n",
            by,
            root,
            format_number(total),
            format_number(opts.budget)
        ));
    } else {
        out.push_str(&format!(
            "Heatmap (by {}, root={}, total={} tokens)\n\n",
            by,
            root,
            format_number(total)
        ));
    }

    if buckets.is_empty() {
        out.push_str("No content to display.\n");
        return out;
    }

    let mut used: i64 = 0;
    for (i, b) in buckets.iter().enumerate() {
        let path = if b.path == "." {
            "<root>"
        } else {
            b.path.as_str()
        };
        let files_noun = if b.files == 1 { "file" } else { "files" };
        let sym_noun = if b.symbols == 1 { "symbol" } else { "symbols" };
        let base = format!(
            "{}. {} \u{2014} {} tokens ({} {}, {} {})",
            i + 1,
            path,
            format_number(b.tokens),
            b.files,
            files_noun,
            b.symbols,
            sym_noun
        );
        if opts.budget > 0 {
            let tag = if used + b.tokens <= opts.budget {
                used += b.tokens;
                "[in budget]"
            } else {
                "[over budget]"
            };
            out.push_str(&format!("{} {}.\n", base, tag));
        } else {
            out.push_str(&format!("{}.\n", base));
        }
    }
    out
}
