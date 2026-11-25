//! `ctx_deps`: the import graph around a file or directory — what it
//! imports, or what imports it (the blast radius of a change).

use std::path::{Component, Path};

use serde_json::{json, value::RawValue, Value};

use crate::protocol::*;
use crate::server::Server;
use crate::types::*;
use crate::*;

const DEFAULT_TOP: i64 = 50;

/// `anchor` as a clean '/'-separated relative path ("" for the root), or
/// `None` when it is absolute or climbs out with `..`.
pub(crate) fn clean_anchor(anchor: &str) -> Option<String> {
    let mut parts = Vec::new();
    for component in Path::new(anchor).components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            _ => return None,
        }
    }
    Some(parts.join("/"))
}

/// Files of `index` the anchor stands for: the file itself, or every file
/// with edges under a directory.
pub(crate) fn anchor_seeds(
    index: &ctx_relations::Index,
    anchor: &str,
    is_dir: bool,
) -> Vec<String> {
    if !is_dir {
        return vec![anchor.to_string()];
    }
    let prefix = if anchor.is_empty() {
        String::new()
    } else {
        format!("{anchor}/")
    };
    let mut seeds: Vec<String> = index
        .imports
        .keys()
        .chain(index.importers.keys())
        .filter(|file| file.starts_with(&prefix))
        .cloned()
        .collect();
    seeds.sort();
    seeds.dedup();
    seeds
}

impl Server {
    pub(crate) fn run_deps(&self, raw: Option<&RawValue>) -> Result<Value, RpcError> {
        let mut args: DepsArgs = parse_json_opt(raw)?;
        validate_len("path", &args.path, MAX_PATH_LEN)?;
        validate_len("anchor", &args.anchor, MAX_ANCHOR_LEN)?;
        if args.path.is_empty() {
            args.path = ".".to_string();
        }
        let direction = match args.direction.as_str() {
            "" | "imports" => ctx_relations::Direction::Imports,
            "importers" => ctx_relations::Direction::Importers,
            other => {
                return invalid_params_with_hint(
                    &format!("unknown direction {other:?}"),
                    "allowed: imports (what the anchor uses), importers (what uses the anchor)",
                )
            }
        };
        if args.depth == 0 {
            args.depth = 1;
        }
        if args.depth < 0 || args.depth > MAX_DEPS_DEPTH {
            return invalid_params_with_hint(
                &format!("depth must be between 1 and {MAX_DEPS_DEPTH}"),
                "suggested: 1 (direct) or 2",
            );
        }
        if args.top == 0 {
            args.top = DEFAULT_TOP;
        }
        if args.top < 0 || args.top > MAX_TOP {
            return invalid_params_with_hint(
                &format!("top must be between 0 and {MAX_TOP}"),
                &format!("suggested: {DEFAULT_TOP}"),
            );
        }
        if !matches!(args.format.as_str(), "" | "markdown" | "json") {
            return invalid_params_with_hint(
                &format!("unknown format {:?}", args.format),
                "allowed: markdown, json",
            );
        }
        let Some(anchor) = clean_anchor(&args.anchor) else {
            return invalid_params_with_hint(
                "anchor must be a relative path inside 'path'",
                "e.g. 'src/server.rs' or 'src/auth/'",
            );
        };
        let root = self.resolve_path(&args.path)?;
        let target = root.join(&anchor);
        let Ok(meta) = std::fs::metadata(&target) else {
            return Ok(tool_error_result(&format!(
                "ctx_deps: {} not found under {}",
                args.anchor, args.path
            )));
        };
        let index = ctx_relations::build_cached(&root.to_string_lossy())
            .map_err(|err| tool_error(format!("ctx_deps: {err}")))?;
        let seeds = anchor_seeds(&index, &anchor, meta.is_dir());
        let found = index.reach(&seeds, direction, args.depth as usize);
        let total = found.len();
        let shown: Vec<&ctx_relations::Reached> = found.iter().take(args.top as usize).collect();
        let importers = direction == ctx_relations::Direction::Importers;

        let text = if args.format == "json" {
            serde_json::to_string_pretty(&json!({
                "anchor": args.anchor,
                "direction": if importers { "importers" } else { "imports" },
                "depth": args.depth,
                "total": total,
                "files": shown,
            }))
            .map_err(tool_error)?
        } else {
            let what = if importers {
                "Files that import"
            } else {
                "Files imported by"
            };
            let mut out = format!("# {what} {}", args.anchor);
            if args.depth > 1 {
                out.push_str(&format!(" (within {} hops)", args.depth));
            }
            out.push_str("\n\n");
            if shown.is_empty() {
                out.push_str("None found. Imports are resolved for Rust, Go, TypeScript / JavaScript (incl. Svelte, Vue), Python, Java / Kotlin, PHP and Swift.\n");
            } else {
                out.push_str("| # | File | Hops | Via |\n|---|---|---|---|\n");
                for (idx, item) in shown.iter().enumerate() {
                    out.push_str(&format!(
                        "| {} | {} | {} | {} |\n",
                        idx + 1,
                        item.path,
                        item.depth,
                        item.via
                    ));
                }
                if total > shown.len() {
                    out.push_str(&format!(
                        "\n{} more not shown (raise 'top').\n",
                        total - shown.len()
                    ));
                }
                out.push_str(&format!(
                    "\nNext: read them with ctx_pack {{\"files\":[...]}} or ctx_skim; for the change set, ctx_pack {{\"changed\":true,\"{}\":5}}.\n",
                    if importers { "dependents" } else { "deps" }
                ));
            }
            out
        };
        Ok(json!({"content": [{"type": "text", "text": text}]}))
    }
}
