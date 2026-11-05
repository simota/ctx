//! `ctx_cochange`: files that historically change together (git co-change).

use std::path::{Component, Path};

use serde_json::{json, value::RawValue, Value};

use crate::protocol::*;
use crate::server::Server;
use crate::types::*;
use crate::*;

const DEFAULT_COMMITS: i64 = 500;
const DEFAULT_MIN_WEIGHT: i64 = 2;
const DEFAULT_TOP: i64 = 20;

/// How repository paths map onto paths the client can pass back to this
/// server (relative to the server root).
enum PathView {
    /// The server root is the repository top level or inside it: strip this
    /// prefix; paths without it lie outside the server root and are hidden.
    Strip(String),
    /// The repository is nested under the server root: add this prefix.
    Add(String),
}

impl PathView {
    fn display(&self, repo_path: &str) -> Option<String> {
        match self {
            PathView::Strip(prefix) => repo_path
                .strip_prefix(prefix.as_str())
                .filter(|rest| !rest.is_empty())
                .map(str::to_string),
            PathView::Add(prefix) => Some(format!("{prefix}{repo_path}")),
        }
    }
}

fn dir_prefix(rel: &Path) -> String {
    let mut out = String::new();
    for part in rel.components() {
        if let Component::Normal(part) = part {
            out.push_str(&part.to_string_lossy());
            out.push('/');
        }
    }
    out
}

/// `anchor` as a clean '/'-separated relative path, or `None` when it is
/// absolute or climbs out with `..`.
fn clean_anchor(anchor: &str) -> Option<String> {
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

fn fmt_time(secs: i64) -> String {
    if secs <= 0 {
        "-".to_string()
    } else {
        ctx_pack::assemble::format_rfc3339_utc(secs)[..10].to_string()
    }
}

impl Server {
    pub(crate) fn run_cochange(&self, raw: Option<&RawValue>) -> Result<Value, RpcError> {
        let mut args: CoChangeArgs = parse_json_opt(raw)?;
        validate_len("path", &args.path, MAX_PATH_LEN)?;
        validate_len("anchor", &args.anchor, MAX_ANCHOR_LEN)?;
        validate_len("since", &args.since, MAX_SINCE_LEN)?;
        if args.path.is_empty() {
            args.path = ".".to_string();
        }
        for (name, value, max, hint) in [
            ("commits", &mut args.commits, MAX_COMMITS, DEFAULT_COMMITS),
            (
                "min_weight",
                &mut args.min_weight,
                MAX_MIN_WEIGHT,
                DEFAULT_MIN_WEIGHT,
            ),
            ("top", &mut args.top, MAX_TOP, DEFAULT_TOP),
        ] {
            if *value < 0 || *value > max {
                return invalid_params_with_hint(
                    &format!("{name} must be between 0 and {max}"),
                    &format!("suggested: {hint}"),
                );
            }
            if *value == 0 {
                *value = hint;
            }
        }
        if !matches!(args.format.as_str(), "" | "markdown" | "json") {
            return invalid_params_with_hint(
                &format!("unknown format {:?}", args.format),
                "allowed: markdown, json",
            );
        }
        let since = if args.since.is_empty() {
            None
        } else {
            Some(ctx_digest::git_since_arg(&args.since).map_err(tool_error)?)
        };
        let Some(anchor) = clean_anchor(&args.anchor) else {
            return invalid_params_with_hint(
                "anchor must be a relative path inside 'path'",
                "e.g. 'src/server.rs' or 'internal/auth/'",
            );
        };

        let root = self.resolve_path(&args.path)?;
        let (Ok(toplevel), Ok(root_prefix)) =
            (ctx_git::show_toplevel(&root), ctx_git::show_prefix(&root))
        else {
            return Ok(tool_error_result("ctx_cochange: not a Git repository"));
        };
        let toplevel = std::fs::canonicalize(&toplevel).unwrap_or(toplevel);
        let view = if let Ok(rel) = self.root.strip_prefix(&toplevel) {
            PathView::Strip(dir_prefix(rel))
        } else if let Ok(rel) = toplevel.strip_prefix(&self.root) {
            PathView::Add(dir_prefix(rel))
        } else {
            // --allow-outside-root: report repository-relative paths.
            PathView::Strip(String::new())
        };

        let limit = args.commits as usize;
        let min_weight = args.min_weight as u32;
        let top = args.top as usize;
        let json_out = args.format == "json";

        if anchor.is_empty() && args.anchor.trim().is_empty() {
            let found = match ctx_git::co_change_top_pairs(
                &toplevel,
                limit,
                since.as_deref(),
                min_weight,
                0,
            ) {
                Ok(found) => found,
                Err(err) => return Ok(tool_error_result(&format!("ctx_cochange: {err}"))),
            };
            let scoped = |p: &str| -> Option<String> {
                let rel = p.strip_prefix(root_prefix.as_str())?;
                view.display(&format!("{root_prefix}{rel}"))
            };
            let (scanned, truncated) = (found.commits_scanned, found.truncated);
            let pairs: Vec<(String, String, u32)> = found
                .pairs
                .into_iter()
                .filter_map(|pair| Some((scoped(&pair.a)?, scoped(&pair.b)?, pair.weight)))
                .take(top)
                .collect();
            let text = if json_out {
                let items: Vec<Value> = pairs
                    .iter()
                    .map(|(a, b, w)| json!({"a": a, "b": b, "weight": w}))
                    .collect();
                serde_json::to_string_pretty(&json!({
                    "pairs": items,
                    "commits_scanned": scanned,
                    "truncated": truncated,
                }))
                .map_err(tool_error)?
            } else {
                let mut out = format!(
                    "# Most coupled file pairs ({scanned} commits scanned{})\n\n",
                    if truncated {
                        ", older history truncated"
                    } else {
                        ""
                    }
                );
                if pairs.is_empty() {
                    out.push_str("No file pair changed together often enough (try a lower min_weight or more commits).\n");
                } else {
                    out.push_str("| # | Together | File A | File B |\n|---|---|---|---|\n");
                    for (idx, (a, b, w)) in pairs.iter().enumerate() {
                        out.push_str(&format!("| {} | {w} | {a} | {b} |\n", idx + 1));
                    }
                }
                out
            };
            return Ok(json!({"content": [{"type": "text", "text": text}]}));
        }

        let repo_anchor = format!("{root_prefix}{anchor}");
        let found = match ctx_git::co_change_partners(
            &toplevel,
            std::slice::from_ref(&repo_anchor),
            limit,
            since.as_deref(),
            min_weight,
        ) {
            Ok(found) => found,
            Err(err) => return Ok(tool_error_result(&format!("ctx_cochange: {err}"))),
        };
        let partners: Vec<(String, &ctx_git::CoChangePartner)> = found
            .partners
            .iter()
            .filter_map(|p| Some((view.display(&p.path)?, p)))
            .take(top)
            .collect();
        let shown_anchor = view.display(&repo_anchor).unwrap_or(anchor.clone());
        let text = if json_out {
            let items: Vec<Value> = partners
                .iter()
                .map(|(path, p)| {
                    json!({
                        "path": path,
                        "weight": p.weight,
                        "commits": p.commits,
                        "confidence": (p.confidence * 100.0).round() / 100.0,
                        "last_commit": fmt_time(p.last_commit_time),
                    })
                })
                .collect();
            serde_json::to_string_pretty(&json!({
                "anchor": shown_anchor,
                "anchor_files": found.matched_anchors.len(),
                "partners": items,
                "commits_scanned": found.commits_scanned,
                "truncated": found.truncated,
            }))
            .map_err(tool_error)?
        } else {
            let mut out = format!(
                "# Co-change partners of {shown_anchor} ({} commits scanned{})\n\n",
                found.commits_scanned,
                if found.truncated {
                    ", older history truncated"
                } else {
                    ""
                }
            );
            if found.matched_anchors.is_empty() {
                out.push_str(&format!(
                    "{shown_anchor} has no commits in the scanned history that touch another file at least {min_weight} times.\n"
                ));
            } else if partners.is_empty() {
                out.push_str(&format!(
                    "No file changed together with {shown_anchor} at least {min_weight} times. It looks safe to change on its own.\n"
                ));
            } else {
                out.push_str(
                    "| # | Path | Together | Confidence | Last change |\n|---|---|---|---|---|\n",
                );
                for (idx, (path, p)) in partners.iter().enumerate() {
                    out.push_str(&format!(
                        "| {} | {path} | {} | {:.0}% | {} |\n",
                        idx + 1,
                        p.weight,
                        p.confidence * 100.0,
                        fmt_time(p.last_commit_time)
                    ));
                }
                out.push_str(
                    "\nConfidence = share of the anchor's commits that also touched the file.\n",
                );
                out.push_str("\n## Suggested next reads\n\n");
                for (path, _) in partners.iter().take(3) {
                    let quoted = serde_json::to_string(path).unwrap_or_default();
                    out.push_str(&format!("- `ctx_skim {{\"path\":{quoted}}}`\n"));
                }
            }
            out
        };
        Ok(json!({"content": [{"type": "text", "text": text}]}))
    }
}
