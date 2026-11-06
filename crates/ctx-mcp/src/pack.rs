use std::ffi::OsStr;
use std::path::Path;

use crate::budget::*;
use crate::protocol::*;
use crate::types::*;
use crate::util::*;
use rayon::prelude::*;
use serde_json::{json, Map, Value};

pub(crate) fn collect_pack_files(root: &Path, boundary: &Path) -> Result<Vec<PackFile>, String> {
    let files = collect_budget_files(root, boundary)?;
    Ok(files
        .into_iter()
        .map(|file| PackFile {
            path: file.path,
            abs_path: file.abs_path,
            size: file.size,
            tokens: 0,
            role: file.role,
            symbols: Vec::new(),
        })
        .collect())
}

pub(crate) fn build_focus_inputs(files: &[PackFile]) -> Vec<ctx_focus::FileInput> {
    // Independent per file: parse in parallel (the indexed collect keeps
    // the input order), reading each file once for symbols and lines.
    files
        .par_iter()
        .map(|file| {
            let content = std::fs::read(&file.abs_path).ok();
            let symbols = content
                .as_deref()
                .map(|bytes| ctx_symbols::extract_source(&file.abs_path, bytes))
                .unwrap_or_default()
                .into_iter()
                .map(|sym| ctx_focus::SymbolInfo {
                    name: sym.name,
                    kind: sym.kind,
                    line: sym.line as i64,
                })
                .collect();
            let lines = content
                .as_deref()
                .and_then(|bytes| std::str::from_utf8(bytes).ok())
                .map(|content| content.lines().map(str::to_string).collect())
                .unwrap_or_default();
            ctx_focus::FileInput {
                path: file.path.clone(),
                is_dir: false,
                symbols,
                lines,
            }
        })
        .collect()
}

/// Plans a pack of `files`, then of `companions` (co-change partners with
/// their selection reason), which are budgeted after every selected file.
pub(crate) fn build_pack_plan(
    files: Vec<PackFile>,
    companions: Vec<(PackFile, String)>,
    args: &PackArgs,
) -> PackPlan {
    let ctx = ctx_pack::RelevanceContext::new(&args.goal, args.budget);
    let mut high = Vec::new();
    let mut medium = Vec::new();
    let mut skipped = Vec::new();

    for file in files {
        if file.path == "." {
            continue;
        }
        let input = pack_file_input(&file);
        let rel = ctx_pack::relevance::score_relevance_with_ctx(&input, &ctx, file.tokens);
        let candidate = PackCandidate {
            file,
            tokens: input.tokens,
            score: rel.score,
            relevance: rel.tier.clone(),
            reason: rel.reason.clone(),
            digest: None,
        };
        match rel.tier.as_str() {
            "High" => high.push(candidate),
            "Medium" => medium.push(candidate),
            _ => {
                let reason = if rel.reason.is_empty() {
                    "outside goal scope".to_string()
                } else {
                    rel.reason
                };
                skipped.push((candidate.file.path, reason));
            }
        }
    }

    sort_pack_candidates(&mut high);
    sort_pack_candidates(&mut medium);

    let mut extra = Vec::new();
    for (file, reason) in companions {
        let input = pack_file_input(&file);
        let rel = ctx_pack::relevance::score_relevance_with_ctx(&input, &ctx, file.tokens);
        if rel.tier.is_empty() {
            continue;
        }
        extra.push(PackCandidate {
            file,
            tokens: input.tokens,
            score: rel.score,
            relevance: rel.tier,
            reason,
            digest: None,
        });
    }

    let mut plan = PackPlan {
        high: Vec::new(),
        medium: Vec::new(),
        skipped: Vec::new(),
        used: 0,
        budget: args.budget,
    };
    for mut candidate in high.into_iter().chain(medium).chain(extra) {
        if plan.used + candidate.tokens > plan.budget {
            let digest = (args.overflow == "signatures")
                .then(|| overflow_digest(&candidate))
                .flatten()
                .filter(|(_, tokens)| plan.used + tokens <= plan.budget);
            let Some((digest, tokens)) = digest else {
                plan.skipped
                    .push((candidate.file.path.clone(), "budget exceeded".to_string()));
                continue;
            };
            candidate.tokens = tokens;
            candidate.digest = Some(digest);
        }
        plan.used += candidate.tokens;
        if candidate.relevance == "High" {
            plan.high.push(candidate);
        } else {
            plan.medium.push(candidate);
        }
    }
    plan.skipped.extend(skipped);
    plan
}

/// The signature digest (and its token count) of a candidate whose full
/// text does not fit, read and redacted like a packed file.
fn overflow_digest(candidate: &PackCandidate) -> Option<(String, i64)> {
    let data = std::fs::read(&candidate.file.abs_path).ok()?;
    let body = ctx_pack::redact::redact_secrets(
        &candidate.file.abs_path.to_string_lossy(),
        std::str::from_utf8(&data).ok()?,
        &Default::default(),
    );
    let lines: Vec<i64> = candidate
        .file
        .symbols
        .iter()
        .map(|sym| sym.line as i64)
        .collect();
    let digest = ctx_pack::signatures::signature_digest(
        &candidate.file.path,
        &body,
        &lines,
        candidate.tokens,
    )?;
    let tokens = ctx_tokens::count_str(&digest).max(1);
    Some((digest, tokens))
}

pub(crate) fn pack_file_input(file: &PackFile) -> ctx_pack::FileInput {
    let content_head = std::fs::read(&file.abs_path)
        .map(|bytes| bytes.into_iter().take(512).collect())
        .unwrap_or_default();
    let symbols = file
        .symbols
        .iter()
        .map(|sym| ctx_pack::SymbolInput {
            name: sym.name.clone(),
            kind: sym.kind.clone(),
            line: sym.line as i64,
        })
        .collect();
    ctx_pack::FileInput {
        path: file.path.clone(),
        abs_path: file.abs_path.to_string_lossy().into_owned(),
        is_dir: false,
        tokens: file.tokens,
        role: file.role.clone(),
        metadata: ctx_pack::MetadataInput {
            size: file.size,
            tokens_est: file.tokens,
            role: file.role.clone(),
            symbols,
        },
        content_head,
    }
}

pub(crate) fn sort_pack_candidates(candidates: &mut [PackCandidate]) {
    candidates.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.file.path.cmp(&b.file.path))
    });
}

pub(crate) fn render_pack_markdown(plan: &PackPlan, args: &PackArgs) -> Result<String, RpcError> {
    let mut out = String::new();
    out.push_str("# Context Pack\n\n");
    out.push_str(&format!("**Goal**: {}\n", value_or_dash(&args.goal)));
    out.push_str(&format!("**Generated**: {}\n", generated_timestamp()));
    out.push_str(&format!(
        "**Budget**: {} / {} tokens\n\n",
        plan.used, plan.budget
    ));

    let included_count = plan.high.len() + plan.medium.len();
    out.push_str(&format!(
        "## Included files ({} files, {} tokens)\n\n",
        included_count, plan.used
    ));
    write_pack_relevance_list(&mut out, "High relevance", &plan.high, args.explain);
    write_pack_relevance_list(&mut out, "Medium relevance", &plan.medium, args.explain);

    if !plan.skipped.is_empty() {
        out.push_str("## Skipped\n");
        for (path, reason) in &plan.skipped {
            out.push_str(&format!("- {path} ({reason})\n"));
        }
        out.push('\n');
    }

    if let Some(next) = pack_next_hint(plan) {
        out.push_str("## Next\n");
        out.push_str(&next);
        out.push('\n');
    }

    out.push_str("---\n\n");
    out.push_str("## File contents\n\n");
    for candidate in plan.high.iter().chain(&plan.medium) {
        match &candidate.digest {
            Some(digest) => write_pack_digest(&mut out, &candidate.file, digest),
            None => write_pack_file_content(&mut out, &candidate.file)?,
        }
    }
    Ok(out)
}

pub(crate) fn write_pack_relevance_list(
    out: &mut String,
    title: &str,
    files: &[PackCandidate],
    explain: bool,
) {
    out.push_str(&format!("### {title}\n"));
    if files.is_empty() {
        out.push_str("- -\n");
    } else {
        for candidate in files {
            let tokens = format_count(candidate.tokens);
            let note = if candidate.digest.is_some() {
                ", signatures only"
            } else {
                ""
            };
            if explain && !candidate.reason.is_empty() {
                out.push_str(&format!(
                    "- {} ({} tokens{note}) - {}\n",
                    candidate.file.path, tokens, candidate.reason
                ));
            } else {
                out.push_str(&format!(
                    "- {} ({} tokens{note})\n",
                    candidate.file.path, tokens
                ));
            }
        }
    }
    out.push('\n');
}

pub(crate) fn pack_next_hint(plan: &PackPlan) -> Option<String> {
    let mut out = String::new();
    if let Some(top) = plan.high.first().or_else(|| plan.medium.first()) {
        let base = Path::new(&top.file.path)
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or(&top.file.path);
        out.push_str(&format!(
            "- Top-relevance file: {} ({} tokens). Call `ctx_focus {{\"anchor\":{}}}` for a symbol-level dive.\n",
            top.file.path,
            format_count(top.tokens),
            serde_json::to_string(base).unwrap_or_else(|_| "\"\"".to_string())
        ));
    }
    for (path, reason) in &plan.skipped {
        if reason.to_ascii_lowercase().contains("budget") {
            out.push_str(&format!(
                "- Dropped {path} due to budget cap. Call `ctx_skim {{\"path\":{}}}` for a compressed view.\n",
                serde_json::to_string(path).unwrap_or_else(|_| "\"\"".to_string())
            ));
            break;
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn write_pack_digest(out: &mut String, file: &PackFile, digest: &str) {
    out.push_str(&format!("### {}\n\n", file.path));
    out.push_str(&format!(
        "```{}\n",
        ctx_pack::diff::lang_from_path(&file.path)
    ));
    out.push_str(digest);
    out.push_str("```\n\n");
}

pub(crate) fn write_pack_file_content(out: &mut String, file: &PackFile) -> Result<(), RpcError> {
    out.push_str(&format!("### {}\n\n", file.path));
    out.push_str(&format!(
        "```{}\n",
        ctx_pack::diff::lang_from_path(&file.path)
    ));
    let data = std::fs::read(&file.abs_path)
        .map_err(|err| tool_error(format!("pack: read {}: {err}", file.path)))?;
    // ctx_pack output goes straight to an agent: redact secrets as the CLI
    // pack does with its default `[security]` settings.
    let body = ctx_pack::redact::redact_secrets(
        &file.abs_path.to_string_lossy(),
        &String::from_utf8_lossy(&data),
        &Default::default(),
    );
    out.push_str(&body);
    if !data.is_empty() && data.last() != Some(&b'\n') {
        out.push('\n');
    }
    out.push_str("```\n\n");
    Ok(())
}

pub(crate) fn render_pack_json(
    plan: &PackPlan,
    args: &PackArgs,
) -> Result<String, serde_json::Error> {
    let included: Vec<Value> = plan
        .high
        .iter()
        .chain(&plan.medium)
        .map(|candidate| {
            let mut m = Map::new();
            m.insert("path".to_string(), json!(candidate.file.path));
            if !candidate.reason.is_empty() {
                m.insert("reason".to_string(), json!(candidate.reason));
            }
            m.insert("relevance".to_string(), json!(candidate.relevance));
            if candidate.digest.is_some() {
                m.insert("signatures_only".to_string(), json!(true));
            }
            m.insert("tokens".to_string(), json!(candidate.tokens));
            Value::Object(m)
        })
        .collect();
    let skipped: Vec<Value> = plan
        .skipped
        .iter()
        .map(|(path, reason)| json!({"path": path, "reason": reason}))
        .collect();
    let mut m = Map::new();
    m.insert("budget".to_string(), json!(plan.budget));
    m.insert("goal".to_string(), json!(value_or_dash(&args.goal)));
    m.insert("included".to_string(), Value::Array(included));
    m.insert("skipped".to_string(), Value::Array(skipped));
    m.insert("used".to_string(), json!(plan.used));
    m.insert("warnings".to_string(), Value::Array(Vec::new()));
    let mut out = serde_json::to_string_pretty(&Value::Object(m))?;
    out.push('\n');
    Ok(out)
}

pub(crate) fn render_pack_plain(plan: &PackPlan, args: &PackArgs) -> Result<String, RpcError> {
    let mut out = String::new();
    out.push_str(&format!(
        "Context Pack\nGoal: {}\nBudget: {} / {} tokens\nFiles: {}\n\n",
        value_or_dash(&args.goal),
        plan.used,
        plan.budget,
        plan.high.len() + plan.medium.len()
    ));
    for candidate in plan.high.iter().chain(&plan.medium) {
        let note = if candidate.digest.is_some() {
            ", signatures only"
        } else {
            ""
        };
        out.push_str(&format!(
            "+ {}  ({} tokens{note})\n",
            candidate.file.path, candidate.tokens
        ));
    }
    for (path, reason) in &plan.skipped {
        out.push_str(&format!("- {path}  ({reason})\n"));
    }
    Ok(out)
}

pub(crate) fn render_pack_xml(plan: &PackPlan, args: &PackArgs) -> String {
    format!(
        "<context-pack goal={:?} used={:?} budget={:?} files={:?} />\n",
        value_or_dash(&args.goal),
        plan.used.to_string(),
        plan.budget.to_string(),
        (plan.high.len() + plan.medium.len()).to_string()
    )
}

/// `ctx_pack` with `files` or `mix`: pack an explicit, ordered selection
/// under `root` (see `ctx_pack::selection`). A mix supplies its goal and
/// budget unless the call set them.
pub(crate) fn run_pack_selection(
    root: &Path,
    args: &PackArgs,
    explicit_budget: bool,
) -> Result<String, RpcError> {
    let mut goal = args.goal.clone();
    let mut budget = args.budget;
    let mut label = String::new();
    let files = if args.mix.trim().is_empty() {
        args.files.clone()
    } else {
        let store = ctx_mix::Store::open_default().map_err(|e| tool_error(e.to_string()))?;
        let mix = store.find(&args.mix).map_err(|e| {
            tool_error(format!(
                "ctx_pack: {e}. Saved mixes are created with `ctx mix save` or the ctx browse pinned board."
            ))
        })?;
        if goal.is_empty() {
            goal = mix.goal.clone();
        }
        if !explicit_budget && mix.budget.limit > 0 {
            budget = mix.budget.limit;
        }
        label = format!("{} ({})", mix.name, mix.id);
        mix.files
    };
    let format = match args.format.as_str() {
        "markdown" | "plain" => args.format.as_str(),
        "json" => "markdown",
        other => {
            return Err(tool_error(format!(
                "ctx_pack: format {other:?} is not supported with files / mix (use markdown, plain or json)"
            )))
        }
    };
    let opts = ctx_pack::selection::SelectionOptions {
        goal: goal.clone(),
        budget,
        format: format.to_string(),
        contract: false,
    };
    let pack = ctx_pack::selection::pack_selection(root, &files, &opts).map_err(tool_error)?;
    if args.format == "json" {
        let mut m = Map::new();
        m.insert("budget".to_string(), json!(pack.budget));
        m.insert("goal".to_string(), json!(value_or_dash(&goal)));
        m.insert(
            "included".to_string(),
            Value::Array(
                pack.included
                    .iter()
                    .map(|f| json!({"path": f.path, "relevance": "selected", "tokens": f.tokens}))
                    .collect(),
            ),
        );
        if !label.is_empty() {
            m.insert("mix".to_string(), json!(label));
        }
        m.insert(
            "skipped".to_string(),
            Value::Array(
                pack.skipped
                    .iter()
                    .map(|s| json!({"path": s.path, "reason": s.reason}))
                    .collect(),
            ),
        );
        m.insert("used".to_string(), json!(pack.used));
        m.insert("warnings".to_string(), Value::Array(Vec::new()));
        let mut out = serde_json::to_string_pretty(&Value::Object(m)).map_err(tool_error)?;
        out.push('\n');
        return Ok(out);
    }
    let mut out = String::new();
    if !label.is_empty() {
        out.push_str(&format!("Mix: {label}\n\n"));
    }
    out.push_str(&pack.content);
    if !pack.skipped.is_empty() {
        out.push_str("\n## Skipped\n");
        for s in &pack.skipped {
            out.push_str(&format!("- {} ({})\n", s.path, s.reason));
        }
        if pack.skipped.iter().any(|s| s.reason.starts_with("budget")) {
            out.push_str(
                "\nRaise 'budget', or call `ctx_skim {\"path\":...}` for a compressed view of a dropped file.\n",
            );
        }
    }
    Ok(out)
}
