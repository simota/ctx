//! `ctx_tests`: the test files that exercise a file, a directory or the
//! current change — named after the source or importing it.

use std::collections::BTreeSet;

use serde_json::{json, value::RawValue, Value};

use crate::deps::clean_anchor;
use crate::pack::collect_pack_files;
use crate::protocol::*;
use crate::server::Server;
use crate::types::*;
use crate::util::{branch_changes_under, changed_paths_under};
use crate::*;

const DEFAULT_TOP: i64 = 50;

impl Server {
    pub(crate) fn run_tests(&self, raw: Option<&RawValue>) -> Result<Value, RpcError> {
        let mut args: TestsArgs = parse_json_opt(raw)?;
        validate_len("path", &args.path, MAX_PATH_LEN)?;
        validate_len("anchor", &args.anchor, MAX_ANCHOR_LEN)?;
        validate_len("base", &args.base, MAX_MIX_REF_LEN)?;
        if args.path.is_empty() {
            args.path = ".".to_string();
        }
        if !args.base.is_empty() {
            args.changed = true;
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
        if args.anchor.trim().is_empty() && !args.changed {
            return invalid_params_with_hint(
                "name an anchor or pass changed=true (or base)",
                "e.g. {\"anchor\":\"src/store.rs\"} or {\"changed\":true}",
            );
        }
        let Some(anchor) = clean_anchor(&args.anchor) else {
            return invalid_params_with_hint(
                "anchor must be a relative path inside 'path'",
                "e.g. 'src/server.rs' or 'src/auth/'",
            );
        };
        let root = self.resolve_path(&args.path)?;
        let files: Vec<String> = collect_pack_files(&root, &self.walk_boundary(&root))
            .map_err(tool_error)?
            .into_iter()
            .map(|file| file.path)
            .collect();

        let mut seeds: Vec<String> = Vec::new();
        // Test files inside a directory anchor are its tests, not sources.
        let mut inside: Vec<String> = Vec::new();
        let mut seen = BTreeSet::new();
        if !args.anchor.trim().is_empty() {
            let Ok(meta) = std::fs::metadata(root.join(&anchor)) else {
                return Ok(tool_error_result(&format!(
                    "ctx_tests: {} not found under {}",
                    args.anchor, args.path
                )));
            };
            if meta.is_dir() {
                let prefix = if anchor.is_empty() {
                    String::new()
                } else {
                    format!("{anchor}/")
                };
                for file in files.iter().filter(|file| file.starts_with(&prefix)) {
                    if ctx_relations::is_test_path(file) {
                        inside.push(file.clone());
                    } else if seen.insert(file.clone()) {
                        seeds.push(file.clone());
                    }
                }
            } else if seen.insert(anchor.clone()) {
                seeds.push(anchor.clone());
            }
        }
        if args.changed {
            let changed: Vec<String> = if args.base.is_empty() {
                changed_paths_under(&root)
                    .map_err(|err| {
                        tool_error(format!(
                            "ctx_tests: changed=true needs a Git work tree: {err}"
                        ))
                    })?
                    .into_iter()
                    .collect()
            } else {
                branch_changes_under(&root, &args.base)
                    .map_err(|err| tool_error(format!("ctx_tests: base: {err}")))?
                    .into_iter()
                    .filter(|(_, _, deleted)| !deleted)
                    .map(|(path, _, _)| path)
                    .collect()
            };
            for path in changed {
                if seen.insert(path.clone()) {
                    seeds.push(path);
                }
            }
        }

        let index = ctx_relations::build_cached(&root.to_string_lossy()).ok();
        let links = ctx_relations::find_tests(&files, index.as_ref(), &seeds);
        let changed_tests: Vec<&String> = if args.changed {
            seeds
                .iter()
                .filter(|seed| ctx_relations::is_test_path(seed))
                .collect()
        } else {
            Vec::new()
        };
        let covered: BTreeSet<&str> = links
            .iter()
            .flat_map(|link| link.subjects.iter().map(String::as_str))
            .collect();
        let untested: Vec<&String> = seeds
            .iter()
            .filter(|seed| {
                !ctx_relations::is_test_path(seed)
                    && ctx_relations::supported(seed)
                    && !covered.contains(seed.as_str())
            })
            .collect();
        let mut rows: Vec<(String, String, String)> = changed_tests
            .iter()
            .map(|path| (path.to_string(), "changed".to_string(), String::new()))
            .collect();
        rows.extend(links.iter().map(|link| {
            (
                link.path.clone(),
                link.reason().trim_start_matches("test: ").to_string(),
                link.subject.clone(),
            )
        }));
        for path in inside {
            if !rows.iter().any(|(listed, _, _)| *listed == path) {
                let dir = if anchor.is_empty() {
                    ".".to_string()
                } else {
                    format!("{anchor}/")
                };
                rows.push((path, format!("inside {dir}"), String::new()));
            }
        }
        let total = rows.len();
        rows.truncate(args.top as usize);

        let text = if args.format == "json" {
            let tests: Vec<Value> = rows
                .iter()
                .map(|(path, why, subject)| json!({"path": path, "why": why, "subject": subject}))
                .collect();
            serde_json::to_string_pretty(&json!({
                "total": total,
                "tests": tests,
                "untested": untested,
            }))
            .map_err(tool_error)?
        } else {
            let mut out = if args.anchor.trim().is_empty() {
                "# Tests for the current change\n\n".to_string()
            } else {
                format!("# Tests for {}\n\n", args.anchor)
            };
            if rows.is_empty() {
                out.push_str("No test file is named after or imports these files.\n");
            } else {
                out.push_str("| # | Test | Why |\n|---|---|---|\n");
                for (idx, (path, why, _)) in rows.iter().enumerate() {
                    out.push_str(&format!("| {} | {} | {} |\n", idx + 1, path, why));
                }
                if total > rows.len() {
                    out.push_str(&format!(
                        "\n{} more not shown (raise 'top').\n",
                        total - rows.len()
                    ));
                }
            }
            if !untested.is_empty() {
                out.push_str("\nNo test found for:\n");
                for path in untested.iter().take(args.top as usize) {
                    out.push_str(&format!("- {path}\n"));
                }
            }
            if !rows.is_empty() {
                out.push_str("\nNext: read them with ctx_pack {\"files\":[...]} (or ctx_pack {\"changed\":true,\"tests\":5}), then run them.\n");
            }
            out
        };
        Ok(json!({"content": [{"type": "text", "text": text}]}))
    }
}
