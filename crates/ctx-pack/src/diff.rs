// crates/ctx-pack/src/diff.rs
//
// Port of internal/pack/diff.go. PackDiff renders a list of git
// FileDiff entries as markdown. The function is layout-driven:
//   * unified    — fenced ```diff blocks with the raw patch
//   * sequential — Before / After fenced blocks, with commit headers
//   * side-by-side — HTML <table> with Before / After columns
//
// The API-only content rewriting branch (symbols.ExtractPublicAPI
// FromSource) is NOT ported because it depends on tree-sitter via
// the symbols crate, which lives Go-side. The Go dispatcher does
// the rewrite BEFORE shipping the DiffEntry across FFI, so by the
// time we render here BeforeContent / AfterContent are already in
// their final form.

use std::fmt::Write;

use crate::types::{DiffEntry, DiffOptions};

/// Render a list of diff entries. Returns the rendered markdown as
/// a UTF-8 string.
pub fn render(diffs: &[DiffEntry], opts: &DiffOptions) -> String {
    let mut layout = opts.layout.clone();
    if layout.is_empty() {
        layout = "sequential".to_string();
    }
    let mut out = String::new();
    if layout == "unified" {
        for d in diffs {
            if d.binary {
                let _ = write!(out, "### {}\n\nbinary file changed\n\n", d.path);
                continue;
            }
            out.push_str("```diff\n");
            out.push_str(&d.patch);
            if !d.patch.ends_with('\n') {
                out.push('\n');
            }
            out.push_str("```\n");
            out.push('\n');
        }
        return out;
    }
    for d in diffs {
        let before = &d.before_content;
        let after = &d.after_content;
        if d.binary {
            let _ = write!(out, "### {}\n\nbinary file changed\n\n", d.path);
            continue;
        }
        let _ = write!(out, "### {}\n\n", d.path);
        if layout == "side-by-side" {
            write_side_by_side(&mut out, d, before, after);
            continue;
        }
        write_sequential(&mut out, d, before, after);
    }
    out
}

fn write_sequential(out: &mut String, d: &DiffEntry, before: &str, after: &str) {
    let lang = lang_from_path(&d.path);
    if !d.added {
        let _ = write!(out, "**Before** (commit {}):\n", d.before_commit);
        let _ = write!(out, "```{lang}\n{before}");
        if !before.is_empty() && !before.ends_with('\n') {
            out.push('\n');
        }
        out.push_str("```\n");
        out.push('\n');
    }
    if !d.deleted {
        let _ = write!(out, "**After** (commit {}):\n", d.after_commit);
        let _ = write!(out, "```{lang}\n{after}");
        if !after.is_empty() && !after.ends_with('\n') {
            out.push('\n');
        }
        out.push_str("```\n");
        out.push('\n');
    }
}

fn write_side_by_side(out: &mut String, d: &DiffEntry, before: &str, after: &str) {
    let lang = lang_from_path(&d.path);
    out.push_str("<table>\n");
    out.push_str("<tr><th>Before</th><th>After</th></tr>\n");
    out.push_str("<tr><td>\n");
    if !d.added {
        let _ = write!(out, "\n```{lang}\n{before}");
        if !before.is_empty() && !before.ends_with('\n') {
            out.push('\n');
        }
        out.push_str("```\n");
    }
    out.push_str("\n</td><td>\n");
    if !d.deleted {
        let _ = write!(out, "\n```{lang}\n{after}");
        if !after.is_empty() && !after.ends_with('\n') {
            out.push('\n');
        }
        out.push_str("```\n");
    }
    out.push_str("\n</td></tr>\n");
    out.push_str("</table>\n");
    out.push('\n');
}

pub fn lang_from_path(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    let ext = match lower.rsplit_once('.') {
        Some((_, e)) => e,
        None => "",
    };
    match ext {
        "go" => "go",
        "ts" | "tsx" => "typescript",
        "js" | "jsx" | "mjs" => "javascript",
        "py" => "python",
        "rs" => "rust",
        "md" => "markdown",
        "toml" => "toml",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, before: &str, after: &str) -> DiffEntry {
        DiffEntry {
            path: path.into(),
            before_content: before.into(),
            after_content: after.into(),
            before_commit: "abc".into(),
            after_commit: "def".into(),
            patch: format!(
                "--- a/{p}\n+++ b/{p}\n@@ @@\n-{b}\n+{a}\n",
                p = path,
                b = before,
                a = after
            ),
            added: false,
            deleted: false,
            binary: false,
        }
    }

    #[test]
    fn unified_renders_fenced_patch() {
        let d = entry("a.go", "old", "new");
        let r = render(
            &[d],
            &DiffOptions {
                layout: "unified".into(),
                preset: String::new(),
            },
        );
        assert!(r.contains("```diff"));
        assert!(r.contains("@@ @@"));
    }

    #[test]
    fn sequential_default_layout_has_before_after_blocks() {
        let d = entry("a.go", "old", "new");
        let r = render(&[d], &DiffOptions::default());
        assert!(r.contains("**Before**"));
        assert!(r.contains("**After**"));
        assert!(r.contains("```go"));
    }

    #[test]
    fn binary_entry_renders_marker() {
        let mut d = entry("img.png", "", "");
        d.binary = true;
        let r = render(&[d], &DiffOptions::default());
        assert!(r.contains("binary file changed"));
    }

    #[test]
    fn side_by_side_emits_table() {
        let d = entry("x.ts", "old", "new");
        let r = render(
            &[d],
            &DiffOptions {
                layout: "side-by-side".into(),
                preset: String::new(),
            },
        );
        assert!(r.contains("<table>"));
        assert!(r.contains("<th>Before</th>"));
    }

    #[test]
    fn added_skips_before_block() {
        let mut d = entry("a.go", "", "new");
        d.added = true;
        let r = render(&[d], &DiffOptions::default());
        assert!(!r.contains("**Before**"));
        assert!(r.contains("**After**"));
    }

    #[test]
    fn deleted_skips_after_block() {
        let mut d = entry("a.go", "old", "");
        d.deleted = true;
        let r = render(&[d], &DiffOptions::default());
        assert!(r.contains("**Before**"));
        assert!(!r.contains("**After**"));
    }
}
