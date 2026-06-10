use std::env;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::commands::where_cmd::{where_files_with, WalkIgnoreOptions};
use crate::common::*;
use serde::Serialize;

#[derive(Debug)]
pub(crate) struct OnboardingArgs {
    root: PathBuf,
    limit: usize,
    persona: String,
    format: String,
}

// OnboardingOutput is no longer used directly — JSON is built manually in
// onboarding_command to match Go's encoding/json omitempty + integer-float behaviour.
#[allow(dead_code)]
#[derive(Debug, Serialize)]
pub(crate) struct OnboardingOutput {
    root: String,
    persona: String,
    steps: Vec<OnboardingStep>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct OnboardingStep {
    path: String,
    rank: usize,
    score: f64,
    score_breakdown: OnboardingBreakdown,
    role: String,
    reason: String,
    symbols: usize,
    description: String,
    loc: usize,
    ref_count: usize,
    hot: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct OnboardingBreakdown {
    entry_role: f64,
    churn_60d: f64,
    symbol_count: f64,
    referenced_by: f64,
}

pub(crate) fn run_onboarding_command(args: &[OsString]) -> Option<ExitCode> {
    let parsed = parse_onboarding_args(args)?;
    match onboarding_command(parsed) {
        Ok(()) => Some(ExitCode::SUCCESS),
        Err(err) => {
            eprintln!("{err}");
            Some(ExitCode::from(1))
        }
    }
}

pub(crate) fn parse_onboarding_args(args: &[OsString]) -> Option<OnboardingArgs> {
    let mut saw_onboarding = false;
    let mut limit = 10usize;
    let mut persona = "human".to_string();
    let mut format = "text".to_string();
    let mut json = false;
    let mut positionals = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == OsStr::new("--json") {
            json = true;
        } else if arg == OsStr::new("onboarding") {
            if saw_onboarding {
                return None;
            }
            saw_onboarding = true;
        } else if let Some(value) = flag_value(arg, "--limit") {
            limit = value.to_string_lossy().parse().ok()?;
        } else if arg == OsStr::new("--limit") {
            i += 1;
            limit = args.get(i)?.to_string_lossy().parse().ok()?;
        } else if let Some(value) = flag_value(arg, "--persona") {
            persona = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--persona") {
            i += 1;
            persona = args.get(i)?.to_string_lossy().into_owned();
        } else if let Some(value) = flag_value(arg, "--format") {
            format = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--format") {
            i += 1;
            format = args.get(i)?.to_string_lossy().into_owned();
        } else if is_option(arg) {
            return None;
        } else if saw_onboarding {
            positionals.push(arg.clone());
        } else {
            return None;
        }
        i += 1;
    }
    if !saw_onboarding || positionals.len() > 1 {
        return None;
    }
    if json {
        format = "json".to_string();
    }
    Some(OnboardingArgs {
        root: match positionals.as_slice() {
            [] => PathBuf::from("."),
            [root] => PathBuf::from(root),
            _ => return None,
        },
        limit,
        persona,
        format,
    })
}

pub(crate) fn onboarding_command(args: OnboardingArgs) -> Result<(), String> {
    let root = if args.root.is_absolute() {
        args.root.clone()
    } else {
        env::current_dir()
            .map_err(|err| err.to_string())?
            .join(&args.root)
    };
    let steps = rank_onboarding(&root, args.limit, &args.persona)?;
    if args.format == "json" {
        // Build JSON manually to mirror Go's encoding/json behaviour:
        //   1. Whole-number f64s serialize as integers (73.0 → 73, not 73.0).
        //   2. Zero-value / empty fields with `omitempty` are omitted:
        //      tokens (omitempty), symbols (omitempty), description (omitempty),
        //      loc (omitempty), ref_count (omitempty), hot (omitempty).
        //   3. Non-omitempty fields always appear: path, rank, score,
        //      score_breakdown, role, reason.
        let json_steps: Vec<serde_json::Value> = steps
            .iter()
            .map(|s| {
                let mut breakdown = serde_json::Map::new();
                breakdown.insert(
                    "entry_role".to_string(),
                    onboarding_go_float(s.score_breakdown.entry_role),
                );
                breakdown.insert(
                    "churn_60d".to_string(),
                    onboarding_go_float(s.score_breakdown.churn_60d),
                );
                breakdown.insert(
                    "symbol_count".to_string(),
                    onboarding_go_float(s.score_breakdown.symbol_count),
                );
                breakdown.insert(
                    "referenced_by".to_string(),
                    onboarding_go_float(s.score_breakdown.referenced_by),
                );

                let mut obj = serde_json::Map::new();
                obj.insert(
                    "path".to_string(),
                    serde_json::Value::String(s.path.clone()),
                );
                obj.insert("rank".to_string(), serde_json::json!(s.rank));
                obj.insert("score".to_string(), onboarding_go_float(s.score));
                obj.insert(
                    "score_breakdown".to_string(),
                    serde_json::Value::Object(breakdown),
                );
                obj.insert(
                    "role".to_string(),
                    serde_json::Value::String(s.role.clone()),
                );
                obj.insert(
                    "reason".to_string(),
                    serde_json::Value::String(s.reason.clone()),
                );
                // omitempty fields (omit when zero/false/empty)
                if s.symbols > 0 {
                    obj.insert("symbols".to_string(), serde_json::json!(s.symbols));
                }
                if !s.description.is_empty() {
                    obj.insert(
                        "description".to_string(),
                        serde_json::Value::String(s.description.clone()),
                    );
                }
                if s.loc > 0 {
                    obj.insert("loc".to_string(), serde_json::json!(s.loc));
                }
                if s.ref_count > 0 {
                    obj.insert("ref_count".to_string(), serde_json::json!(s.ref_count));
                }
                if s.hot {
                    obj.insert("hot".to_string(), serde_json::Value::Bool(true));
                }
                serde_json::Value::Object(obj)
            })
            .collect();
        let root_display = args.root.to_string_lossy().into_owned();
        let output = serde_json::json!({
            "root":    root_display,
            "persona": args.persona,
            "steps":   json_steps,
        });
        let pretty = serde_json::to_string_pretty(&output).map_err(|err| err.to_string())?;
        print!("{pretty}");
        println!();
    } else if args.persona == "ai" {
        println!("Reading order (top {}):", steps.len());
        for s in steps {
            let hot = if s.hot { ", hot" } else { "" };
            println!(
                " {}. {} [{}, score {}{}]",
                s.rank, s.path, s.role, s.score as i64, hot
            );
        }
    } else {
        println!("Reading order for new contributors:");
        println!();
        for s in steps {
            // Go text format: " N. path (role, LOC LOC[, hot])"
            let mut meta = format!("{}, {} LOC", s.role, s.loc);
            if s.hot {
                meta.push_str(", hot");
            }
            println!(" {}. {} ({})", s.rank, s.path, meta);
            if !s.reason.is_empty() {
                // Go uses "    → " (U+2192 arrow), not "    -> "
                println!("    \u{2192} {}", s.reason);
            }
            if !s.description.is_empty() {
                println!("    {}", s.description);
            }
            println!();
        }
    }
    Ok(())
}

pub(crate) fn rank_onboarding(
    root: &Path,
    limit: usize,
    persona: &str,
) -> Result<Vec<OnboardingStep>, String> {
    // Mirror Go onboarding/curriculum.go: same options as walk.DefaultOptions
    // except RespectCtxignore is left unset (false).
    let all_files = where_files_with(
        root,
        &WalkIgnoreOptions {
            respect_ctxignore: false,
            ..WalkIgnoreOptions::default_where()
        },
    )?;
    // Mirror Go: filter to non-dir, non-test candidates BEFORE building ref
    // counts, so test files do NOT count as reference sources.
    let candidates: Vec<_> = all_files
        .into_iter()
        .filter(|fi| !fi.is_dir && !is_onboarding_test_file(&fi.path))
        .collect();
    let ref_counts = onboarding_ref_counts(&candidates);
    let max_ref = ref_counts.values().copied().max().unwrap_or(0).max(1);
    let mut steps = Vec::new();
    for fi in candidates {
        let symbols = fi.symbols.len();
        let loc = fi.lines.len();
        let mut role = "core".to_string();
        let mut entry_role = 0.0;
        if is_onboarding_entry(&fi.path, &fi.lines) {
            role = "entry".to_string();
            entry_role = 50.0;
        } else if symbols > 0 {
            entry_role = 15.0;
        }
        let symbol_count = if symbols > 0 {
            ((symbols + 1) as f64).log2().mul_add(3.0, 0.0).min(15.0)
        } else {
            0.0
        };
        let refs = ref_counts.get(&fi.path).copied().unwrap_or(0);
        let referenced_by = (refs as f64 / max_ref as f64 * 20.0).min(20.0);
        let breakdown = OnboardingBreakdown {
            entry_role,
            churn_60d: 0.0,
            symbol_count,
            referenced_by,
        };
        let score = entry_role + symbol_count + referenced_by;
        // Mirror Go: description is only extracted for human persona.
        // For ai persona, desc="" → omitted from JSON via omitempty.
        let description = if persona == "human" {
            fi.lines
                .iter()
                .map(|line| line.trim())
                .find(|line| line.starts_with("//") || line.starts_with("#"))
                .map(|line| {
                    line.trim_start_matches('/')
                        .trim_start_matches('#')
                        .trim()
                        .to_string()
                })
                .unwrap_or_default()
        } else {
            String::new()
        };
        steps.push(OnboardingStep {
            path: fi.path,
            rank: 0,
            score: (score * 100.0).round() / 100.0,
            score_breakdown: breakdown,
            role,
            reason: String::new(),
            symbols,
            description,
            loc,
            ref_count: refs,
            hot: false,
        });
    }
    steps.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.path.cmp(&b.path))
    });
    let cap = if limit == 0 { 10 } else { limit };
    steps.truncate(cap);
    for (idx, step) in steps.iter_mut().enumerate() {
        step.rank = idx + 1;
        step.reason = onboarding_reason(step);
    }
    Ok(steps)
}

pub(crate) fn onboarding_ref_counts(
    files: &[ctx_where::FileInput],
) -> std::collections::BTreeMap<String, usize> {
    let mut out = std::collections::BTreeMap::new();
    for target in files {
        let stem = Path::new(&target.path)
            .file_stem()
            .and_then(OsStr::to_str)
            .unwrap_or("");
        if stem.is_empty() {
            continue;
        }
        let mut count = 0usize;
        for source in files {
            if source.path == target.path {
                continue;
            }
            if source.lines.iter().any(|line| line.contains(stem)) {
                count += 1;
            }
        }
        out.insert(target.path.clone(), count);
    }
    out
}

pub(crate) fn is_onboarding_test_file(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with("_test.go")
        || lower.ends_with(".test.ts")
        || lower.ends_with(".spec.ts")
        || lower.contains("/testdata/")
}

pub(crate) fn is_onboarding_entry(path: &str, lines: &[String]) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with("main.go")
        || lower.ends_with("cmd/main.go")
        || lower.ends_with("index.ts")
        || lower.ends_with("index.js")
        || lines.iter().any(|line| {
            line.contains("func main(")
                || line.contains("if __name__ == \"__main__\"")
                || line.contains("if __name__ == '__main__'")
        })
}

/// Convert a whole-number f64 to a JSON integer, mirroring Go's encoding/json
/// behaviour: `73.0` → `73`, `19.75` → `19.75`. Matches ctx-echo's go_float.
pub(crate) fn onboarding_go_float(v: f64) -> serde_json::Value {
    if v.is_finite() && v.fract() == 0.0 && v >= i64::MIN as f64 && v <= i64::MAX as f64 {
        serde_json::Value::Number(serde_json::Number::from(v as i64))
    } else {
        serde_json::json!(v)
    }
}

pub(crate) fn onboarding_reason(step: &OnboardingStep) -> String {
    // Mirror Go's buildReason exactly:
    //   entry → "application entry point"
    //   doc   → "concept overview"
    //   config → "project configuration"
    //   core with >10 symbols → "core domain (N symbols)"
    //   core with >0 symbols → "N symbols"
    //   hot   → append "frequently modified"
    //   ref_count > 0 → append "referenced by N files"
    //   fallback → "source file"
    // Parts are joined with ". " and capped at 2.
    let mut parts: Vec<String> = Vec::new();
    match step.role.as_str() {
        "entry" => parts.push("application entry point".to_string()),
        "doc" => parts.push("concept overview".to_string()),
        "config" => parts.push("project configuration".to_string()),
        _ => {
            if step.symbols > 10 {
                parts.push(format!("core domain ({} symbols)", step.symbols));
            } else if step.symbols > 0 {
                parts.push(format!("{} symbols", step.symbols));
            }
        }
    }
    if step.hot {
        parts.push("frequently modified".to_string());
    }
    if step.ref_count > 0 {
        parts.push(format!("referenced by {} files", step.ref_count));
    }
    if parts.is_empty() {
        return "source file".to_string();
    }
    if parts.len() > 2 {
        parts.truncate(2);
    }
    parts.join(". ")
}
