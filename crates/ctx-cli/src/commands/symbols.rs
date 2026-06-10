use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::common::*;
use crate::tree::*;

/// Native `ctx [PATH] --symbols --json` — emit the flat symbols-JSON
/// document (mirrors render.JSONSymbols) using the native tree-sitter
/// extractor in ctx-symbols. Returns `None` (→ delegate to Go) for any
/// invocation outside the narrowly-supported native shape, so behavior is
/// preserved for flags whose walk semantics we do not reproduce here.
pub(crate) fn run_symbols_command(args: &[OsString]) -> Option<ExitCode> {
    // Root invocation only: recognized subcommands are dispatched earlier in
    // try_run_native, so any leading non-flag token here is the PATH
    // positional. We parse the (small) flag surface below and bail to Go for
    // anything outside the natively-supported symbols-JSON shape.
    let mut want_symbols = false;
    let mut want_json = false;
    let mut path: Option<String> = None;
    // `max_depth` mirrors Go's `walk.Options.MaxDepth` (0 = unlimited). The
    // symbols walk halts recursion at the limit just like the root tree walk,
    // so deep files are excluded (e.g. `--depth 1` on a corpus whose only files
    // live in subdirectories yields no symbol files → `{"files": null}`).
    let mut max_depth: i64 = 0;

    let mut iter = args.iter().peekable();
    while let Some(arg) = iter.next() {
        let s = arg.to_string_lossy();
        match s.as_ref() {
            "--symbols" | "--symbols=true" => want_symbols = true,
            "--json" | "--json=true" => want_json = true,
            // Walk-affecting / output-mode flags we do NOT reproduce here.
            "--symbols=false" | "--json=false" | "--tui" | "--plain" => return None,
            // `--depth N` / `-L N` (cobra-style; also `--depth=N`).
            "--depth" | "-L" => {
                let value = iter.next()?.to_string_lossy().parse::<i64>().ok()?;
                max_depth = value;
            }
            _ if s.starts_with("--depth=") => {
                max_depth = s["--depth=".len()..].parse::<i64>().ok()?;
            }
            _ if s.starts_with("-L=") => {
                max_depth = s["-L=".len()..].parse::<i64>().ok()?;
            }
            _ if s.starts_with("--since")
                || s.starts_with("--until")
                || s.starts_with("--budget")
                || s.starts_with("--plan")
                || s.starts_with("--use-mtime")
                || s.starts_with("--git") && s != "--git=false"
                || s.starts_with("--unit") =>
            {
                return None;
            }
            // Accept --git=false (no effect on symbols-JSON, which omits git).
            "--git=false" => {}
            _ if is_option(arg) => return None, // unknown flag → not native here.
            _ => {
                if path.is_some() {
                    return None; // multiple positionals.
                }
                path = Some(s.to_string());
            }
        }
    }

    if !(want_symbols && want_json) {
        return None;
    }

    let root = PathBuf::from(path.as_deref().unwrap_or("."));
    match render_symbols_json(&root, max_depth) {
        Ok(()) => Some(ExitCode::SUCCESS),
        Err(_) => None,
    }
}

/// Walk `root` (reusing the proven native walker) and emit the
/// symbols-JSON document with native tree-sitter extraction. Output is
/// byte-identical to Go's `render.JSONSymbols`: `{"files":[...]}` with
/// 2-space indent + trailing newline, files in lexical path order, only
/// files that have ≥1 symbol.
pub(crate) fn render_symbols_json(root: &Path, max_depth: i64) -> Result<(), String> {
    // Build the same depth-limited walk tree as the root tree renderer
    // (`build_root_tree`), so `--depth N` excludes files deeper than the limit
    // exactly like Go's `walk` (MaxDepth halts recursion at `depth >= N`). Go's
    // `render.JSONSymbols` then flattens that file set.
    let opts = TreeBuildOpts {
        max_depth,
        ..TreeBuildOpts::default()
    };
    let mut files: Vec<String> = Vec::new();
    if let Some(node) = build_root_tree(root, &opts)? {
        collect_tree_file_paths(&node, &mut files);
    }
    files.sort();

    let mut entries: Vec<SymbolsJsonFile> = Vec::new();
    for rel in &files {
        let abs = root.join(rel);
        let syms = ctx_symbols::extract(&abs).map_err(|e| e.to_string())?;
        if syms.is_empty() {
            continue;
        }
        entries.push(SymbolsJsonFile {
            path: rel.clone(),
            symbols: syms
                .into_iter()
                .map(|s| SymbolsJsonEntry {
                    name: s.name,
                    kind: s.kind,
                    line: s.line,
                })
                .collect(),
        });
    }
    let doc = SymbolsJsonDoc {
        files: (!entries.is_empty()).then_some(entries),
    };
    // Go uses json.Encoder with SetIndent("", "  ") which appends a newline.
    let mut out = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?;
    out.push('\n');
    print!("{out}");
    Ok(())
}
