// crates/ctx-contract/src/format.rs
//
// Port of internal/contract/format.go. Renders a verify Result as
// markdown, plain text, or JSON. Unknown formats fall back to markdown,
// matching Go's switch-default.

use std::collections::BTreeMap;
use std::io::{self, Write};

use crate::types::{OK, Result as VerifyResult, Violation, ViolationKind};

/// Emits the verify result in the requested format. Unknown format
/// strings fall back to markdown.
pub fn render<W: Write + ?Sized>(
    w: &mut W,
    res: &VerifyResult,
    format: &str,
) -> io::Result<()> {
    match format.to_ascii_lowercase().as_str() {
        "json" => render_json(w, res),
        "plain" => render_plain(w, res),
        _ => render_markdown(w, res),
    }
}

fn render_json<W: Write + ?Sized>(w: &mut W, res: &VerifyResult) -> io::Result<()> {
    // Result's Serialize impl already preserves [] for the four
    // parity-critical collections (no skip_serializing_if), so we can
    // hand it straight to serde_json.
    let body = serde_json::to_string_pretty(res).map_err(io::Error::other)?;
    w.write_all(body.as_bytes())?;
    w.write_all(b"\n")?;
    Ok(())
}

fn render_markdown<W: Write + ?Sized>(w: &mut W, res: &VerifyResult) -> io::Result<()> {
    writeln!(
        w,
        "# CTX-CONTRACT-VERIFY: pack={} schema={} files={}\n",
        display_path(&res.pack_file),
        res.schema_version,
        res.total_files_in_contract
    )?;

    if res.violations.is_empty() {
        writeln!(w, "## Violations (0)")?;
        writeln!(w)?;
        writeln!(w, "(none — every reference matched the contract)")?;
        writeln!(w)?;
    } else {
        writeln!(w, "## Violations ({})\n", res.violations.len())?;
        let grouped = group_violations(&res.violations);
        for kind in sorted_kinds(&grouped) {
            writeln!(w, "### {}", kind.as_str())?;
            if let Some(items) = grouped.get(&kind) {
                for v in items {
                    writeln!(w, "- {}", describe_violation(v))?;
                    if v.source_line > 0 {
                        writeln!(w, "  source: line {} of stdin", v.source_line)?;
                    }
                }
            }
            writeln!(w)?;
        }
    }

    writeln!(w, "## OK ({})", res.ok.len())?;
    if res.ok.is_empty() {
        writeln!(w, "(no verifiable references found)")?;
    } else {
        for o in &res.ok {
            writeln!(w, "- {}", describe_ok(o))?;
        }
    }
    writeln!(w)?;

    if !res.stale_files.is_empty() {
        writeln!(w, "## Stale Files ({})", res.stale_files.len())?;
        for sf in &res.stale_files {
            writeln!(w, "- `{}` — {}", sf.path, sf.message)?;
        }
        writeln!(w)?;
    }

    if !res.repack_suggestions.is_empty() {
        writeln!(w, "## Repack Suggestions")?;
        for p in &res.repack_suggestions {
            writeln!(w, "- include `{}` in a fresh pack", p)?;
        }
        writeln!(w)?;
    }

    writeln!(w, "## Exit")?;
    if res.exit_code == 0 {
        writeln!(w, "0 — no violations detected")?;
    } else {
        writeln!(w, "1 — {} violation(s) detected", res.violations.len())?;
    }
    Ok(())
}

fn render_plain<W: Write + ?Sized>(w: &mut W, res: &VerifyResult) -> io::Result<()> {
    writeln!(
        w,
        "pack={} schema={} files={} refs={} violations={} ok={} exit={}",
        display_path(&res.pack_file),
        res.schema_version,
        res.total_files_in_contract,
        res.references_found,
        res.violations.len(),
        res.ok.len(),
        res.exit_code
    )?;
    for v in &res.violations {
        writeln!(
            w,
            "V\t{}\t{}\tline={}",
            v.kind.as_str(),
            describe_violation(v),
            v.source_line
        )?;
    }
    for o in &res.ok {
        writeln!(
            w,
            "O\t{}\t{}\tline={}",
            o.kind,
            describe_ok(o),
            o.source_line
        )?;
    }
    for sf in &res.stale_files {
        writeln!(w, "S\t{}\t{}", sf.path, sf.message)?;
    }
    for p in &res.repack_suggestions {
        writeln!(w, "R\t{}", p)?;
    }
    Ok(())
}

fn group_violations(in_: &[Violation]) -> BTreeMap<ViolationKind, Vec<Violation>> {
    let mut out: BTreeMap<ViolationKind, Vec<Violation>> = BTreeMap::new();
    for v in in_ {
        out.entry(v.kind).or_default().push(v.clone());
    }
    out
}

fn sorted_kinds(m: &BTreeMap<ViolationKind, Vec<Violation>>) -> Vec<ViolationKind> {
    let mut keys: Vec<ViolationKind> = m.keys().copied().collect();
    keys.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    keys
}

fn describe_violation(v: &Violation) -> String {
    match v.kind {
        ViolationKind::PhantomSymbol => {
            format!("`{}` referenced in response but not in pack", v.symbol)
        }
        ViolationKind::StaleContent => format!(
            "`{}:{}-{}` referenced with content not matching pack span",
            v.path, v.line_start, v.line_end
        ),
        ViolationKind::OutOfContext => {
            if !v.symbol.is_empty() {
                format!("`{}` symbol referenced outside pack contract", v.symbol)
            } else if v.line_start > 0 {
                format!(
                    "`{}:{}-{}` referenced but file is not in pack",
                    v.path, v.line_start, v.line_end
                )
            } else if !v.path.is_empty() {
                format!("`{}` referenced but path is not in pack", v.path)
            } else {
                v.message.clone()
            }
        }
    }
}

fn describe_ok(o: &OK) -> String {
    match o.kind.as_str() {
        "file" => format!("`{}` (file ref, line {})", o.path, o.source_line),
        "line-range" => format!(
            "`{}:{}-{}` (line ref, line {})",
            o.path, o.line_start, o.line_end, o.source_line
        ),
        "symbol" => format!("`{}` (symbol, line {})", o.symbol, o.source_line),
        "diff-header" => format!("`{}` (diff header, line {})", o.path, o.source_line),
        _ => format!("{} (line {})", o.kind, o.source_line),
    }
}

fn display_path(p: &str) -> &str {
    if p.is_empty() || p == "-" {
        "(stdin)"
    } else {
        p
    }
}

// BTreeMap on ViolationKind needs Ord — provide based on string repr.
impl Ord for ViolationKind {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_str().cmp(other.as_str())
    }
}
impl PartialOrd for ViolationKind {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
