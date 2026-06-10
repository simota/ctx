use std::path::Path;

use super::*;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub(crate) struct NativeWhyDiagnostic {
    pub(crate) path: String,
    pub(crate) exists: bool,
    pub(crate) score: i64,
    #[serde(skip_serializing_if = "is_default_breakdown")]
    pub(crate) breakdown: ctx_pack::ScoreBreakdown,
    pub(crate) tier: String,
    pub(crate) decision: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(crate) reason: String,
    #[serde(skip_serializing_if = "is_zero_i64")]
    pub(crate) threshold_gap: i64,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(crate) next_tier: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(crate) nearest_keyword: String,
    #[serde(skip_serializing_if = "is_zero_i64")]
    pub(crate) keyword_dist: i64,
}

pub(crate) fn diagnose_pack_why(
    root: &Path,
    args: &PackArgs,
    cfg: &PackCtxToml,
) -> Result<Vec<NativeWhyDiagnostic>, String> {
    let mut inputs = Vec::new();
    let base = if root.is_file() {
        root.parent().unwrap_or_else(|| Path::new("."))
    } else {
        root
    };
    let ignore = PackIgnore::load(base, &cfg.ignore.patterns, true);
    collect_pack_inputs(base, root, &ignore, &mut inputs)?;
    let mut by_path: std::collections::BTreeMap<String, ctx_pack::FileInput> =
        std::collections::BTreeMap::new();
    for input in inputs {
        by_path.insert(input.path.clone(), input);
    }
    let ctx = ctx_pack::RelevanceContext::new(&args.goal, args.budget);
    let keywords = ctx_pack::extract_goal_keywords(&args.goal);
    let mut diagnostics = Vec::with_capacity(args.why_paths.len());
    for raw in &args.why_paths {
        diagnostics.push(diagnose_pack_why_one(raw, base, &by_path, &ctx, &keywords));
    }
    Ok(diagnostics)
}

pub(crate) fn diagnose_pack_why_one(
    raw: &str,
    root: &Path,
    by_path: &std::collections::BTreeMap<String, ctx_pack::FileInput>,
    ctx: &ctx_pack::RelevanceContext,
    keywords: &[String],
) -> NativeWhyDiagnostic {
    let mut rel = normalize_pack_why_path(raw, root);
    let mut input = by_path.get(&rel);
    if input.is_none() {
        if let Some((actual, found)) = by_path
            .iter()
            .find(|(path, _)| path.eq_ignore_ascii_case(&rel))
        {
            rel = actual.clone();
            input = Some(found);
        }
    }
    let Some(input) = input else {
        let (nearest_keyword, keyword_dist) = nearest_keyword_for_path(&rel, keywords);
        return NativeWhyDiagnostic {
            path: rel,
            exists: false,
            score: 0,
            breakdown: ctx_pack::ScoreBreakdown::default(),
            tier: "outside_scope".to_string(),
            decision: "outside_scope".to_string(),
            reason: "file not found in repo walk".to_string(),
            threshold_gap: 0,
            next_tier: String::new(),
            nearest_keyword,
            keyword_dist,
        };
    };
    let result = ctx_pack::relevance::score_relevance_with_ctx(input, ctx, input.tokens);
    let (tier, decision, reason) = match result.tier.as_str() {
        "High" => ("high", "included", "high relevance".to_string()),
        "Medium" => ("medium", "included", "medium relevance".to_string()),
        _ => match result.reason.as_str() {
            "binary file" => ("skip", "skipped", "binary file".to_string()),
            "generated" => ("skip", "skipped", "generated file".to_string()),
            "low relevance" => ("low", "skipped", "low relevance".to_string()),
            reason if !reason.is_empty() => ("outside_scope", "outside_scope", reason.to_string()),
            _ => (
                "outside_scope",
                "outside_scope",
                "outside goal scope".to_string(),
            ),
        },
    };
    let (threshold_gap, next_tier) = threshold_gap_for_score(result.score);
    let (nearest_keyword, keyword_dist) =
        if !keywords.is_empty() && !has_exact_keyword_match(input, keywords) {
            nearest_keyword_for_input(input, keywords)
        } else {
            (String::new(), 0)
        };
    NativeWhyDiagnostic {
        path: input.path.clone(),
        exists: true,
        score: result.score,
        breakdown: result.breakdown,
        tier: tier.to_string(),
        decision: decision.to_string(),
        reason,
        threshold_gap,
        next_tier,
        nearest_keyword,
        keyword_dist,
    }
}

pub(crate) fn render_why_diagnostics(
    diagnostics: &[NativeWhyDiagnostic],
    format: &str,
) -> Result<String, String> {
    if format == "json" {
        let mut out = serde_json::to_string_pretty(diagnostics).map_err(|err| err.to_string())?;
        out.push('\n');
        return Ok(out);
    }
    let mut out = String::new();
    for (index, diag) in diagnostics.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(&format!("File: {}\n", diag.path));
        if !diag.exists {
            out.push_str(&format!(
                "  Decision: {} ({})\n",
                diag.decision,
                default_reason(&diag.reason, "file not found")
            ));
            if !diag.nearest_keyword.is_empty() {
                out.push_str(&format!(
                    "  Nearest keyword: {:?} (distance {})\n",
                    diag.nearest_keyword, diag.keyword_dist
                ));
            }
            continue;
        }
        let decision = if diag.reason.is_empty() {
            diag.decision.clone()
        } else {
            format!("{} ({})", diag.decision, diag.reason)
        };
        out.push_str(&format!("  Decision: {decision}\n"));
        out.push_str(&format!(
            "  Score: {} [basename:{}, path:{}, symbol:{}, content:{}, role:{}]\n",
            diag.score,
            diag.breakdown.basename,
            diag.breakdown.path,
            diag.breakdown.symbol,
            diag.breakdown.content,
            diag.breakdown.role
        ));
        let tier = if !diag.next_tier.is_empty() && diag.threshold_gap > 0 {
            format!(
                "{} (would need +{} to reach {} tier)",
                diag.tier, diag.threshold_gap, diag.next_tier
            )
        } else {
            diag.tier.clone()
        };
        out.push_str(&format!("  Tier: {tier}\n"));
        if !diag.nearest_keyword.is_empty() {
            out.push_str(&format!(
                "  Nearest keyword: {:?} (distance {}) - try refining your goal\n",
                diag.nearest_keyword, diag.keyword_dist
            ));
        }
    }
    Ok(out)
}

pub(crate) fn default_reason<'a>(reason: &'a str, fallback: &'a str) -> &'a str {
    if reason.is_empty() {
        fallback
    } else {
        reason
    }
}

pub(crate) fn threshold_gap_for_score(score: i64) -> (i64, String) {
    if score >= 10 {
        (0, String::new())
    } else if score >= 3 {
        (10 - score, "high".to_string())
    } else {
        (3 - score, "medium".to_string())
    }
}

pub(crate) fn normalize_pack_why_path(raw: &str, root: &Path) -> String {
    let path = Path::new(raw.trim().trim_matches('"'));
    let rel = if path.is_absolute() {
        path.strip_prefix(root).unwrap_or(path)
    } else {
        path
    };
    clean_pack_input_path(&rel.to_string_lossy())
}

pub(crate) fn nearest_keyword_for_input(
    input: &ctx_pack::FileInput,
    keywords: &[String],
) -> (String, i64) {
    let mut tokens = name_tokens_for_path(&input.path);
    for symbol in &input.metadata.symbols {
        let name = symbol.name.to_lowercase();
        if !name.is_empty() {
            tokens.push(name);
        }
    }
    nearest_keyword_over(&tokens, keywords)
}

pub(crate) fn nearest_keyword_for_path(path: &str, keywords: &[String]) -> (String, i64) {
    let tokens = name_tokens_for_path(path);
    nearest_keyword_over(&tokens, keywords)
}

pub(crate) fn name_tokens_for_path(path: &str) -> Vec<String> {
    let path = Path::new(path);
    let mut tokens = Vec::new();
    if let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) {
        if !stem.is_empty() {
            tokens.push(stem.to_lowercase());
        }
    }
    if let Some(parent) = path.parent() {
        for component in parent.components() {
            let text = component.as_os_str().to_string_lossy().to_lowercase();
            if !text.is_empty() && text != "." {
                tokens.push(text);
            }
        }
    }
    tokens
}

pub(crate) fn nearest_keyword_over(name_tokens: &[String], keywords: &[String]) -> (String, i64) {
    let mut best = String::new();
    let mut best_dist: Option<usize> = None;
    for keyword in keywords {
        for token in name_tokens {
            let dist = levenshtein(keyword, token);
            if dist == 0 {
                continue;
            }
            if best_dist.is_none_or(|current| dist < current) {
                best = keyword.clone();
                best_dist = Some(dist);
            }
        }
    }
    (best, best_dist.unwrap_or(0) as i64)
}

pub(crate) fn has_exact_keyword_match(input: &ctx_pack::FileInput, keywords: &[String]) -> bool {
    let path = input.path.to_lowercase();
    keywords.iter().any(|keyword| {
        path.contains(keyword)
            || input
                .metadata
                .symbols
                .iter()
                .any(|symbol| symbol.name.to_lowercase().contains(keyword))
    })
}

pub(crate) fn levenshtein(a: &str, b: &str) -> usize {
    let b_chars: Vec<char> = b.chars().collect();
    let mut costs: Vec<usize> = (0..=b_chars.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut prev = costs[0];
        costs[0] = i + 1;
        for (j, cb) in b_chars.iter().enumerate() {
            let old = costs[j + 1];
            costs[j + 1] = if ca == *cb {
                prev
            } else {
                1 + prev.min(costs[j]).min(costs[j + 1])
            };
            prev = old;
        }
    }
    costs[b_chars.len()]
}
