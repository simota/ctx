use crate::*;
use serde_json::{json, Map, Value};

pub(crate) fn tools() -> Vec<Value> {
    vec![
        tool(
            "ctx_pack",
            "Bundle a directory of source files into a single LLM-ready context pack. Output is concatenated file contents in the requested format. Use this when the caller needs the full text of many files for prompt grounding. For a single file, prefer ctx_skim. For a symbol neighbourhood, prefer ctx_focus. Typical chain: ctx_where → ctx_pack (or ctx_focus for targeted reads).",
            object_schema(
                vec![
                    ("budget", describe(int_schema(0, MAX_BUDGET), "Target token budget (0..1000000). Defaults to 50000. Recommended: 20000–80000.")),
                    ("changed", describe(bool_schema(), "When true, restrict to files changed in the working tree.")),
                    ("explain", describe(bool_schema(), "When true, include selection-reason annotations in the pack.")),
                    ("format", describe(enum_schema(&["markdown", "xml", "json", "plain"]), "Output format. Defaults to 'markdown'.")),
                    ("goal", describe(str_schema(MAX_GOAL_LEN, None, &["implement rate limiter", "audit log writer", "config loader"]), "Goal annotation embedded in the pack header (free-form text).")),
                    ("path", describe(str_schema(MAX_PATH_LEN, Some("."), &[]), "Directory to walk. Defaults to '.'")),
                ],
                &["path"],
            ),
        ),
        tool(
            "ctx_where",
            "Locate files, symbols, and matching lines by keyword. Returns ranked paths with brief reasons. Faster and lighter than ctx_pack — use this when the caller needs to *find* code rather than read it. Use to surface relevant files for a goal; then pass top hits to ctx_focus or ctx_pack. Typical chain: ctx_where → ctx_focus → ctx_pack.",
            object_schema(
                vec![
                    ("format", describe(enum_schema(&["default", "vimgrep", "json"]), "Output format. Defaults to 'default'.")),
                    ("limit", describe(int_schema(0, MAX_LIMIT), "Max number of results (0..1000). Defaults to 10. Recommended: 5–20.")),
                    ("path", describe(str_schema(MAX_PATH_LEN, Some("."), &[]), "Search root. Defaults to '.'")),
                    ("query", describe(str_schema(MAX_QUERY_LEN, None, &["rate limiter", "config loader", "audit log"]), "Free-form query. Space-separated terms are AND-combined.")),
                ],
                &["path", "query"],
            ),
        ),
        tool(
            "ctx_budget",
            "Plan a token budget across a directory: returns which files fit and which are skipped under the given budget, as JSON. Use before ctx_pack to preview fit/skip without reading file contents.",
            object_schema(
                vec![
                    ("budget", describe(int_schema(0, MAX_BUDGET), "Token budget to allocate (0..1000000). Recommended: 20000–80000.")),
                    ("path", describe(str_schema(MAX_PATH_LEN, Some("."), &[]), "Directory to plan. Defaults to '.'")),
                ],
                &["path", "budget"],
            ),
        ),
        tool(
            "ctx_symbols",
            "Extract public symbols (functions, types, methods) from a directory's source files. Returns JSON keyed by file path. Useful when the caller wants an API outline without the file bodies. Lightweight alternative to ctx_tree(with_symbols=true) when only the symbol map matters.",
            object_schema(
                vec![
                    ("cursor", describe(str_schema(MAX_CURSOR_LEN, None, &[]), "Opaque pagination cursor returned by a previous call. Omit for the first page.")),
                    ("page_size", describe(int_schema(0, MAX_PAGE_SIZE), "Max files per page (1..500). Omit or 0 for all files in one call.")),
                    ("path", describe(str_schema(MAX_PATH_LEN, Some("."), &[]), "Directory to scan. Defaults to '.'")),
                ],
                &["path"],
            ),
        ),
        tool(
            "ctx_skim",
            "Compress a single file to fit a token budget using tiered fallback (full → api+doc → signatures → outline). Returns a header line followed by the compressed body. Prefer this over ctx_pack when only one file matters and budget is tight. Typical chain: ctx_where → ctx_skim (quick API read) → ctx_focus (deep dive).",
            object_schema(
                vec![
                    ("budget", describe(int_schema(0, MAX_BUDGET), "Target budget in --unit (0..1000000). Defaults to 1000. Recommended: 500–4000.")),
                    ("lang", describe(str_schema(MAX_LANG_LEN, None, &["go", "ts", "python", "rust", "java"]), "Language hint for symbol extraction. Defaults to auto-detect.")),
                    ("path", describe(str_schema(MAX_PATH_LEN, None, &[]), "File path to skim (required).")),
                    ("tier", describe(enum_schema(&["full", "api+doc", "signatures", "outline"]), "Force a specific tier and skip degradation. Omit to let the budget drive tier selection.")),
                    ("unit", describe(enum_schema(&["tokens", "chars"]), "Budget unit. Defaults to 'tokens'.")),
                ],
                &["path"],
            ),
        ),
        tool(
            "ctx_digest",
            "Summarise recent repository activity over a time window: commits, authors, file churn, net token/symbol delta, and hot files. Requires a Git repository. Use with since='7d' for recent changes; since='1mo' for a sprint retrospective. Typical chain: ctx_digest → ctx_skim (hot files) → ctx_focus (changed symbols).",
            object_schema(
                vec![
                    ("cursor", describe(str_schema(MAX_CURSOR_LEN, None, &[]), "Opaque pagination cursor returned by a previous call. Omit for the first page.")),
                    ("format", describe(enum_schema(&["markdown", "json", "plain"]), "Output format. Defaults to 'markdown'.")),
                    ("page_size", describe(int_schema(0, MAX_PAGE_SIZE), "Max hot files per page (1..500). Omit or 0 to return all up to 'top'.")),
                    ("path", describe(str_schema(MAX_PATH_LEN, Some("."), &[]), "Repository root. Defaults to '.'")),
                    ("since", describe(str_schema(MAX_SINCE_LEN, Some("7d"), &["7d", "24h", "2w", "1mo"]), "Lookback window. Defaults to '7d'.")),
                    ("top", describe(int_schema(0, MAX_TOP), "Max number of hot files in output (0..200). Defaults to 10.")),
                ],
                &[],
            ),
        ),
        tool(
            "ctx_focus",
            "Build a symbol-anchored mini-pack with one or two hops of expansion. anchor resolves in order: exact symbol name → file basename → repo-relative path. Use after ctx_where when you already know which symbol or file to expand. Pass a symbol name (e.g. 'ServeHTTP') for code-level focus, or a path (e.g. 'internal/mcp/server.go') for file-level focus. Typical chain: ctx_where → ctx_focus → ctx_pack.",
            object_schema(
                vec![
                    ("anchor", describe(str_schema(MAX_ANCHOR_LEN, None, &["ServeHTTP", "internal/mcp/server.go", "runPack"]), "Symbol name, file basename, or repo-relative path to anchor on.")),
                    ("budget", describe(int_schema(0, MAX_BUDGET), "Token budget (0..1000000). Defaults to 8000. Recommended: 4000–16000.")),
                    ("format", describe(enum_schema(&["markdown", "xml", "json", "plain"]), "Output format. Defaults to 'markdown'.")),
                    ("hops", describe(int_schema(0, MAX_HOPS), "Expansion hops (0..2). Defaults to 1. Use 2 to pull in transitive callers.")),
                    ("path", describe(str_schema(MAX_PATH_LEN, Some("."), &[]), "Repository root. Defaults to '.'")),
                ],
                &["anchor"],
            ),
        ),
        tool(
            "ctx_roots_list",
            "List ctx project roots registered in ~/.ctx/roots.toml. Returns each entry's name, absolute path, and last_opened_at, plus a marker on the root currently served by this MCP server. Use this when the user mentions another project, or when you need to know which other repositories are registered alongside the current one so you can suggest the right `ctx roots open <name>` command. Read-only.",
            object_schema(Vec::new(), &[]),
        ),
        tool(
            "ctx_tree",
            "Return the directory tree as JSON with token, size, line, Git-status, and symbol metadata per file. Use this when the caller needs structural awareness of the codebase before deciding which files to read. For symbol-only views, prefer ctx_symbols (faster, no tree overhead). Typical chain: ctx_tree → ctx_where → ctx_focus.",
            object_schema(
                vec![
                    ("cursor", describe(str_schema(MAX_CURSOR_LEN, None, &[]), "Opaque pagination cursor returned by a previous call. Omit for the first page.")),
                    ("depth", describe(int_schema(0, MAX_DEPTH), "Max directory depth (0 = unlimited, capped at 16). Defaults to 0. Recommended: 2–4 for large repos.")),
                    ("page_size", describe(int_schema(0, MAX_PAGE_SIZE), "Max flattened entries per page (1..500). Omit or 0 to return the full tree.")),
                    ("path", describe(str_schema(MAX_PATH_LEN, Some("."), &[]), "Directory to walk. Defaults to '.'")),
                    ("since", describe(str_schema(MAX_SINCE_LEN, None, &["7d", "24h", "2w", "2026-01-01"]), "Include only files touched since this duration or date (e.g. '7d', '2026-01-01'). Omit to return all files.")),
                    ("until", describe(str_schema(MAX_SINCE_LEN, None, &["2026-04-01"]), "Include only files touched before this duration or date. Omit to return all files.")),
                    ("use_mtime", describe(bool_schema(), "Use file modification time instead of git commit time for since/until filtering. Defaults to false.")),
                    ("with_git", describe(bool_schema(), "Include Git status per file. Defaults to true.")),
                    ("with_symbols", describe(bool_schema(), "Include extracted symbols per file. Defaults to false.")),
                    ("with_tokens", describe(bool_schema(), "Include token estimates per file. Defaults to true.")),
                ],
                &["path"],
            ),
        ),
    ]
}

pub(crate) fn tool(name: &'static str, description: &'static str, input_schema: Value) -> Value {
    let mut m = Map::new();
    m.insert("description".to_string(), json!(description));
    m.insert("inputSchema".to_string(), input_schema);
    m.insert("name".to_string(), json!(name));
    Value::Object(m)
}

pub(crate) fn object_schema(props: Vec<(&'static str, Value)>, required: &[&'static str]) -> Value {
    let mut properties = Map::new();
    for (name, schema) in props {
        properties.insert(name.to_string(), schema);
    }

    let mut m = Map::new();
    m.insert("additionalProperties".to_string(), json!(false));
    m.insert("properties".to_string(), Value::Object(properties));
    if !required.is_empty() {
        m.insert("required".to_string(), json!(required));
    }
    m.insert("type".to_string(), json!("object"));
    Value::Object(m)
}

pub(crate) fn str_schema(
    max_len: usize,
    default: Option<&'static str>,
    examples: &[&'static str],
) -> Value {
    let mut m = Map::new();
    if let Some(default) = default {
        m.insert("default".to_string(), json!(default));
    }
    if !examples.is_empty() {
        m.insert("examples".to_string(), json!(examples));
    }
    m.insert("maxLength".to_string(), json!(max_len));
    m.insert("type".to_string(), json!("string"));
    Value::Object(m)
}

pub(crate) fn int_schema(min: i64, max: i64) -> Value {
    let mut m = Map::new();
    m.insert("maximum".to_string(), json!(max));
    m.insert("minimum".to_string(), json!(min));
    m.insert("type".to_string(), json!("integer"));
    Value::Object(m)
}

pub(crate) fn bool_schema() -> Value {
    let mut m = Map::new();
    m.insert("type".to_string(), json!("boolean"));
    Value::Object(m)
}

pub(crate) fn enum_schema(values: &[&'static str]) -> Value {
    let mut m = Map::new();
    m.insert("enum".to_string(), json!(values));
    m.insert("type".to_string(), json!("string"));
    Value::Object(m)
}

pub(crate) fn describe(mut schema: Value, description: &'static str) -> Value {
    if let Value::Object(m) = &mut schema {
        let mut entries: Vec<(String, Value)> = std::mem::take(m).into_iter().collect();
        entries.push(("description".to_string(), json!(description)));
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        for (key, value) in entries {
            m.insert(key, value);
        }
    }
    schema
}
