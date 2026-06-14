use std::env;
use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use rayon::prelude::*;

use crate::common::*;
use ctx_gitignore::GitIgnore;

#[derive(Debug)]
pub(crate) struct WhereArgs {
    root: PathBuf,
    query: String,
    limit: i64,
    format: String,
    context_n: i64,
    no_suggest: bool,
    require_all: bool,
    regex: String,
    explain: bool,
    plain: bool,
}

pub(crate) fn run_where_command(args: &[OsString]) -> Option<ExitCode> {
    let parsed = parse_where_args(args)?;
    match where_command(parsed) {
        Ok(()) => Some(ExitCode::SUCCESS),
        Err(err) => {
            eprintln!("Error: {err}");
            Some(ExitCode::from(1))
        }
    }
}

pub(crate) fn parse_where_args(args: &[OsString]) -> Option<WhereArgs> {
    let mut json = false;
    let mut plain = false;
    let mut limit = 10;
    let mut format = "default".to_string();
    let mut context_n = 0;
    let mut no_suggest = false;
    let mut require_all = false;
    let mut regex = String::new();
    let mut explain = false;
    let mut saw_where = false;
    let mut positionals: Vec<OsString> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == OsStr::new("--json") {
            json = true;
        } else if arg == OsStr::new("--plain") {
            plain = true;
        } else if arg == OsStr::new("where") {
            if saw_where {
                return None;
            }
            saw_where = true;
        } else if let Some(value) = flag_value(arg, "--limit") {
            limit = value.to_string_lossy().parse().ok()?;
        } else if arg == OsStr::new("--limit") {
            i += 1;
            limit = args.get(i)?.to_string_lossy().parse().ok()?;
        } else if let Some(value) = flag_value(arg, "--format") {
            format = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--format") {
            i += 1;
            format = args.get(i)?.to_string_lossy().into_owned();
        } else if let Some(value) = flag_value(arg, "--context") {
            context_n = value.to_string_lossy().parse().ok()?;
        } else if arg == OsStr::new("--context") {
            i += 1;
            context_n = args.get(i)?.to_string_lossy().parse().ok()?;
        } else if arg == OsStr::new("--no-suggest") {
            no_suggest = true;
        } else if arg == OsStr::new("--all") {
            require_all = true;
        } else if let Some(value) = flag_value(arg, "--regex") {
            regex = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--regex") {
            i += 1;
            regex = args.get(i)?.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--explain") {
            explain = true;
        } else if flag_value(arg, "--where-engine").is_some() {
        } else if arg == OsStr::new("--where-engine") {
            i += 1;
            args.get(i)?;
        } else if is_option(arg) {
            return None;
        } else if saw_where {
            positionals.push(arg.clone());
        } else {
            return None;
        }
        i += 1;
    }
    if !saw_where || positionals.len() > 2 {
        return None;
    }
    if json {
        format = "json".to_string();
    }
    let (query, root) = match positionals.as_slice() {
        [] => (String::new(), PathBuf::from(".")),
        [query] => (query.to_string_lossy().into_owned(), PathBuf::from(".")),
        [query, root] => (query.to_string_lossy().into_owned(), PathBuf::from(root)),
        _ => unreachable!(),
    };
    Some(WhereArgs {
        root,
        query,
        limit,
        format,
        context_n,
        no_suggest,
        require_all,
        regex,
        explain,
        plain,
    })
}

pub(crate) fn where_command(args: WhereArgs) -> Result<(), String> {
    if args.query.is_empty() && args.regex.is_empty() {
        return Err("where: requires a query argument or --regex pattern".to_string());
    }
    if !args.regex.is_empty() {
        regex::Regex::new(&args.regex).map_err(|err| format!("invalid --regex pattern: {err}"))?;
    }
    let root = if args.root.is_absolute() {
        args.root.clone()
    } else {
        env::current_dir()
            .map_err(|err| err.to_string())?
            .join(&args.root)
    };
    let files = where_files(&root)?;
    let opts = ctx_where::Options {
        limit: args.limit,
        context_n: args.context_n,
        require_all: args.require_all,
        regex: args.regex.clone(),
        synonyms: Default::default(),
        explain: args.explain,
    };
    let results = ctx_where::search_with_options(&files, &args.query, &opts);
    let suggestions = if results.is_empty() && !args.no_suggest && args.regex.is_empty() {
        ctx_where::suggest_similar(&files, &args.query, 3)
    } else {
        Vec::new()
    };
    render_where(&results, &suggestions, &args)
}

/// Walker ignore options, mirroring Go `walk.Options` (the ignore subset).
#[derive(Debug, Clone)]
pub(crate) struct WalkIgnoreOptions {
    pub(crate) respect_gitignore: bool,
    pub(crate) respect_ctxignore: bool,
    pub(crate) extra_ignore: Vec<String>,
}

impl WalkIgnoreOptions {
    /// Mirror of Go `walk.DefaultOptions()` — used by `where` (and by the Go
    /// oracle's noise / braid / focus loaders, which share these options).
    pub(crate) fn default_where() -> Self {
        Self {
            respect_gitignore: true,
            respect_ctxignore: true,
            extra_ignore: [".git/", "node_modules/", "dist/", "coverage/"]
                .iter()
                .map(ToString::to_string)
                .collect(),
        }
    }
}

/// Compiled ignorers for one walk, built once at `where_files` entry.
/// Mirrors Go `walk.New`: the root `.gitignore` (when present and respected)
/// is compiled together with the extra-ignore lines; otherwise the extra
/// lines alone; `.ctxignore` is compiled separately.
pub(crate) struct WalkIgnore {
    ignorer: Option<GitIgnore>,
    ctx_ignorer: Option<GitIgnore>,
}

impl WalkIgnore {
    pub(crate) fn new(root: &Path, opts: &WalkIgnoreOptions) -> Result<Self, String> {
        let mut ignorer = None;
        if opts.respect_gitignore {
            let gi_path = root.join(".gitignore");
            if gi_path.exists() {
                ignorer = Some(GitIgnore::from_file_and_lines(
                    &gi_path,
                    &opts.extra_ignore,
                )?);
            }
        }
        if ignorer.is_none() && !opts.extra_ignore.is_empty() {
            ignorer = Some(GitIgnore::from_lines(opts.extra_ignore.iter()));
        }
        let mut ctx_ignorer = None;
        if opts.respect_ctxignore {
            let ci_path = root.join(".ctxignore");
            if ci_path.exists() {
                ctx_ignorer = Some(GitIgnore::from_file(&ci_path)?);
            }
        }
        Ok(Self {
            ignorer,
            ctx_ignorer,
        })
    }

    /// Mirror of Go `walk.visit`'s ignore application: directories are checked
    /// with a trailing "/" AND as the bare relative path; files with the bare
    /// relative path only (both MatchesPath calls collapse to the same string).
    pub(crate) fn skips(&self, rel_slash: &str, is_dir: bool) -> bool {
        let check_path = if is_dir {
            std::borrow::Cow::Owned(format!("{rel_slash}/"))
        } else {
            std::borrow::Cow::Borrowed(rel_slash)
        };
        for ignorer in [&self.ignorer, &self.ctx_ignorer].into_iter().flatten() {
            if ignorer.matches_path(&check_path) || ignorer.matches_path(rel_slash) {
                return true;
            }
        }
        false
    }
}

pub(crate) fn where_files(root: &Path) -> Result<Vec<ctx_where::FileInput>, String> {
    where_files_with(root, &WalkIgnoreOptions::default_where())
}

pub(crate) fn where_files_with(
    root: &Path,
    opts: &WalkIgnoreOptions,
) -> Result<Vec<ctx_where::FileInput>, String> {
    let ignore = WalkIgnore::new(root, opts)?;
    // Two-phase (mirrors the pack walk): a cheap sequential pass enumerates the
    // surviving file paths (readdir + ignore pruning only), then the heavy
    // per-file work (read + tree-sitter symbol extraction) runs in parallel.
    // Each file maps to a self-contained `FileInput`, so collection order is
    // irrelevant — the `sort_by(path)` below restores the deterministic order,
    // keeping byte-parity with the sequential walk.
    let mut candidates = Vec::new();
    collect_where_paths(root, root, &ignore, &mut candidates)?;
    let mut out: Vec<ctx_where::FileInput> = candidates
        .par_iter()
        .filter_map(|path| build_where_input(root, path))
        .collect();
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// Enumerate the file paths under `dir` that survive ignore pruning, applying
/// the same rules the sequential walk did (a matching directory prunes the
/// whole subtree; a matching file is skipped). The per-file read + symbol
/// extraction is deferred to [`build_where_input`] so it can run in parallel.
pub(crate) fn collect_where_paths(
    root: &Path,
    dir: &Path,
    ignore: &WalkIgnore,
    out: &mut Vec<PathBuf>,
) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|err| format!("walk {}: {err}", dir.display()))? {
        let entry = entry.map_err(|err| err.to_string())?;
        let path = entry.path();
        let rel = path
            .strip_prefix(root)
            .map_err(|err| err.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        let is_dir = path.is_dir();
        // Go walk.visit: a matching directory prunes the whole subtree; a
        // matching file is skipped. Note dir-only patterns like "dist/" no
        // longer skip plain FILES named "dist" (the old hardcoded check did —
        // that was a divergence from the Go oracle).
        if ignore.skips(&rel, is_dir) {
            continue;
        }
        if is_dir {
            collect_where_paths(root, &path, ignore, out)?;
            continue;
        }
        out.push(path);
    }
    Ok(())
}

/// Read and parse a single candidate file into a `FileInput`, or `None` when it
/// cannot be read (mirroring the sequential walk's `let Ok(body) … else
/// continue`). Pure given `(root, path)`, so it is safe to call concurrently.
pub(crate) fn build_where_input(root: &Path, path: &Path) -> Option<ctx_where::FileInput> {
    let rel = path
        .strip_prefix(root)
        .ok()?
        .to_string_lossy()
        .replace('\\', "/");
    let body = std::fs::read_to_string(path).ok()?;
    let lines: Vec<String> = body.lines().map(ToString::to_string).collect();
    // Mirror Go: every consumer of this walk (where / noise / onboarding /
    // focus / braid) extracts symbols via symbols.New() — the tree-sitter
    // extractor (ported as ctx_symbols::extract). The previous regex
    // extractor diverged from the oracle on languages tree-sitter does
    // not cover (e.g. Rust files scored where symbol bonuses Go never
    // awards). Errors leave symbols empty, like Go's err == nil guard.
    let symbols = ctx_symbols::extract(path)
        .unwrap_or_default()
        .into_iter()
        .map(|sym| ctx_where::SymbolInput {
            name: sym.name,
            kind: sym.kind,
            line: i64::from(sym.line),
        })
        .collect();
    Some(ctx_where::FileInput {
        path: rel,
        is_dir: false,
        symbols,
        lines,
    })
}

pub(crate) fn extract_where_symbols(path: &str, lines: &[String]) -> Vec<ctx_where::SymbolInput> {
    let mut out = Vec::new();
    for (idx, line) in lines.iter().enumerate() {
        let line_no = (idx + 1) as i64;
        match Path::new(path).extension().and_then(OsStr::to_str) {
            Some("go") => extract_go_symbol(line, line_no, &mut out),
            Some("rs") => extract_rust_symbol(line, line_no, &mut out),
            Some("ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs") => {
                extract_js_symbol(line, line_no, &mut out)
            }
            Some("py") => extract_python_symbol(line, line_no, &mut out),
            _ => {}
        }
    }
    out
}

pub(crate) fn extract_go_symbol(line: &str, line_no: i64, out: &mut Vec<ctx_where::SymbolInput>) {
    let trimmed = line.trim_start();
    if let Some(rest) = trimmed.strip_prefix("func ") {
        if let Some(name) = rest
            .strip_prefix('(')
            .and_then(|receiver_tail| receiver_tail.split_once(')'))
            .and_then(|(_, after_receiver)| first_ident(after_receiver.trim_start()))
            .or_else(|| first_ident(rest))
        {
            out.push(symbol_input(name, "function", line_no));
        }
    } else if let Some(rest) = trimmed.strip_prefix("type ") {
        if let Some(name) = first_ident(rest) {
            out.push(symbol_input(name, "type", line_no));
        }
    }
}

pub(crate) fn extract_rust_symbol(line: &str, line_no: i64, out: &mut Vec<ctx_where::SymbolInput>) {
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix("pub ").unwrap_or(trimmed);
    for (prefix, kind) in [
        ("fn ", "function"),
        ("struct ", "type"),
        ("enum ", "type"),
        ("trait ", "interface"),
    ] {
        if let Some(tail) = rest.strip_prefix(prefix) {
            if let Some(name) = first_ident(tail) {
                out.push(symbol_input(name, kind, line_no));
            }
            return;
        }
    }
}

pub(crate) fn extract_js_symbol(line: &str, line_no: i64, out: &mut Vec<ctx_where::SymbolInput>) {
    let trimmed = line.trim_start();
    let rest = trimmed
        .strip_prefix("export default ")
        .or_else(|| trimmed.strip_prefix("export "))
        .unwrap_or(trimmed);
    for (prefix, kind) in [
        ("function ", "function"),
        ("class ", "class"),
        ("interface ", "interface"),
    ] {
        if let Some(tail) = rest.strip_prefix(prefix) {
            if let Some(name) = first_ident(tail) {
                out.push(symbol_input(name, kind, line_no));
            }
            return;
        }
    }
    for prefix in ["const ", "let ", "var "] {
        if let Some(tail) = rest.strip_prefix(prefix) {
            if let Some((name, after)) = first_ident_with_tail(tail) {
                if after.trim_start().starts_with('=') && after.contains("=>") {
                    out.push(symbol_input(name, "function", line_no));
                }
            }
            return;
        }
    }
}

pub(crate) fn extract_python_symbol(
    line: &str,
    line_no: i64,
    out: &mut Vec<ctx_where::SymbolInput>,
) {
    let trimmed = line.trim_start();
    if let Some(tail) = trimmed.strip_prefix("def ") {
        if let Some(name) = first_ident(tail) {
            out.push(symbol_input(name, "function", line_no));
        }
    } else if let Some(tail) = trimmed.strip_prefix("class ") {
        if let Some(name) = first_ident(tail) {
            out.push(symbol_input(name, "class", line_no));
        }
    }
}

pub(crate) fn symbol_input(name: &str, kind: &str, line: i64) -> ctx_where::SymbolInput {
    ctx_where::SymbolInput {
        name: name.to_string(),
        kind: kind.to_string(),
        line,
    }
}

pub(crate) fn first_ident(input: &str) -> Option<&str> {
    first_ident_with_tail(input).map(|(name, _)| name)
}

pub(crate) fn first_ident_with_tail(input: &str) -> Option<(&str, &str)> {
    let input = input.trim_start();
    let mut chars = input.char_indices();
    let (_, first) = chars.next()?;
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return None;
    }
    let mut end = first.len_utf8();
    for (idx, ch) in chars {
        if ch == '_' || ch.is_ascii_alphanumeric() {
            end = idx + ch.len_utf8();
        } else {
            return Some((&input[..end], &input[idx..]));
        }
    }
    Some((&input[..end], ""))
}

/// Mirror Go encoding/json's default HTML escaping: ampersand and angle
/// brackets become their `u0026` / `u003c` / `u003e` escape sequences.
/// Applied to the serialized text — those bytes can only occur inside JSON
/// string literals, so the blanket replacement is safe (same pattern as
/// ctx-mcp's response writer).
pub(crate) fn go_json_escape(body: &str) -> String {
    body.replace('&', "\\u0026")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
}

pub(crate) fn render_where(
    results: &[ctx_where::SearchResult],
    suggestions: &[ctx_where::Suggestion],
    args: &WhereArgs,
) -> Result<(), String> {
    match args.format.as_str() {
        "json" => {
            if args.explain {
                // Mirror Go's renderWhere JSON+explain envelope:
                // { synonyms_applied?, expanded_keywords?, results: [...] }
                // with per-result synonyms_applied / expanded_keywords cleared
                // from results[0] to avoid duplication (Go design doc Issue #17).
                #[derive(serde::Serialize)]
                struct JsonEnvelope {
                    #[serde(skip_serializing_if = "Option::is_none")]
                    synonyms_applied: Option<std::collections::BTreeMap<String, Vec<String>>>,
                    #[serde(skip_serializing_if = "Option::is_none")]
                    expanded_keywords: Option<Vec<Vec<String>>>,
                    // Go serializes a nil slice as null; use Option to mirror that.
                    results: Option<Vec<ctx_where::SearchResult>>,
                }
                // Extract top-level fields from results[0] before cloning.
                let sa_val = results
                    .first()
                    .and_then(|r| r.synonyms_applied.as_ref())
                    .filter(|m| !m.is_empty())
                    .cloned();
                let ek_val = results
                    .first()
                    .and_then(|r| r.expanded_keywords.as_ref())
                    .filter(|v| !v.is_empty())
                    .cloned();
                // Clone and clear per-result fields to avoid duplication.
                let mut owned: Vec<ctx_where::SearchResult> = results.to_vec();
                if let Some(first) = owned.first_mut() {
                    first.synonyms_applied = None;
                    first.expanded_keywords = None;
                }
                let envelope = JsonEnvelope {
                    synonyms_applied: sa_val,
                    expanded_keywords: ek_val,
                    // Mirror Go nil-slice → null: only Some when non-empty.
                    results: if owned.is_empty() { None } else { Some(owned) },
                };
                let body =
                    serde_json::to_string_pretty(&envelope).map_err(|err| err.to_string())?;
                println!("{}", go_json_escape(&body));
            } else if results.is_empty() {
                // Go's json.Encoder.Encode(nil_slice) emits "null\n".
                // An empty Rust Vec serializes to "[]" — use serde_json::Value::Null
                // to match the Go nil-slice wire format.
                serde_json::to_writer_pretty(io::stdout(), &serde_json::Value::Null)
                    .map_err(|err| err.to_string())?;
                println!();
            } else {
                let body = serde_json::to_string_pretty(results).map_err(|err| err.to_string())?;
                println!("{}", go_json_escape(&body));
            }
            Ok(())
        }
        "vimgrep" => {
            for result in results {
                for m in &result.matches {
                    println!("{}:{}:{}:{}", result.path, m.line, m.column, m.text);
                }
            }
            Ok(())
        }
        "default" | "" => {
            if args.plain {
                render_where_plain(results, suggestions, &args.query, args.explain);
            } else {
                render_where_default(results, suggestions, &args.query, args.explain);
            }
            Ok(())
        }
        other => Err(format!("unsupported where format: {other}")),
    }
}

/// Format the score breakdown bracket, e.g. "[symbol:10, splitname:6, content:3]".
/// Returns empty string when breakdown is None or all zero.
pub(crate) fn format_where_score_breakdown(result: &ctx_where::SearchResult) -> String {
    let Some(b) = result.score_breakdown.as_ref() else {
        return String::new();
    };
    let mut parts = Vec::new();
    if b.basename > 0 {
        parts.push(format!("basename:{}", b.basename));
    }
    if b.symbol > 0 {
        parts.push(format!("symbol:{}", b.symbol));
    }
    if b.splitname > 0 {
        parts.push(format!("splitname:{}", b.splitname));
    }
    if b.path > 0 {
        parts.push(format!("path:{}", b.path));
    }
    if b.content > 0 {
        parts.push(format!("content:{}", b.content));
    }
    if parts.is_empty() {
        return String::new();
    }
    parts.join(", ")
}

/// Build the --explain header for text formats (default / plain).
/// Mirrors Go's formatExplainHeader: shows synonyms_applied then expanded_keywords.
pub(crate) fn format_where_explain_header(results: &[ctx_where::SearchResult]) -> String {
    let Some(r) = results.first() else {
        return String::new();
    };
    let has_synonyms = r.synonyms_applied.as_ref().is_some_and(|m| !m.is_empty());
    let has_expanded = r.expanded_keywords.as_ref().is_some_and(|v| !v.is_empty());
    if !has_synonyms && !has_expanded {
        return String::new();
    }
    let mut out = String::new();
    if has_synonyms {
        out.push_str("Synonyms applied:\n");
        if let (Some(ek), Some(sa)) = (r.expanded_keywords.as_ref(), r.synonyms_applied.as_ref()) {
            for row in ek {
                if row.is_empty() {
                    continue;
                }
                let orig = &row[0];
                if let Some(syns) = sa.get(orig) {
                    if !syns.is_empty() {
                        out.push_str(&format!("  {} \u{2192} {}\n", orig, syns.join(", ")));
                    }
                }
            }
        }
    }
    if has_expanded {
        out.push_str("Expanded keyword sets:\n");
        if let Some(ek) = r.expanded_keywords.as_ref() {
            for row in ek {
                out.push_str(&format!("  - {}\n", row.join(" | ")));
            }
        }
    }
    out
}

pub(crate) fn render_where_default(
    results: &[ctx_where::SearchResult],
    suggestions: &[ctx_where::Suggestion],
    query: &str,
    explain: bool,
) {
    if explain {
        let header = format_where_explain_header(results);
        if !header.is_empty() {
            print!("{header}");
        }
    }
    if results.is_empty() {
        println!("No matches found for query: {query}");
        if !suggestions.is_empty() {
            let names: Vec<_> = suggestions.iter().map(|s| s.name.as_str()).collect();
            println!("Did you mean: {}?", names.join(", "));
        }
        return;
    }
    println!("Best matches\n");
    for (idx, result) in results.iter().enumerate() {
        println!("{}. {}", idx + 1, result.path);
        let breakdown = format_where_score_breakdown(result);
        if !breakdown.is_empty() {
            println!("   score {} [{}]", result.score, breakdown);
        }
        if result.reason.is_empty() {
            println!("   reason: matched query");
        } else {
            println!("   reason: {}", result.reason);
        }
        // Render context lines when present (matches Go's renderMatchWithContext).
        let has_context = result
            .matches
            .iter()
            .any(|m| !m.before.is_empty() || !m.after.is_empty());
        if has_context {
            for m in &result.matches {
                let start_line = m.line - m.before.len() as i64;
                for (i, line) in m.before.iter().enumerate() {
                    println!("    {}: {}", start_line + i as i64, line);
                }
                println!("  > {}: {}", m.line, m.text);
                for (i, line) in m.after.iter().enumerate() {
                    println!("    {}: {}", m.line + 1 + i as i64, line);
                }
            }
        }
        println!();
    }
}

pub(crate) fn render_where_plain(
    results: &[ctx_where::SearchResult],
    suggestions: &[ctx_where::Suggestion],
    query: &str,
    explain: bool,
) {
    // Mirror Go's renderWherePlain --explain: emit compact synonym lines.
    if explain {
        if let Some(r) = results.first() {
            if let (Some(ek), Some(sa)) =
                (r.expanded_keywords.as_ref(), r.synonyms_applied.as_ref())
            {
                for row in ek {
                    if row.is_empty() {
                        continue;
                    }
                    let orig = &row[0];
                    if let Some(syns) = sa.get(orig) {
                        if !syns.is_empty() {
                            println!("Synonyms: {} \u{2192} {}", orig, syns.join(", "));
                        }
                    }
                }
            }
        }
    }
    if results.is_empty() {
        if suggestions.is_empty() {
            println!("No matches found for query: {query}. Try different keywords.");
        } else {
            let names: Vec<_> = suggestions.iter().map(|s| s.name.as_str()).collect();
            println!(
                "No matches found for query: {query}. Did you mean {}? Otherwise try different keywords.",
                names.join(" or ")
            );
        }
        return;
    }
    let noun = if results.len() == 1 {
        "match"
    } else {
        "matches"
    };
    println!("Found {} {noun} for query: {query}.", results.len());
    for (idx, result) in results.iter().enumerate() {
        let reason = if result.reason.is_empty() {
            "matched query"
        } else {
            &result.reason
        };
        println!(
            "Result {} of {}: {}. score {}. reason: {}.",
            idx + 1,
            results.len(),
            result.path,
            result.score,
            reason
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Temp fixture dir that cleans itself up on drop.
    struct TempFixture(PathBuf);

    impl TempFixture {
        fn new(name: &str) -> Self {
            let dir = env::temp_dir().join(format!("ctx-where-walk-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
        fn write(&self, rel: &str, body: &str) {
            let path = self.0.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, body).unwrap();
        }
    }

    impl Drop for TempFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn paths(files: &[ctx_where::FileInput]) -> Vec<&str> {
        files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>()
    }

    #[test]
    fn where_files_skips_gitignored_dir() {
        let fx = TempFixture::new("gitignore");
        fx.write(".gitignore", "/target/\n*.log\n");
        fx.write("src/main.rs", "fn main() {}\n");
        fx.write("target/debug/build.rs", "fn ignored() {}\n");
        fx.write("debug.log", "noise\n");
        fx.write("node_modules/pkg/index.js", "module.exports = 1\n");

        let files = where_files(&fx.0).unwrap();
        assert_eq!(paths(&files), vec![".gitignore", "src/main.rs"]);
    }

    #[test]
    fn where_files_includes_file_named_dist() {
        // Go's "dist/" ExtraIgnore pattern is dir-only: a plain FILE named
        // "dist" must be included (the old hardcoded name check skipped it).
        let fx = TempFixture::new("dist-file");
        fx.write("dist", "plain file named dist\n");
        fx.write("dist-dir/app.js", "ignored only when named exactly dist\n");

        let files = where_files(&fx.0).unwrap();
        assert_eq!(paths(&files), vec!["dist", "dist-dir/app.js"]);
    }

    #[test]
    fn where_files_respects_ctxignore() {
        let fx = TempFixture::new("ctxignore");
        fx.write(".ctxignore", "generated/\n");
        fx.write("generated/big.json", "{}\n");
        fx.write("lib.rs", "pub fn lib() {}\n");

        let files = where_files(&fx.0).unwrap();
        assert_eq!(paths(&files), vec![".ctxignore", "lib.rs"]);

        // map-style options (respect_ctxignore=false) must keep generated/.
        let opts = WalkIgnoreOptions {
            respect_ctxignore: false,
            ..WalkIgnoreOptions::default_where()
        };
        let files = where_files_with(&fx.0, &opts).unwrap();
        assert_eq!(
            paths(&files),
            vec![".ctxignore", "generated/big.json", "lib.rs"]
        );
    }

    #[test]
    fn where_files_gitignore_negation() {
        let fx = TempFixture::new("negation");
        fx.write(".gitignore", "*.gen.ts\n!keep.gen.ts\n");
        fx.write("a.gen.ts", "export {}\n");
        fx.write("keep.gen.ts", "export {}\n");

        let files = where_files(&fx.0).unwrap();
        assert_eq!(paths(&files), vec![".gitignore", "keep.gen.ts"]);
    }
}
