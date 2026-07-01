// crates/ctx-braid/src/format.rs
//
// Port of internal/braid/format.go — markdown / json / plain renderers
// for the allocation report. The pack body itself is written by Run()
// in Go and is intentionally not part of the Rust crate (out of scope
// per Tier 2 #1 brief).

use std::fmt::Write;

use crate::types::Result;

/// renderMarkdown mirrors Go's `renderMarkdown` byte-for-byte.
pub fn render_markdown(res: &Result, explain: bool) -> String {
    let mut out = String::new();
    let mode = if res.dry_run { "dry-run" } else { "pack" };
    writeln!(
        out,
        "# CTX-BRAID: file={} strands={} budget={} selected={} tokens_used={} mode={}\n",
        res.file,
        res.strands.len(),
        res.budget,
        res.files.len(),
        res.tokens_used,
        mode
    )
    .unwrap();
    out.push_str("## Strand allocation\n");
    out.push('\n');
    out.push_str("| Strand | Share | Budget | Selected | Tokens | Policy |\n");
    out.push_str("|--------|-------|--------|----------|--------|--------|\n");
    for s in &res.strands {
        // Go: fmt.Fprintf(w, "| %s | %.2f | %d | %d | %d | %s |\n", ...)
        writeln!(
            out,
            "| {} | {:.2} | {} | {} | {} | {} |",
            s.name,
            s.share,
            s.budget,
            s.selected,
            s.tokens,
            s.policy.as_str()
        )
        .unwrap();
    }
    out.push('\n');

    if explain || res.dry_run {
        out.push_str("## Files (by strand)\n");
        out.push('\n');
        let mut current = String::new();
        for f in &res.files {
            if f.origin != current {
                current = f.origin.clone();
                writeln!(out, "### Strand: {current}\n").unwrap();
            }
            writeln!(out, "- {}", f.path).unwrap();
        }
        out.push('\n');
    }

    if res.dry_run {
        out.push_str("## Dry run\n");
        out.push_str(
            "Pack body not generated. Re-run without --dry-run to materialise the bundle.\n",
        );
    }
    out
}

/// renderPlain mirrors Go's `renderPlain` byte-for-byte.
pub fn render_plain(res: &Result) -> String {
    let mut out = String::new();
    writeln!(
        out,
        "file={} budget={} strands={} selected={} tokens={} dry_run={}",
        res.file,
        res.budget,
        res.strands.len(),
        res.files.len(),
        res.tokens_used,
        if res.dry_run { "true" } else { "false" }
    )
    .unwrap();
    for s in &res.strands {
        writeln!(
            out,
            "{}\tshare={:.3}\tbudget={}\tselected={}\ttokens={}\tpolicy={}",
            s.name,
            s.share,
            s.budget,
            s.selected,
            s.tokens,
            s.policy.as_str()
        )
        .unwrap();
    }
    for f in &res.files {
        writeln!(out, "{}\t{}", f.origin, f.path).unwrap();
    }
    out
}

/// renderJSON mirrors Go's `json.Encoder.SetIndent("", "  ").Encode(res)`
/// — 2-space indent, trailing newline.
pub fn render_json(res: &Result) -> std::result::Result<Vec<u8>, serde_json::Error> {
    let mut buf = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"  ");
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
    serde::Serialize::serialize(res, &mut ser)?;
    buf.push(b'\n');
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{MergedFile, PolicyKind, StrandReport};

    fn sample_result() -> Result {
        Result {
            file: "braid.toml".into(),
            budget: 32000,
            strands: vec![StrandReport {
                name: "a".into(),
                share: 0.5,
                budget: 16000,
                selected: 2,
                tokens: 1200,
                policy: PolicyKind::Merge,
                raw_paths: 3,
                trim_note: String::new(),
            }],
            files: vec![MergedFile {
                path: "internal/a.go".into(),
                origin: "a".into(),
            }],
            tokens_used: 1200,
            dry_run: false,
            pack_bytes: 0,
            pack_sha256: String::new(),
        }
    }

    #[test]
    fn markdown_contains_header_and_table() {
        let md = render_markdown(&sample_result(), false);
        assert!(md.contains("# CTX-BRAID:"));
        assert!(md.contains("| a | 0.50 |"));
        assert!(!md.contains("## Files"));
    }

    #[test]
    fn plain_renders_tab_rows() {
        let p = render_plain(&sample_result());
        assert!(p.starts_with("file=braid.toml budget=32000"));
        assert!(p.contains("a\tshare=0.500\tbudget=16000"));
    }

    #[test]
    fn json_pretty_prints_with_newline() {
        let j = render_json(&sample_result()).unwrap();
        let s = String::from_utf8(j).unwrap();
        assert!(s.ends_with('\n'));
        assert!(s.contains("\"file\": \"braid.toml\""));
        assert!(s.contains("  \"strands\":"));
    }
}
