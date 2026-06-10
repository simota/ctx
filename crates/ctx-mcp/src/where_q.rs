use std::ffi::OsStr;
use std::path::Path;

pub(crate) fn collect_where_files(root: &Path) -> Result<Vec<ctx_where::FileInput>, String> {
    let mut out = Vec::new();
    collect_where_files_inner(root, root, &mut out)?;
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

pub(crate) fn collect_where_files_inner(
    root: &Path,
    dir: &Path,
    out: &mut Vec<ctx_where::FileInput>,
) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|err| format!("walk {}: {err}", dir.display()))? {
        let entry = entry.map_err(|err| err.to_string())?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || name == "node_modules" || name == "dist" || name == "coverage" {
            continue;
        }
        if path.is_dir() {
            collect_where_files_inner(root, &path, out)?;
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .map_err(|err| err.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        let Ok(body) = std::fs::read_to_string(&path) else {
            continue;
        };
        let lines: Vec<String> = body.lines().map(ToString::to_string).collect();
        let symbols = extract_where_symbols(&rel, &lines);
        out.push(ctx_where::FileInput {
            path: rel,
            is_dir: false,
            symbols,
            lines,
        });
    }
    Ok(())
}

pub(crate) fn extract_where_symbols(path: &str, lines: &[String]) -> Vec<ctx_where::SymbolInput> {
    let mut out = Vec::new();
    let ext = Path::new(path)
        .extension()
        .and_then(OsStr::to_str)
        .unwrap_or("");
    for (idx, line) in lines.iter().enumerate() {
        let line_no = (idx + 1) as i64;
        match ext {
            "go" => extract_prefixed_symbol(
                line,
                line_no,
                &mut out,
                &[("func ", "function"), ("type ", "type")],
            ),
            "rs" => extract_prefixed_symbol(
                line.trim_start().strip_prefix("pub ").unwrap_or(line),
                line_no,
                &mut out,
                &[
                    ("fn ", "function"),
                    ("struct ", "type"),
                    ("enum ", "type"),
                    ("trait ", "interface"),
                ],
            ),
            "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => extract_prefixed_symbol(
                line.trim_start().strip_prefix("export ").unwrap_or(line),
                line_no,
                &mut out,
                &[
                    ("function ", "function"),
                    ("class ", "class"),
                    ("interface ", "interface"),
                ],
            ),
            "py" => extract_prefixed_symbol(
                line,
                line_no,
                &mut out,
                &[("def ", "function"), ("class ", "class")],
            ),
            _ => {}
        }
    }
    out
}

pub(crate) fn extract_prefixed_symbol(
    line: &str,
    line_no: i64,
    out: &mut Vec<ctx_where::SymbolInput>,
    prefixes: &[(&str, &str)],
) {
    let trimmed = line.trim_start();
    for (prefix, kind) in prefixes {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            let name = if *prefix == "func " {
                rest.strip_prefix(')')
                    .and_then(|_| None)
                    .or_else(|| {
                        rest.strip_prefix('(')
                            .and_then(|tail| tail.split_once(')'))
                            .and_then(|(_, after)| first_ident(after.trim_start()))
                    })
                    .or_else(|| first_ident(rest))
            } else {
                first_ident(rest)
            };
            if let Some(name) = name {
                out.push(ctx_where::SymbolInput {
                    name: name.to_string(),
                    kind: (*kind).to_string(),
                    line: line_no,
                });
            }
            return;
        }
    }
}

pub(crate) fn first_ident(input: &str) -> Option<&str> {
    let input = input.trim_start();
    let mut chars = input.char_indices();
    let (_, first) = chars.next()?;
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return Option::None;
    }
    for (idx, ch) in chars {
        if !(ch == '_' || ch.is_ascii_alphanumeric()) {
            return Some(&input[..idx]);
        }
    }
    Some(input)
}

pub(crate) fn format_where(
    results: &[ctx_where::SearchResult],
    format: &str,
) -> Result<String, String> {
    let mut out = String::new();
    match format {
        "json" => {
            out = serde_json::to_string_pretty(results).map_err(|err| err.to_string())?;
            out.push('\n');
        }
        "vimgrep" => {
            for result in results {
                for m in &result.matches {
                    out.push_str(&format!(
                        "{}:{}:{}:{} | {}\n",
                        result.path, m.line, m.column, m.text, m.kind
                    ));
                }
            }
        }
        "default" | "" => {
            out.push_str("Best matches\n\n");
            for (idx, result) in results.iter().enumerate() {
                let (line, symbol) = where_top_match(result);
                if line > 0 {
                    if symbol.is_empty() {
                        out.push_str(&format!("{}. {}:{}\n", idx + 1, result.path, line));
                    } else {
                        out.push_str(&format!(
                            "{}. {}:{} {}\n",
                            idx + 1,
                            result.path,
                            line,
                            symbol
                        ));
                    }
                } else {
                    out.push_str(&format!("{}. {}\n", idx + 1, result.path));
                }
                out.push_str(&format!("   reason: {}\n", result.reason));
                let anchor = if symbol.is_empty() {
                    result.path.clone()
                } else {
                    format!("{symbol}@{}", result.path)
                };
                out.push_str(&format!("   anchor: {anchor}\n\n"));
            }
        }
        other => return Err(format!("unsupported where format: {other}")),
    }
    Ok(out)
}

pub(crate) fn where_top_match(result: &ctx_where::SearchResult) -> (i64, String) {
    let symbol = first_symbol_name(&result.reason);
    if let Some(m) = result
        .matches
        .iter()
        .find(|m| m.kind == "symbol" && m.line > 0)
    {
        return (m.line, symbol);
    }
    if let Some(m) = result
        .matches
        .iter()
        .find(|m| m.kind != "path" && m.line > 0)
    {
        return (m.line, symbol);
    }
    (0, symbol)
}

pub(crate) fn first_symbol_name(reason: &str) -> String {
    let Some((_, rest)) = reason.split_once("symbol match: ") else {
        return String::new();
    };
    rest.split([',', ';'])
        .next()
        .unwrap_or("")
        .trim()
        .to_string()
}
