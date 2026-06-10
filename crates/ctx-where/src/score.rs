// crates/ctx-where/src/score.rs
//
// Port of internal/where.go's scoring + keyword extraction logic.
// Mirrors the Go source semantics line-by-line so the parity goldens
// match byte-for-byte.

use std::collections::{BTreeMap, HashMap, HashSet};

use once_cell::sync::Lazy;
use regex::Regex;

use crate::types::{KeywordSet, Match, Result, ScoreBreakdown};

// Matches Go's queryTokenRE: CJK runs OR ASCII identifier chunks.
static QUERY_TOKEN_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"[\p{Han}\p{Hiragana}\p{Katakana}]+|[A-Za-z0-9_-]+").expect("query token re")
});

static QUERY_STOPWORDS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        "a", "an", "and", "for", "in", "of", "on", "or", "the", "to", "with",
        "どこ", "どれ", "です", "する", "した", "して", "ある", "いる",
        "から", "まで", "の", "は", "を",
    ]
    .into_iter()
    .collect()
});

/// Mirrors `where.extractKeywords`. Returns lower-cased keywords in
/// first-occurrence order with stop-words and ≤1-rune tokens filtered.
pub fn extract_keywords(query: &str) -> Vec<String> {
    let lower = query.to_lowercase();
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<String> = Vec::new();
    for m in QUERY_TOKEN_RE.find_iter(&lower) {
        let mut token = m.as_str().trim_matches(|c| c == '_' || c == '-').to_string();
        if token.is_empty() {
            continue;
        }
        // Drop ≤1-rune tokens.
        if token.chars().count() < 2 {
            continue;
        }
        if QUERY_STOPWORDS.contains(token.as_str()) {
            continue;
        }
        if seen.contains(&token) {
            continue;
        }
        seen.insert(token.clone());
        // Take ownership.
        out.push(std::mem::take(&mut token));
    }
    out
}

/// Mirrors `where.splitIdentifier`. Output tokens are lower-cased and
/// length-≥2-rune-filtered.
pub fn split_identifier(s: &str) -> Vec<String> {
    if s.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let emit = |tok: &str, out: &mut Vec<String>| {
        if tok.chars().count() < 2 {
            return;
        }
        out.push(tok.to_lowercase());
    };
    // First split on _ and -.
    for segment in s.split(|c| c == '_' || c == '-') {
        let runes: Vec<char> = segment.chars().collect();
        if runes.is_empty() {
            continue;
        }
        let class = |r: char| -> i32 {
            if ('a'..='z').contains(&r) {
                0
            } else if ('A'..='Z').contains(&r) {
                1
            } else if ('0'..='9').contains(&r) {
                2
            } else {
                3
            }
        };
        let mut start: usize = 0;
        let mut i: usize = 1;
        while i < runes.len() {
            let prev = class(runes[i - 1]);
            let cur = class(runes[i]);
            let mut boundary = false;
            // Note: Go uses match-by-switch-style cases. We mirror order
            // because the acronym case has a `continue` side effect.
            if prev == 0 && cur == 1 {
                boundary = true;
            } else if prev == 1 && cur == 0 && (i as i64 - start as i64) >= 2 {
                let out_split = i - 1;
                if out_split > start {
                    let s: String = runes[start..out_split].iter().collect();
                    emit(&s, &mut out);
                    start = out_split;
                }
                i += 1;
                continue;
            } else if (prev == 2) != (cur == 2) {
                boundary = true;
            } else if (prev == 3) != (cur == 3) {
                boundary = true;
            }
            if boundary {
                let s: String = runes[start..i].iter().collect();
                emit(&s, &mut out);
                start = i;
            }
            i += 1;
        }
        if start < runes.len() {
            let s: String = runes[start..].iter().collect();
            emit(&s, &mut out);
        }
    }
    out
}

const SYNONYM_DISCOUNT: f64 = 0.7;

/// SymbolInfo mirrors a slice of model.Symbol that the dispatcher
/// passes through FFI. We only need Name + Line + Kind.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SymbolInfo {
    pub name: String,
    pub kind: String,
    pub line: i64,
}

/// FileForScore captures everything score_file needs about a file:
/// repo-relative path, pre-extracted symbols, and content lines.
#[derive(Debug, Clone, Default)]
pub struct FileForScore<'a> {
    pub path: String,
    pub symbols: &'a [SymbolInfo],
    pub lines: &'a [String],
}

fn slash_path(s: &str) -> String {
    s.replace('\\', "/")
}

fn basename(p: &str) -> String {
    let s = slash_path(p);
    s.rsplit('/').next().unwrap_or("").to_string()
}

fn dirname(p: &str) -> String {
    let s = slash_path(p);
    if let Some(idx) = s.rfind('/') {
        if idx == 0 {
            return "/".into();
        }
        s[..idx].to_string()
    } else {
        ".".into()
    }
}

fn trim_ext(name: &str) -> String {
    if let Some(idx) = name.rfind('.') {
        if idx > 0 {
            return name[..idx].to_string();
        }
    }
    name.to_string()
}

/// Mirrors `where.scoreFile` — KeywordSet-free version.
pub fn score_file(fi: &FileForScore, keywords: &[String], context_n: usize) -> Result {
    let kw_sets: Vec<KeywordSet> = keywords
        .iter()
        .map(|kw| KeywordSet {
            original: kw.clone(),
            synonyms: Vec::new(),
        })
        .collect();
    score_file_with_sets(fi, &kw_sets, context_n)
}

/// Mirrors `where.scoreFileWithSets`. Original-keyword matches earn the
/// full score; synonym matches earn `SYNONYM_DISCOUNT` × base.
pub fn score_file_with_sets(
    fi: &FileForScore,
    kw_sets: &[KeywordSet],
    context_n: usize,
) -> Result {
    // Build the token universe and per-token discount flag.
    let mut token_map: HashMap<String, bool> = HashMap::new();
    for ks in kw_sets {
        token_map.entry(ks.original.clone()).or_insert(false);
        for syn in &ks.synonyms {
            token_map.entry(syn.clone()).or_insert(true);
        }
    }
    let mut all_tokens: Vec<String> = token_map.keys().cloned().collect();
    all_tokens.sort();

    let add_score = |token: &str, base: i64| -> i64 {
        if *token_map.get(token).unwrap_or(&false) {
            (base as f64 * SYNONYM_DISCOUNT + 0.5).floor() as i64
        } else {
            base
        }
    };

    let mut result = Result {
        path: slash_path(&fi.path),
        ..Default::default()
    };
    let mut reasons: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();
    let mut breakdown = ScoreBreakdown::default();

    let bn = basename(&fi.path);
    let base = trim_ext(&bn).to_lowercase();
    let dir = dirname(&fi.path).to_lowercase();

    let base_raw = trim_ext(&bn);
    let base_subs = split_identifier(&base_raw);
    let mut base_hit: HashMap<String, bool> = HashMap::new();

    for kw in &all_tokens {
        if base.contains(kw) {
            let pts = add_score(kw, 12);
            result.score += pts;
            breakdown.basename += pts;
            append_unique(reasons.entry("basename match").or_default(), kw);
            base_hit.insert(kw.clone(), true);
        }
        if dir != "." && dir.contains(kw) {
            let pts = add_score(kw, 6);
            result.score += pts;
            breakdown.path += pts;
            append_unique(reasons.entry("path match").or_default(), kw);
        }
    }

    for kw in &all_tokens {
        if *base_hit.get(kw).unwrap_or(&false) {
            continue;
        }
        for sub in &base_subs {
            if sub == kw {
                let pts = add_score(kw, 6);
                result.score += pts;
                breakdown.splitname += pts;
                append_unique(reasons.entry("splitname match").or_default(), sub);
                break;
            }
        }
    }

    // Symbol Contains pass.
    for sym in fi.symbols {
        let name = sym.name.to_lowercase();
        for kw in &all_tokens {
            if !name.contains(kw) {
                continue;
            }
            let pts = add_score(kw, 10);
            result.score += pts;
            breakdown.symbol += pts;
            let text = symbol_text(fi.lines, sym);
            let (before, after) = context_lines(fi.lines, sym.line as i64, context_n as i64);
            result.matches.push(Match {
                line: sym.line,
                column: column_of(&text, &sym.name) as i64,
                kind: "symbol".into(),
                text,
                before,
                after,
            });
            append_unique(reasons.entry("symbol match").or_default(), &sym.name);
            break;
        }
    }

    // Symbol splitname pass.
    for sym in fi.symbols {
        let subs = split_identifier(&sym.name);
        if subs.is_empty() {
            continue;
        }
        for kw in &all_tokens {
            for sub in &subs {
                if sub == kw {
                    let pts = add_score(kw, 6);
                    result.score += pts;
                    breakdown.splitname += pts;
                    append_unique(reasons.entry("splitname match").or_default(), sub);
                    break;
                }
            }
        }
    }

    // Content pass.
    for (line_no, line) in fi.lines.iter().enumerate() {
        let lower = line.to_lowercase();
        for kw in &all_tokens {
            if let Some(idx) = lower.find(kw) {
                let pts = add_score(kw, 3);
                result.score += pts;
                breakdown.content += pts;
                let (before, after) =
                    context_lines(fi.lines, (line_no + 1) as i64, context_n as i64);
                result.matches.push(Match {
                    line: (line_no + 1) as i64,
                    column: (idx + 1) as i64,
                    kind: "content".into(),
                    text: line.trim().to_string(),
                    before,
                    after,
                });
                append_unique(reasons.entry("content match").or_default(), kw);
                break;
            }
        }
    }

    result.reason = format_reasons(&reasons);
    if result.score > 0 {
        result.score_breakdown = Some(breakdown);
    }
    if result.matches.is_empty() && result.score > 0 {
        result.matches.push(Match {
            line: 1,
            column: 1,
            kind: "path".into(),
            text: result.path.clone(),
            ..Default::default()
        });
    }
    result
}

fn append_unique(values: &mut Vec<String>, value: &str) {
    for existing in values.iter() {
        if existing == value {
            return;
        }
    }
    values.push(value.to_string());
}

fn format_reasons(reasons: &BTreeMap<&'static str, Vec<String>>) -> String {
    // Match the Go ordering exactly.
    let order = [
        "symbol match",
        "splitname match",
        "content match",
        "basename match",
        "path match",
    ];
    let mut parts = Vec::new();
    for key in order {
        if let Some(values) = reasons.get(key) {
            if !values.is_empty() {
                parts.push(format!("{key}: {}", values.join(", ")));
            }
        }
    }
    parts.join("; ")
}

/// Mirrors `where.symbolText`.
fn symbol_text(lines: &[String], sym: &SymbolInfo) -> String {
    let line = sym.line;
    if line > 0 && (line as usize) <= lines.len() {
        return lines[(line - 1) as usize].trim().to_string();
    }
    format!("{} {}", sym.kind, sym.name)
}

/// Mirrors `where.columnOf`.
fn column_of(text: &str, needle: &str) -> usize {
    let t = text.to_lowercase();
    let n = needle.to_lowercase();
    if let Some(idx) = t.find(&n) {
        idx + 1
    } else {
        1
    }
}

/// Mirrors `where.contextLines`. Output vectors are EMPTY when nothing
/// should be returned — the serde `skip_serializing_if = "is_empty_vec"`
/// drops them from the JSON, matching Go's omitempty.
pub fn context_lines(lines: &[String], line_no: i64, n: i64) -> (Vec<String>, Vec<String>) {
    if n <= 0 || line_no <= 0 || (line_no as usize) > lines.len() {
        return (Vec::new(), Vec::new());
    }
    let start = ((line_no - 1) - n).max(0) as usize;
    let end = ((line_no + n) as usize).min(lines.len());
    let before: Vec<String> = lines[start..(line_no - 1) as usize].to_vec();
    let after: Vec<String> = lines[line_no as usize..end].to_vec();
    (before, after)
}

/// Mirrors `where.hasAllKeywordSets`. Used for AND-filtering.
pub fn has_all_keyword_sets(
    sets: &[KeywordSet],
    fi: &FileForScore,
) -> bool {
    let bn = basename(&fi.path);
    let base = trim_ext(&bn).to_lowercase();
    let dir = dirname(&fi.path).to_lowercase();

    let token_matches_file = |t: &str| -> bool {
        if base.contains(t) {
            return true;
        }
        if dir != "." && dir.contains(t) {
            return true;
        }
        for sym in fi.symbols {
            if sym.name.to_lowercase().contains(t) {
                return true;
            }
        }
        for line in fi.lines {
            if line.to_lowercase().contains(t) {
                return true;
            }
        }
        false
    };

    for ks in sets {
        let mut found = token_matches_file(&ks.original);
        if !found {
            for syn in &ks.synonyms {
                if token_matches_file(syn) {
                    found = true;
                    break;
                }
            }
        }
        if !found {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_keywords_lowercases_and_dedupes() {
        let out = extract_keywords("Foo BAR foo");
        assert_eq!(out, vec!["foo", "bar"]);
    }

    #[test]
    fn extract_keywords_drops_stopwords() {
        let out = extract_keywords("the user and admin");
        assert_eq!(out, vec!["user", "admin"]);
    }

    #[test]
    fn split_identifier_camel() {
        assert_eq!(
            split_identifier("getUserByID"),
            vec!["get", "user", "by", "id"]
        );
    }

    #[test]
    fn split_identifier_acronym() {
        assert_eq!(
            split_identifier("parseHTTPHeader"),
            vec!["parse", "http", "header"]
        );
    }

    #[test]
    fn split_identifier_snake() {
        assert_eq!(
            split_identifier("user_repository"),
            vec!["user", "repository"]
        );
    }

    #[test]
    fn split_identifier_drops_short_tokens() {
        // v2User → "v", "2", "User" — first two dropped (len < 2).
        assert_eq!(split_identifier("v2User"), vec!["user"]);
    }

    #[test]
    fn score_basename_match() {
        let lines = vec!["package pack".to_string(), "func foo() {}".to_string()];
        let fi = FileForScore {
            path: "internal/pack/relevance.go".into(),
            symbols: &[],
            lines: &lines,
        };
        let kws = vec!["relevance".to_string()];
        let r = score_file(&fi, &kws, 0);
        assert!(r.score > 0);
        let breakdown = r.score_breakdown.as_ref().unwrap();
        assert!(breakdown.basename >= 12);
    }
}
