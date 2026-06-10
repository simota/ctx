use std::path::Path;

use super::*;

/// Display unit for the metric column (mirrors `render.Unit`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextUnit {
    Tokens,
    Chars,
    Pages,
}

/// Mirrors Go's `render.NormalizeUnit`: anything that is not chars/pages → tokens.
pub(crate) fn normalize_text_unit(raw: &str) -> TextUnit {
    match raw.to_ascii_lowercase().as_str() {
        "chars" => TextUnit::Chars,
        "pages" => TextUnit::Pages,
        _ => TextUnit::Tokens,
    }
}

/// Display options for the text tree (mirrors `render.Options`).
pub(crate) struct TextTreeOptions {
    pub(crate) show_git: bool,
    pub(crate) show_tokens: bool,
    pub(crate) show_size: bool,
    pub(crate) show_lines: bool,
    pub(crate) show_symbols: bool,
    pub(crate) plain: bool,
    pub(crate) unit: TextUnit,
}

/// Build the enriched tree (reusing the slice-1/2 `build_root_tree` builder)
/// and render it as the default text tree (mirrors `render.Tree`), then emit the
/// `renderPlanFit` footer when `--plan` is set.
pub(crate) fn render_root_text_tree(
    root: &Path,
    tree_opts: &TreeBuildOpts,
    opts: &TextTreeOptions,
    plan: &str,
) -> Result<(), String> {
    let node = build_root_tree(root, tree_opts)?.ok_or_else(|| "root is ignored".to_string())?;

    let mut out = String::new();
    // render.Tree: print root name (path + "/" for a dir) then its children.
    let mut name = node.path.clone();
    if node.is_dir {
        name.push('/');
    }
    out.push_str(&name);
    out.push('\n');
    let depth = if opts.plain { 1 } else { 0 };
    render_text_children(&mut out, &node.children, "", depth, opts);

    // renderPlanFit footer (mirrors root.go renderPlanFit): appended only when
    // --plan is non-empty and the aggregated root token total is positive.
    render_plan_fit(&mut out, plan, node.metadata.tokens);

    print!("{out}");
    Ok(())
}

/// Mirrors Go's `renderPlanFit` (internal/cli/root.go). Appends the
/// "Total: N tokens" line plus a per-plan fit line for each comma-separated
/// plan name. Unknown plans render "[name: unknown plan]". `total_tokens` is the
/// aggregated root token estimate (`fi.Metadata.TokensEst`).
pub(crate) fn render_plan_fit(out: &mut String, plan: &str, total_tokens: i64) {
    if plan.trim().is_empty() || total_tokens <= 0 {
        return;
    }
    out.push_str(&format!(
        "Total: {} tokens\n",
        format_grouped_number(total_tokens)
    ));
    for name in plan.split(',') {
        let name = name.trim();
        match lookup_plan(name) {
            Some((plan_name, limit)) => {
                let mark = if total_tokens <= limit {
                    "\u{2713}"
                } else {
                    "x"
                };
                let percent = ((total_tokens as f64 / limit as f64) * 100.0 + 0.5).floor() as i64;
                out.push_str(&format!(
                    "[{}: {} {}% of {}]\n",
                    plan_name,
                    mark,
                    percent,
                    format_grouped_number(limit)
                ));
            }
            None => {
                out.push_str(&format!("[{name}: unknown plan]\n"));
            }
        }
    }
}

/// Mirrors `internal/tokens/plans.go` `LookupPlan` (+ the `Plans` table).
/// Lookup is case-insensitive. Returns `(display name, token limit)`.
pub(crate) fn lookup_plan(name: &str) -> Option<(&'static str, i64)> {
    match name.to_ascii_lowercase().as_str() {
        "claude-free" => Some(("Claude Free", 200_000)),
        "claude-pro" => Some(("Claude Pro", 200_000)),
        "gpt-4o" => Some(("GPT-4o", 128_000)),
        "gpt-3.5" => Some(("GPT-3.5", 16_385)),
        "gemini-1.5-pro" => Some(("Gemini 1.5 Pro", 2_000_000)),
        _ => None,
    }
}

/// Mirrors Go's `formatNumber` (root.go): groups digits in threes with commas.
pub(crate) fn format_grouped_number(n: i64) -> String {
    let neg = n < 0;
    let mut s = n.unsigned_abs().to_string();
    if s.len() > 3 {
        let mut parts: Vec<String> = Vec::new();
        while s.len() > 3 {
            let tail = s.split_off(s.len() - 3);
            parts.push(tail);
        }
        parts.push(s);
        parts.reverse();
        s = parts.join(",");
    }
    if neg {
        format!("-{s}")
    } else {
        s
    }
}

// ---------------------------------------------------------------------------
// Native root `--budget` path — ports internal/budget/budget.go (Plan),
// internal/render/budget.go (BudgetWithOptions), and internal/render/json.go
// (JSONBudget). Reuses the slice-1/2 walk+enrich tree builder for the file
// list (path/tokens/role).
// ---------------------------------------------------------------------------

/// Mirrors `render.renderChildren`: connectors + per-node meta, recursing into
/// directories.
pub(crate) fn render_text_children(
    out: &mut String,
    children: &[JsonTreeNode],
    prefix: &str,
    depth: usize,
    opts: &TextTreeOptions,
) {
    const CONNECTOR_MID: &str = "\u{251c}\u{2500} "; // "├─ "
    const CONNECTOR_LAST: &str = "\u{2514}\u{2500} "; // "└─ "
    const PREFIX_MID: &str = "\u{2502}  "; // "│  "
    const PREFIX_LAST: &str = "   ";

    let len = children.len();
    let mut effective_prefix = prefix.to_string();
    for (i, child) in children.iter().enumerate() {
        let is_last = i == len - 1;
        let mut connector = if is_last {
            CONNECTOR_LAST
        } else {
            CONNECTOR_MID
        };
        let mut next_prefix = format!(
            "{}{}",
            effective_prefix,
            if is_last { PREFIX_LAST } else { PREFIX_MID }
        );
        if opts.plain {
            connector = "";
            effective_prefix = "  ".repeat(depth);
            next_prefix = String::new();
        }

        // Name: basename + "/" for dirs.
        let mut name = match child.path.rsplit_once('/') {
            Some((_, base)) => base.to_string(),
            None => child.path.clone(),
        };
        if child.is_dir {
            name.push('/');
        }

        let meta = build_text_meta(child, opts);
        out.push_str(&effective_prefix);
        out.push_str(connector);
        out.push_str(&name);
        out.push_str(&meta);
        out.push('\n');

        if child.is_dir {
            render_text_children(out, &child.children, &next_prefix, depth + 1, opts);
        }
    }
}

/// Mirrors `render.buildMeta`: assembles the trailing metadata string for a node
/// (role, size, lines, git, tokens/chars/pages, symbols).
pub(crate) fn build_text_meta(node: &JsonTreeNode, opts: &TextTreeOptions) -> String {
    let m = &node.metadata;
    let mut parts: Vec<String> = Vec::new();

    // role: rendered when non-empty and not "unknown".
    if !m.role.is_empty() && m.role != "unknown" {
        parts.push(m.role.clone());
    }
    // size: shown when ShowSize && size > 0.
    if opts.show_size && m.size > 0 {
        parts.push(format_text_size(m.size));
    }
    // lines: shown for files when ShowLines && lines > 0.
    if opts.show_lines && !node.is_dir && m.lines > 0 {
        parts.push(format!("{}L", m.lines));
    }
    // git: shown when ShowGit && status non-empty && != "unmodified".
    if opts.show_git && !m.git_status.is_empty() && m.git_status != "unmodified" {
        parts.push(format!("git:{}", m.git_status));
    }
    // tokens/chars/pages metric.
    if opts.show_tokens {
        let value = match opts.unit {
            TextUnit::Chars | TextUnit::Pages => m.chars,
            TextUnit::Tokens => m.tokens,
        };
        if value > 0 {
            parts.push(format_metric_value(value, opts.unit));
        }
    }
    // symbols: shown when ShowSymbols && node has symbols.
    if opts.show_symbols && !m.symbols.is_empty() {
        parts.push(format!("symbols:{}", format_text_symbols(&m.symbols)));
    }

    if parts.is_empty() {
        return String::new();
    }
    format!("   {}", parts.join("  "))
}

/// Mirrors `render.formatSymbols`: first 3 names, then "..." if more.
pub(crate) fn format_text_symbols(symbols: &[SymbolsJsonEntry]) -> String {
    let limit = symbols.len().min(3);
    let mut names: Vec<&str> = symbols[..limit].iter().map(|s| s.name.as_str()).collect();
    if symbols.len() > 3 {
        names.push("...");
    }
    names.join(", ")
}

/// Mirrors `render.formatSize`.
pub(crate) fn format_text_size(n: i64) -> String {
    if n < 1024 {
        format!("{n}B")
    } else if n < 1024 * 1024 {
        format!("{:.1}k", n as f64 / 1024.0)
    } else {
        format!("{:.1}M", n as f64 / 1024.0 / 1024.0)
    }
}

/// Mirrors `render.formatCount`.
pub(crate) fn format_text_count(n: i64) -> String {
    if n < 1000 {
        format!("{n}")
    } else if n < 1000 * 1000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        format!("{:.1}M", n as f64 / 1000.0 / 1000.0)
    }
}

/// Mirrors `render.FormatMetricValue`.
pub(crate) fn format_metric_value(value: i64, unit: TextUnit) -> String {
    match unit {
        TextUnit::Pages => format!("pages:{:.1}", value as f64 / 400.0),
        TextUnit::Chars => format!("chars:{}", format_text_count(value)),
        TextUnit::Tokens => format!("tokens:{}", format_text_count(value)),
    }
}
