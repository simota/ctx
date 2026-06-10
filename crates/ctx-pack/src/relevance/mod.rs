// crates/ctx-pack/src/relevance/mod.rs
//
// Port of internal/pack/relevance.go. Behaviour parity is the
// commitment — every signal string, every breakdown bucket, every
// tier threshold must match the Go side byte-for-byte so the byte-
// diff verification passes.
//
// Design split for the sticky-handle session API:
//   * extract_goal_keywords / role_boost / score_relevance are the
//     pure functions; they are the building blocks the session
//     wraps. We expose them via `pub use` for parity testing.
//   * RelevanceContext holds the per-corpus state we want to share
//     across many score calls: the extracted goal keywords +
//     pre-lowercased / pre-trimmed form of each keyword. That state
//     is what the session's open() materialises ONCE.

use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::types::{FileInput, RelevanceResult, ScoreBreakdown, SymbolInput};

pub mod session;

static GOAL_TOKEN_RE: Lazy<Regex> = Lazy::new(|| {
    // Mirrors internal/pack/relevance.go: `[\p{Han}\p{Hiragana}\p{Katakana}]+|[A-Za-z0-9_-]+`
    Regex::new(r"[\p{Han}\p{Hiragana}\p{Katakana}]+|[A-Za-z0-9_-]+")
        .expect("goal token regex")
});

fn stopwords() -> &'static HashSet<&'static str> {
    static SET: Lazy<HashSet<&'static str>> = Lazy::new(|| {
        let mut s = HashSet::new();
        for w in [
            "a", "an", "and", "for", "in", "of", "on", "or", "the", "to", "with",
            "が", "から", "したい", "する", "で", "です", "と", "について", "に",
            "の", "は", "まで", "を",
            "調べたい", "調べる", "見たい", "知りたい", "確認したい",
            "レビューしたい",
        ] {
            s.insert(w);
        }
        s
    });
    &SET
}

fn goal_aliases() -> &'static [(&'static str, &'static [&'static str])] {
    // Iteration order in Go's map literal is unspecified, but
    // extract_goal_keywords dedups via a seen map and the test only
    // checks set membership. We sort by Japanese token (utf8 byte
    // order) so the seq is deterministic. Keep this in step with
    // internal/pack/relevance.go `goalAliases`.
    static ALIASES: Lazy<Vec<(&'static str, &'static [&'static str])>> = Lazy::new(|| {
        let mut v: Vec<(&'static str, &'static [&'static str])> = vec![
            ("ログイン", &["login", "auth", "session"]),
            ("認証", &["auth", "authentication", "login", "session"]),
            ("セッション", &["session", "auth"]),
            ("権限", &["permission", "authorization", "auth"]),
            ("設定", &["config", "settings"]),
        ];
        // Iterate in declaration order. Go's map iteration may
        // produce a different order on each run, but extract_goal_keywords
        // calls addWord which dedups by seen set — the resulting
        // vector ORDER may differ but membership does not. The Go
        // tests assert membership not order; mirror that contract.
        v.sort_by_key(|(jp, _)| *jp);
        v
    });
    &ALIASES
}

/// Mirrors internal/pack/relevance.go::extractGoalKeywords.
pub fn extract_goal_keywords(goal: &str) -> Vec<String> {
    let lower = goal.to_lowercase();
    let mut seen: HashSet<String> = HashSet::new();
    let mut words: Vec<String> = Vec::new();

    let add_word = |word: &str, seen: &mut HashSet<String>, words: &mut Vec<String>| {
        let trimmed = word.trim_matches(|c| c == '_' || c == '-');
        if trimmed.is_empty() || is_stopword(trimmed) || keyword_too_short(trimmed) {
            return;
        }
        if seen.contains(trimmed) {
            return;
        }
        seen.insert(trimmed.to_string());
        words.push(trimmed.to_string());
    };

    for (jp, aliases) in goal_aliases() {
        if lower.contains(*jp) {
            add_word(jp, &mut seen, &mut words);
            for alias in *aliases {
                add_word(alias, &mut seen, &mut words);
            }
        }
    }
    let normalized = normalize_goal_text(&lower);
    for m in GOAL_TOKEN_RE.find_iter(&normalized) {
        add_word(m.as_str(), &mut seen, &mut words);
    }
    words
}

fn normalize_goal_text(goal: &str) -> String {
    // Same pairs as strings.NewReplacer in relevance.go.
    let pairs: [(&str, &str); 13] = [
        ("について", " "),
        ("調べたい", " "),
        ("したい", " "),
        ("レビューしたい", " "),
        ("確認したい", " "),
        ("を", " "),
        ("が", " "),
        ("は", " "),
        ("の", " "),
        ("に", " "),
        ("で", " "),
        ("から", " "),
        ("まで", " "),
    ];
    // strings.NewReplacer applies longest-first match using a trie.
    // To stay byte-compatible, we sort by descending key length and
    // walk the string left-to-right, picking the longest hit at each
    // position. This guarantees "レビューしたい" wins over "したい".
    let mut sorted: Vec<(&str, &str)> = pairs.to_vec();
    sorted.sort_by_key(|(k, _)| std::cmp::Reverse(k.len()));

    let bytes = goal.as_bytes();
    let mut out = String::with_capacity(goal.len());
    let mut i = 0usize;
    while i < bytes.len() {
        let mut matched = false;
        for (k, v) in &sorted {
            let kb = k.as_bytes();
            if i + kb.len() <= bytes.len() && &bytes[i..i + kb.len()] == kb {
                out.push_str(v);
                i += kb.len();
                matched = true;
                break;
            }
        }
        if !matched {
            // Copy one UTF-8 char.
            let ch = goal[i..].chars().next().expect("utf8 char");
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

fn keyword_too_short(word: &str) -> bool {
    if word.chars().count() >= 2 {
        return false;
    }
    stopwords().contains(word)
}

fn is_stopword(word: &str) -> bool {
    stopwords().contains(word)
}

/// Mirrors model.FileRole constants.
fn file_role(f: &FileInput) -> &str {
    if !f.metadata.role.is_empty() {
        return &f.metadata.role;
    }
    &f.role
}

/// Mirrors roleBoost — keywords-aware doc bonus.
pub fn role_boost(f: &FileInput, keywords: &[String]) -> i64 {
    match file_role(f) {
        "entry" => 3,
        "core" => 2,
        "route" => 2,
        "config" => 1,
        "test" => -1,
        "doc" => {
            if goal_mentions_docs(keywords) {
                3
            } else {
                -2
            }
        }
        _ => 0,
    }
}

fn goal_mentions_docs(keywords: &[String]) -> bool {
    for kw in keywords {
        if matches!(
            kw.as_str(),
            "doc"
                | "docs"
                | "document"
                | "documentation"
                | "readme"
                | "仕様"
                | "ドキュメント"
                | "説明"
        ) {
            return true;
        }
    }
    false
}

fn is_packable_source(path: &str) -> bool {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    matches!(
        ext.as_str(),
        "go" | "ts"
            | "tsx"
            | "js"
            | "jsx"
            | "mjs"
            | "py"
            | "rs"
            | "md"
            | "toml"
            | "json"
            | "yaml"
            | "yml"
    )
}

fn excluded_reason(score: i64, path: &str) -> String {
    if is_generated_path(path) {
        return "generated".to_string();
    }
    if score == 0 {
        return "outside goal scope".to_string();
    }
    "low relevance".to_string()
}

fn is_generated_path(path: &str) -> bool {
    let p = path.replace('\\', "/").to_lowercase();
    p.starts_with("dist/")
        || p.contains("/dist/")
        || p.starts_with("build/")
        || p.contains("/build/")
        || p.starts_with("node_modules/")
        || p.contains("/node_modules/")
}

fn is_binary_file(f: &FileInput) -> bool {
    if f.abs_path.is_empty() || f.metadata.size == 0 {
        return false;
    }
    if f.content_head.is_empty() {
        // Match Go: when we can't read the file (no content provided),
        // isBinaryFile returns false. The Go orchestrator reads the
        // file itself; the FFI surface lets the Go side pass the
        // first 512 bytes through if it wants to opt in.
        return false;
    }
    let header_len = f.content_head.len().min(512);
    let header = &f.content_head[..header_len];
    if header.contains(&0u8) {
        return true;
    }
    // utf8.Valid(data) — we approximate with std::str::from_utf8 on
    // the full slice.
    std::str::from_utf8(&f.content_head).is_err()
}

fn append_signal(signals: &mut Vec<String>, signal: String) {
    if signals.iter().any(|s| *s == signal) {
        return;
    }
    signals.push(signal);
}

fn format_relevance_reason(score: i64, signals: &[String]) -> String {
    if signals.is_empty() {
        return format!("score {score}");
    }
    format!("score {score}: {}", signals.join(" + "))
}

fn basename_no_ext(path: &str) -> String {
    let pth = Path::new(path);
    let base = pth
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let stem = match base.rfind('.') {
        Some(i) if i > 0 => &base[..i],
        _ => base,
    };
    stem.to_lowercase()
}

fn parent_path(path: &str) -> String {
    // filepath.ToSlash(filepath.Dir(path)) — the Go side ToSlashes
    // the dir AFTER Dir, so "src/auth/login.ts" -> "src/auth".
    // We normalise backslashes first to match the Go behaviour.
    let normalised = path.replace('\\', "/");
    match normalised.rfind('/') {
        Some(i) => normalised[..i].to_lowercase(),
        None => ".".to_string(),
    }
}

/// Per-corpus precomputed state. Holding this between calls is what
/// the sticky-handle session optimises for.
#[derive(Debug, Clone, Default)]
pub struct RelevanceContext {
    pub goal_keywords: Vec<String>,
    pub goal: String,
    pub budget: i64,
}

impl RelevanceContext {
    pub fn new(goal: &str, budget: i64) -> Self {
        let goal_keywords = extract_goal_keywords(goal);
        Self {
            goal_keywords,
            goal: goal.to_string(),
            budget,
        }
    }
}

/// Stateless score_relevance — mirrors internal/pack/relevance.go.
pub fn score_relevance(
    f: &FileInput,
    goal: &str,
    token_count: i64,
    budget: i64,
) -> RelevanceResult {
    let ctx = RelevanceContext::new(goal, budget);
    score_relevance_with_ctx(f, &ctx, token_count)
}

/// Session-fit score variant — keywords precomputed once, reused for
/// every file in the corpus. The Go session_open call pulls in the
/// goal + budget; subsequent session_score calls only marshal a
/// single FileInput each.
pub fn score_relevance_with_ctx(
    f: &FileInput,
    ctx: &RelevanceContext,
    token_count: i64,
) -> RelevanceResult {
    if is_binary_file(f) {
        return RelevanceResult {
            reason: "binary file".to_string(),
            ..Default::default()
        };
    }

    let keywords = &ctx.goal_keywords;
    let mut score: i64 = 0;
    let mut signals: Vec<String> = Vec::with_capacity(4);
    let mut breakdown = ScoreBreakdown::default();

    if keywords.is_empty() {
        if is_packable_source(&f.path) {
            score = 3;
            append_signal(&mut signals, "source file".to_string());
        }
    } else {
        let base = basename_no_ext(&f.path);
        let parent = parent_path(&f.path);
        for kw in keywords {
            if base.contains(kw) {
                score += 10;
                breakdown.basename += 10;
                append_signal(&mut signals, format!("basename {:?}", kw));
            }
            if parent != "." && parent.contains(kw) {
                score += 5;
                breakdown.path += 5;
                append_signal(&mut signals, format!("path {:?}", kw));
            }
            // Note Go's loop short-circuits per keyword on first
            // matching symbol — we match that with a `for ... break`.
            for sym in &f.metadata.symbols {
                if sym.name.to_lowercase().contains(kw) {
                    score += 8;
                    breakdown.symbol += 8;
                    append_signal(&mut signals, format!("symbol {:?}", sym.name));
                    break;
                }
            }
        }
    }

    let role_score = role_boost(f, keywords);
    if role_score != 0 {
        score += role_score;
        breakdown.role += role_score;
        let sign = if role_score >= 0 { "+" } else { "" };
        append_signal(
            &mut signals,
            format!("role {} {}{}", file_role(f), sign, role_score),
        );
    }
    if ctx.budget > 0 && token_count > ctx.budget / 3 {
        append_signal(
            &mut signals,
            format!(
                "large file: {} tokens > budget/3",
                token_count
            ),
        );
    }

    let mut result = RelevanceResult {
        score,
        signals,
        breakdown,
        ..Default::default()
    };
    if score >= 10 {
        result.tier = "High".to_string();
    } else if score >= 3 {
        result.tier = "Medium".to_string();
    } else {
        result.reason = excluded_reason(score, &f.path);
    }
    if !result.tier.is_empty() {
        result.reason = format_relevance_reason(result.score, &result.signals);
    }
    result
}

/// Sort helper exposed for parity tests — mirrors sortByRelevance.
pub fn sort_by_relevance(files: &mut Vec<(FileInput, RelevanceResult)>) {
    files.sort_by(|a, b| {
        if a.1.score != b.1.score {
            return b.1.score.cmp(&a.1.score);
        }
        a.0.path.cmp(&b.0.path)
    });
}

/// Score every file in `files` against `ctx` and return their results
/// in order. Used by the session.rs `rank_top_n` helper to keep all
/// the per-file work inside Rust.
pub fn score_all(
    ctx: &RelevanceContext,
    files: &[FileInput],
    token_counts: Option<&[i64]>,
) -> Vec<RelevanceResult> {
    let mut out = Vec::with_capacity(files.len());
    for (i, fi) in files.iter().enumerate() {
        let tc = token_counts
            .and_then(|t| t.get(i).copied())
            .unwrap_or(fi.tokens);
        out.push(score_relevance_with_ctx(fi, ctx, tc));
    }
    out
}

/// Compact helper that scores every file in `files` and returns the
/// top-N by (score desc, path asc). Files that did not qualify (tier
/// is empty) are dropped.
pub fn rank_top_n(
    ctx: &RelevanceContext,
    files: &[FileInput],
    token_counts: Option<&[i64]>,
    n: usize,
) -> Vec<(usize, RelevanceResult)> {
    let mut hits: Vec<(usize, RelevanceResult)> = Vec::new();
    for (i, fi) in files.iter().enumerate() {
        let tc = token_counts
            .and_then(|t| t.get(i).copied())
            .unwrap_or(fi.tokens);
        let r = score_relevance_with_ctx(fi, ctx, tc);
        if !r.tier.is_empty() {
            hits.push((i, r));
        }
    }
    hits.sort_by(|a, b| {
        if a.1.score != b.1.score {
            return b.1.score.cmp(&a.1.score);
        }
        files[a.0].path.cmp(&files[b.0].path)
    });
    if hits.len() > n {
        hits.truncate(n);
    }
    hits
}

/// Tiny helper used by the parity tests to expose the unexported
/// stopwords + alias table when debugging divergence.
pub fn debug_keyword_count() -> (usize, usize) {
    let s = stopwords();
    let a = goal_aliases();
    let mut total_aliases = 0usize;
    for (_, list) in a {
        total_aliases += list.len();
    }
    (s.len(), total_aliases)
}

/// Mirror of the Go-private map for the diagnostics path. Returned
/// as a sorted Vec for stable comparison.
pub fn debug_alias_map() -> HashMap<String, Vec<String>> {
    let mut m = HashMap::new();
    for (jp, aliases) in goal_aliases() {
        m.insert((*jp).to_string(), aliases.iter().map(|s| s.to_string()).collect());
    }
    m
}

#[allow(dead_code)]
fn _ensure_symbol_type_used(_s: &SymbolInput) {}
