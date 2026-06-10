use std::path::Path;
use std::time::SystemTime;

use serde::Serialize;

#[derive(Serialize)]
pub(crate) struct SymbolsJsonDoc {
    // Go's `jsonSymbols.Files` is a nil slice when no file has symbols, which
    // `encoding/json` renders as `null` (not `[]`). Mirror that: empty → null.
    pub(crate) files: Option<Vec<SymbolsJsonFile>>,
}

#[derive(Serialize)]
pub(crate) struct SymbolsJsonFile {
    pub(crate) path: String,
    pub(crate) symbols: Vec<SymbolsJsonEntry>,
}

#[derive(Serialize)]
pub(crate) struct SymbolsJsonEntry {
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) line: i32,
}

/// Collect every non-directory file's relative path from a built tree node
/// (depth-limited by `build_root_tree`). The root node's own path is "."; only
/// leaf files are collected.
pub(crate) fn collect_tree_file_paths(node: &JsonTreeNode, out: &mut Vec<String>) {
    if node.is_dir {
        for child in &node.children {
            collect_tree_file_paths(child, out);
        }
    } else {
        out.push(node.path.clone());
    }
}

/// Mirrors Go's `render.jsonTreeNode`. Field names + order match the Go struct
/// tags exactly (`path`, `isDir`, `metadata`, `children` with omitempty).
#[derive(Serialize)]
pub(crate) struct JsonTreeNode {
    pub(crate) path: String,
    #[serde(rename = "isDir")]
    pub(crate) is_dir: bool,
    pub(crate) metadata: JsonMetadata,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) children: Vec<JsonTreeNode>,
}

/// Mirrors Go's `render.jsonMetadata`. Field order + omitempty match the Go
/// struct tags: `size`, `tokens`, `lines` always; `chars`, `role`, `gitStatus`,
/// `symbols` omitted when empty/zero.
#[derive(Serialize)]
pub(crate) struct JsonMetadata {
    pub(crate) size: i64,
    pub(crate) tokens: i64,
    pub(crate) lines: i64,
    #[serde(rename = "chars", skip_serializing_if = "is_zero_i64_meta")]
    pub(crate) chars: i64,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(crate) role: String,
    #[serde(rename = "gitStatus", skip_serializing_if = "String::is_empty")]
    pub(crate) git_status: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) symbols: Vec<SymbolsJsonEntry>,
}

pub(crate) fn is_zero_i64_meta(value: &i64) -> bool {
    *value == 0
}

/// Walk `root` (reproducing Go's `walk.New`/`Walk` for the default root
/// invocation), enrich each node with size/tokens/lines/chars/role/symbols,
/// aggregate directory totals, then emit the JSONTree document. Output is
/// byte-identical to Go's `render.JSONTree`: 2-space indent + trailing newline,
/// children in lexical (os.ReadDir) order.
pub(crate) fn render_root_json_tree(root: &Path, opts: &TreeBuildOpts) -> Result<(), String> {
    let node = build_root_tree(root, opts)?.ok_or_else(|| "root is ignored".to_string())?;
    let mut out = serde_json::to_string_pretty(&node).map_err(|e| e.to_string())?;
    out.push('\n');
    print!("{out}");
    Ok(())
}

/// Options threaded through the native root walk (mirrors `walk.Options`).
///
/// `max_depth` mirrors Go's `walk.Options.MaxDepth` (0 = unlimited): once
/// `depth >= max_depth` for a directory, its children are not walked (the dir
/// node is still emitted, with zero aggregated metadata — exactly like Go's
/// `visit` early-return after the MaxDepth check).
///
/// `since`/`until` mirror the time-filter: non-directory files whose effective
/// modification time falls outside the window are dropped, and directories left
/// with no descendant files are pruned (mirrors `pruneEmptyDirs`). On this
/// non-git fixture (and whenever git is unavailable / `--use-mtime`), the
/// effective time is the file mtime — matching Go's `fileTime` fallback.
#[derive(Clone, Copy, Default)]
pub(crate) struct TreeBuildOpts {
    pub(crate) max_depth: i64,
    pub(crate) since: Option<SystemTime>,
    pub(crate) until: Option<SystemTime>,
}

impl TreeBuildOpts {
    pub(crate) fn time_filter_active(&self) -> bool {
        self.since.is_some() || self.until.is_some()
    }
}

/// Build a JsonTreeNode for `path` relative to `root` at depth 0.
/// Applies the depth limit and time-filter from `opts`, then prunes empty
/// directories when a time-filter is active (mirrors `walk.Walk`).
pub(crate) fn build_root_tree(
    root: &Path,
    opts: &TreeBuildOpts,
) -> Result<Option<JsonTreeNode>, String> {
    let mut node = build_json_tree_node(root, root, 0, opts)?;
    if opts.time_filter_active() {
        if let Some(ref mut n) = node {
            prune_empty_dirs(n);
        }
    }
    Ok(node)
}

/// Mirrors `walk.pruneEmptyDirs`: drop child directories with no descendant
/// files (in place, preserving order). The root node is always kept. Returns
/// whether the node itself should be retained by its caller.
pub(crate) fn prune_empty_dirs(node: &mut JsonTreeNode) -> bool {
    if !node.is_dir {
        return true;
    }
    node.children.retain_mut(prune_empty_dirs);
    !node.children.is_empty()
}

/// Build a JsonTreeNode for `path` relative to `root`. Returns `Ok(None)` for
/// entries that are skipped by the ExtraIgnore patterns (mirrors Go's walker
/// returning `nil`) or filtered out by the time-window. Directories aggregate
/// size/tokens/chars from children; `lines` stays 0 for directories (matching
/// Go's `aggregateDirectories`). `depth` is the current recursion depth (root =
/// 0); `opts.max_depth` halts recursion into directories at the limit.
pub(crate) fn build_json_tree_node(
    root: &Path,
    path: &Path,
    depth: i64,
    opts: &TreeBuildOpts,
) -> Result<Option<JsonTreeNode>, String> {
    let rel = if path == root {
        ".".to_string()
    } else {
        path.strip_prefix(root)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/")
    };

    // ExtraIgnore: ".git/", "node_modules/", "dist/", "coverage/", "target/"
    // (plus the default config patterns *.lock / dist / coverage /
    // node_modules / .git). `target/` is build output and can be huge enough to
    // make the default root view appear silent while it is still walking.
    // The fixture has no .gitignore, so Go compiles the patterns from
    // cfg.Ignore.Patterns. We skip the same basenames.
    if rel != "." {
        let base = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if json_tree_should_skip(&base) {
            return Ok(None);
        }
    }

    let meta = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    let is_dir = meta.is_dir();

    // Time-filter: drop non-directory files outside the [since, until] window.
    // Effective time is the file mtime (Go falls back to mtime when git is
    // unavailable / --use-mtime, which is the case for this non-git fixture).
    if !is_dir && opts.time_filter_active() {
        let modified = meta.modified().map_err(|e| e.to_string())?;
        if let Some(since) = opts.since {
            if modified < since {
                return Ok(None);
            }
        }
        if let Some(until) = opts.until {
            if modified > until {
                return Ok(None);
            }
        }
    }

    if is_dir {
        let mut children = Vec::new();
        // MaxDepth (0 = unlimited): once depth >= max_depth, do not walk
        // children — the directory node is still emitted (matching Go's
        // `visit` early-return after the MaxDepth check, yielding zero
        // aggregated metadata).
        if opts.max_depth <= 0 || depth < opts.max_depth {
            let mut entries: Vec<_> = std::fs::read_dir(path)
                .map_err(|e| e.to_string())?
                .flatten()
                .collect();
            // os.ReadDir returns entries sorted by filename.
            entries.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
            for entry in entries {
                if let Some(child) = build_json_tree_node(root, &entry.path(), depth + 1, opts)? {
                    children.push(child);
                }
            }
        }
        // aggregateDirectories: size/tokens/chars are the sum of children.
        let size: i64 = children.iter().map(|c| c.metadata.size).sum();
        let tokens: i64 = children.iter().map(|c| c.metadata.tokens).sum();
        let chars: i64 = children.iter().map(|c| c.metadata.chars).sum();
        return Ok(Some(JsonTreeNode {
            path: rel,
            is_dir: true,
            metadata: JsonMetadata {
                size,
                tokens,
                lines: 0,
                chars,
                role: String::new(),
                git_status: String::new(),
                symbols: Vec::new(),
            },
            children,
        }));
    }

    // File node.
    let size = meta.len() as i64;
    let (lines, chars) = count_text_stats(path);
    let role = infer_json_role(&rel);
    let tokens = match ctx_tokens::count_file(&path.to_string_lossy()) {
        Ok(n) => n,
        Err(_) => ctx_tokens::estimate_by_size(size),
    };
    // Display.Symbols defaults to true and --symbols is not explicitly set, so
    // Go runs symbol extraction and embeds the result (rendered via JSONTree).
    let symbols = ctx_symbols::extract(path)
        .map(|syms| {
            syms.into_iter()
                .map(|s| SymbolsJsonEntry {
                    name: s.name,
                    kind: s.kind,
                    line: s.line,
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(Some(JsonTreeNode {
        path: rel,
        is_dir: false,
        metadata: JsonMetadata {
            size,
            tokens,
            lines,
            chars,
            role,
            git_status: String::new(),
            symbols,
        },
        children: Vec::new(),
    }))
}

/// ExtraIgnore basenames from Go's default config patterns (node_modules/dist/
/// coverage/.git). `*.lock` is also a default pattern; honored via suffix.
pub(crate) fn json_tree_should_skip(name: &str) -> bool {
    matches!(
        name,
        ".git" | "node_modules" | "dist" | "coverage" | "target"
    ) || name.ends_with(".lock")
}

/// Mirrors Go `walk.countTextStats`: returns (lines, chars). Binary or
/// non-UTF-8 files (NUL byte or invalid UTF-8 in the first 512 bytes) yield
/// (0, 0). `chars` is the UTF-8 rune count of the whole file.
pub(crate) fn count_text_stats(path: &Path) -> (i64, i64) {
    let Ok(data) = std::fs::read(path) else {
        return (0, 0);
    };
    if data.is_empty() {
        return (0, 0);
    }
    let header = &data[..data.len().min(512)];
    if header.contains(&0) || std::str::from_utf8(header).is_err() {
        return (0, 0);
    }
    let mut lines = data.iter().filter(|&&b| b == b'\n').count() as i64;
    if *data.last().unwrap() != b'\n' {
        lines += 1;
    }
    let chars = String::from_utf8_lossy(&data).chars().count() as i64;
    (lines, chars)
}

/// Mirrors Go `walk.inferRole`. Operates on the forward-slash relative path.
pub(crate) fn infer_json_role(rel_slash: &str) -> String {
    let base = rel_slash.rsplit('/').next().unwrap_or(rel_slash);
    let lower_path = rel_slash.to_ascii_lowercase();
    let lower_base = base.to_ascii_lowercase();
    // Go uses filepath.Ext which includes the leading dot (e.g. ".go").
    let ext = match lower_base.rfind('.') {
        Some(idx) => &lower_base[idx..],
        None => "",
    };

    if lower_path.starts_with("tests/")
        || lower_path.contains("/tests/")
        || lower_base.ends_with("_test.go")
        || json_is_dotted_test_name(&lower_base)
    {
        return "test".to_string();
    }
    if ext == ".md" || lower_base.starts_with("license") || lower_base.starts_with("readme") {
        return "doc".to_string();
    }
    if json_is_config_file(&lower_base, ext) {
        return "config".to_string();
    }
    if base == "main.ts"
        || base == "main.go"
        || base == "main.py"
        || base == "index.ts"
        || base == "index.tsx"
        || base == "index.js"
        || (rel_slash.starts_with("cmd/") && rel_slash.ends_with("/main.go"))
    {
        return "entry".to_string();
    }
    if base.contains("router") || base.contains("route") || base.contains("Router") {
        return "route".to_string();
    }
    if json_is_core_extension(ext) {
        return "core".to_string();
    }
    String::new()
}

pub(crate) fn json_is_dotted_test_name(base: &str) -> bool {
    [".test.ts", ".test.tsx", ".test.js", ".test.go", ".test.py"]
        .iter()
        .any(|suffix| base.ends_with(suffix))
}

pub(crate) fn json_is_config_file(base: &str, ext: &str) -> bool {
    matches!(
        base,
        "package.json" | "go.mod" | "cargo.toml" | "pyproject.toml" | "dockerfile" | "makefile"
    ) || matches!(ext, ".toml" | ".yaml" | ".yml")
}

pub(crate) fn json_is_core_extension(ext: &str) -> bool {
    matches!(ext, ".ts" | ".tsx" | ".js" | ".go" | ".py" | ".rs")
}
