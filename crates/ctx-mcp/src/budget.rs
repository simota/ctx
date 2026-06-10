use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::Path;

use crate::types::*;
use serde_json::{json, Map, Value};

pub(crate) fn collect_budget_files(root: &Path) -> Result<Vec<BudgetFile>, String> {
    let mut out = Vec::new();
    collect_budget_files_inner(root, root, &mut out)?;
    Ok(out)
}

pub(crate) fn collect_budget_files_inner(
    root: &Path,
    dir: &Path,
    out: &mut Vec<BudgetFile>,
) -> Result<(), String> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(|err| format!("walk {}: {err}", dir.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())?;
    entries.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

    for entry in entries {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || name == "node_modules" || name == "dist" || name == "coverage" {
            continue;
        }
        let meta = entry.metadata().map_err(|err| err.to_string())?;
        if meta.is_dir() {
            collect_budget_files_inner(root, &path, out)?;
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .map_err(|err| err.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        if rel.is_empty() || rel == "." {
            continue;
        }
        out.push(BudgetFile {
            role: infer_file_role(&rel),
            path: rel,
            abs_path: path,
            size: meta.len() as i64,
            tokens: 0,
        });
    }
    Ok(())
}

pub(crate) fn count_file_tokens(path: &Path, size: i64) -> i64 {
    match ctx_tokens::count_file(&path.to_string_lossy()) {
        Ok(tokens) => tokens,
        Err(_) => ctx_tokens::estimate_by_size(size),
    }
}

pub(crate) fn format_budget_plan(
    files: &[BudgetFile],
    budget: i64,
) -> Result<String, serde_json::Error> {
    let mut result_budget = budget;
    let mut used = 0;
    let mut included: Vec<BudgetItem> = Vec::new();
    let mut excluded: Vec<BudgetItem> = Vec::new();
    let mut candidates: Vec<BudgetItem> = Vec::new();

    if result_budget > 0 {
        for file in files {
            let mut item = BudgetItem {
                Path: file.path.clone(),
                Tokens: file.tokens,
                Reason: file.role.clone(),
                Group: String::new(),
            };
            if item.Tokens == 0 {
                item.Reason = "binary".to_string();
                excluded.push(item);
            } else if file.role == "generated" {
                item.Reason = "generated".to_string();
                excluded.push(item);
            } else if item.Tokens > result_budget / 2 {
                item.Reason = "too large".to_string();
                excluded.push(item);
            } else {
                candidates.push(item);
            }
        }

        candidates.sort_by(|a, b| {
            role_priority(&a.Reason)
                .cmp(&role_priority(&b.Reason))
                .then(a.Tokens.cmp(&b.Tokens))
                .then(a.Path.cmp(&b.Path))
        });

        for mut item in candidates {
            if used + item.Tokens > result_budget {
                item.Reason = "budget exceeded".to_string();
                excluded.push(item);
                continue;
            }
            item.Reason.clear();
            used += item.Tokens;
            included.push(item);
        }

        assign_budget_groups(&mut included);
        assign_budget_groups(&mut excluded);
    } else {
        result_budget = budget;
    }

    let mut out = Map::new();
    out.insert("budget".to_string(), json!(result_budget));
    out.insert(
        "excluded".to_string(),
        if excluded.is_empty() {
            Value::Null
        } else {
            json!(excluded)
        },
    );
    out.insert(
        "included".to_string(),
        if included.is_empty() {
            Value::Null
        } else {
            json!(included)
        },
    );
    out.insert("used".to_string(), json!(used));
    serde_json::to_string_pretty(&Value::Object(out))
}

pub(crate) fn role_priority(role: &str) -> i32 {
    match role {
        "entry" | "core" | "route" | "config" => 0,
        "test" | "util" => 1,
        "doc" | "unknown" | "" => 2,
        _ => 1,
    }
}

pub(crate) fn assign_budget_groups(items: &mut [BudgetItem]) {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for item in items.iter() {
        let dir = parent_dir_slash(&item.Path);
        if dir != "." {
            *counts.entry(dir).or_insert(0) += 1;
        }
    }
    for item in items.iter_mut() {
        let dir = parent_dir_slash(&item.Path);
        if counts.get(&dir).copied().unwrap_or(0) >= 3 {
            item.Group = dir;
        }
    }
}

pub(crate) fn parent_dir_slash(path: &str) -> String {
    match path.rfind('/') {
        Some(idx) => path[..idx].to_string(),
        None => ".".to_string(),
    }
}

pub(crate) fn infer_file_role(rel_slash: &str) -> String {
    let base = rel_slash.rsplit('/').next().unwrap_or(rel_slash);
    let lower_path = rel_slash.to_ascii_lowercase();
    let lower_base = base.to_ascii_lowercase();
    let ext = Path::new(&lower_base)
        .extension()
        .and_then(OsStr::to_str)
        .unwrap_or("");

    if lower_path.starts_with("tests/")
        || lower_path.contains("/tests/")
        || lower_base.ends_with("_test.go")
        || [".test.ts", ".test.tsx", ".test.js", ".test.go", ".test.py"]
            .iter()
            .any(|suffix| lower_base.ends_with(suffix))
    {
        return "test".to_string();
    }
    if ext == "md" || lower_base.starts_with("license") || lower_base.starts_with("readme") {
        return "doc".to_string();
    }
    if matches!(
        lower_base.as_str(),
        "package.json" | "go.mod" | "cargo.toml" | "pyproject.toml" | "dockerfile" | "makefile"
    ) || matches!(ext, "toml" | "yaml" | "yml")
    {
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
    if matches!(ext, "ts" | "tsx" | "js" | "go" | "py" | "rs") {
        return "core".to_string();
    }
    String::new()
}
