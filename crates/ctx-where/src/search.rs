// crates/ctx-where/src/search.rs
//
// Port of internal/where.SearchWithOptions and SuggestSimilar. The Rust
// port operates on a pre-walked file list provided over FFI: this keeps
// the walker + tree-sitter symbol extraction on the Go side and focuses
// the Rust hot path on the LOOKUP_HEAVY scoring loop (the part we
// expect to win on).

use std::collections::BTreeMap;

use rayon::prelude::*;

use crate::levenshtein::levenshtein;
use crate::score::{
    extract_keywords, has_all_keyword_sets, score_file_literal, score_file_with_sets, FileForScore,
    SymbolInfo,
};
use crate::types::{KeywordSet, Result as SearchResult, ScoreBreakdown, Suggestion};

/// SymbolInput mirrors the JSON shape the Go dispatcher passes through:
/// pre-extracted model.Symbol values.
pub type SymbolInput = SymbolInfo;

/// FileInput is the per-file payload sent across FFI: repo-relative
/// path, pre-extracted symbols, and pre-read content lines. The Go
/// dispatcher fills these in via the existing walk + symbols pipeline.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct FileInput {
    pub path: String,
    pub is_dir: bool,
    #[serde(default, deserialize_with = "null_as_empty_vec")]
    pub symbols: Vec<SymbolInfo>,
    /// Pre-read file content as lines (already filtered for binary /
    /// non-UTF-8). Empty when the dispatcher decided not to read.
    #[serde(default, deserialize_with = "null_as_empty_vec")]
    pub lines: Vec<String>,
}

fn null_as_empty_vec<'de, D, T>(d: D) -> std::result::Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    use serde::Deserialize;
    let v: Option<Vec<T>> = Option::deserialize(d)?;
    Ok(v.unwrap_or_default())
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Options {
    #[serde(default)]
    pub limit: i64,
    #[serde(default)]
    pub context_n: i64,
    #[serde(default)]
    pub require_all: bool,
    #[serde(default)]
    pub regex: String, // optional regex pattern; empty = none
    #[serde(default)]
    pub literal: String, // optional exact pattern; empty = none
    #[serde(default, deserialize_with = "null_as_empty_map")]
    pub synonyms: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub explain: bool,
}

fn null_as_empty_map<'de, D, K, V>(d: D) -> std::result::Result<BTreeMap<K, V>, D::Error>
where
    D: serde::Deserializer<'de>,
    K: serde::Deserialize<'de> + Ord,
    V: serde::Deserialize<'de>,
{
    use serde::Deserialize;
    let v: Option<BTreeMap<K, V>> = Option::deserialize(d)?;
    Ok(v.unwrap_or_default())
}

fn expand_keywords(
    keywords: &[String],
    synonyms: &BTreeMap<String, Vec<String>>,
) -> Vec<KeywordSet> {
    keywords
        .iter()
        .map(|kw| KeywordSet {
            original: kw.clone(),
            synonyms: synonyms.get(kw).cloned().unwrap_or_default(),
        })
        .collect()
}

fn merge_score_breakdown(base: &mut Option<ScoreBreakdown>, extra: Option<ScoreBreakdown>) {
    let Some(extra) = extra else {
        return;
    };
    if base.is_none() {
        *base = Some(ScoreBreakdown::default());
    }
    let Some(base) = base.as_mut() else {
        return;
    };
    base.basename += extra.basename;
    base.symbol += extra.symbol;
    base.splitname += extra.splitname;
    base.path += extra.path;
    base.content += extra.content;
    base.literal += extra.literal;
}

fn merge_search_result(base: &mut SearchResult, extra: SearchResult) {
    base.score += extra.score;
    merge_score_breakdown(&mut base.score_breakdown, extra.score_breakdown);

    if !extra.reason.is_empty() {
        if base.reason.is_empty() {
            base.reason = extra.reason;
        } else {
            base.reason = format!("{}; {}", extra.reason, base.reason);
        }
    }

    let mut seen: std::collections::HashSet<(i64, i64)> =
        base.matches.iter().map(|m| (m.line, m.column)).collect();
    for m in extra.matches {
        if seen.insert((m.line, m.column)) {
            base.matches.push(m);
        }
    }
}

/// Mirrors `where.SearchWithOptions` minus the walk + symbols
/// extraction (those happen in Go before FFI).
pub fn search_with_options(files: &[FileInput], query: &str, opts: &Options) -> Vec<SearchResult> {
    let mut limit = opts.limit;
    if limit <= 0 {
        limit = 10;
    }
    let context_n = if opts.context_n < 0 {
        0
    } else {
        opts.context_n as usize
    };
    let regex_opt = if opts.regex.is_empty() {
        None
    } else {
        regex::Regex::new(&opts.regex).ok()
    };
    let literal_opt = if opts.literal.is_empty() {
        None
    } else {
        Some(opts.literal.as_str())
    };

    let mut keywords = extract_keywords(query);
    if keywords.is_empty() && regex_opt.is_none() && literal_opt.is_none() {
        keywords = vec![query.to_lowercase()];
    }
    let kw_sets = expand_keywords(&keywords, &opts.synonyms);

    // Per-file scoring is independent and read-only over the shared keyword
    // sets / regex, so it runs in parallel. The collection order is irrelevant
    // — the `sort_by(score, path)` below restores the deterministic order,
    // keeping byte-parity with the sequential scan.
    let mut results: Vec<SearchResult> = files
        .par_iter()
        .filter_map(|fi| {
            if fi.is_dir || fi.path == "." {
                return None;
            }
            let file = FileForScore {
                path: fi.path.clone(),
                symbols: &fi.symbols,
                lines: &fi.lines,
            };
            let mut result = score_file_with_sets(&file, &kw_sets, context_n);

            if opts.require_all && !kw_sets.is_empty() && !has_all_keyword_sets(&kw_sets, &file) {
                return None;
            }

            if let Some(literal) = literal_opt {
                let literal_result = score_file_literal(&file, literal, context_n);
                if literal_result.score == 0 {
                    return None;
                }
                merge_search_result(&mut result, literal_result);
            }

            if let Some(re) = &regex_opt {
                let mut regex_matches: Vec<crate::types::Match> = Vec::new();
                for (i, line) in fi.lines.iter().enumerate() {
                    if let Some(m) = re.find(line) {
                        let (before, after) = crate::score::context_lines(
                            &fi.lines,
                            (i + 1) as i64,
                            context_n as i64,
                        );
                        regex_matches.push(crate::types::Match {
                            line: (i + 1) as i64,
                            column: (m.start() + 1) as i64,
                            kind: "content-regex".into(),
                            text: line.trim().to_string(),
                            before,
                            after,
                        });
                    }
                }
                if regex_matches.is_empty() {
                    return None;
                }
                // Dedup against existing matches.
                use std::collections::HashSet;
                let existing: HashSet<(i64, i64)> =
                    result.matches.iter().map(|m| (m.line, m.column)).collect();
                for rm in regex_matches {
                    if existing.contains(&(rm.line, rm.column)) {
                        continue;
                    }
                    result.matches.push(rm);
                    result.score += 3;
                    if result.score_breakdown.is_none() {
                        result.score_breakdown = Some(Default::default());
                    }
                    if let Some(b) = result.score_breakdown.as_mut() {
                        b.content += 3;
                    }
                }
            }

            (result.score > 0).then_some(result)
        })
        .collect();

    results.sort_by(|a, b| {
        if a.score != b.score {
            return b.score.cmp(&a.score);
        }
        a.path.cmp(&b.path)
    });
    if results.len() > limit as usize {
        results.truncate(limit as usize);
    }

    if opts.explain && !kw_sets.is_empty() {
        let mut applied: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut expanded: Vec<Vec<String>> = Vec::new();
        for ks in &kw_sets {
            if !ks.synonyms.is_empty() {
                applied.insert(ks.original.clone(), ks.synonyms.clone());
            }
            let mut row = Vec::with_capacity(1 + ks.synonyms.len());
            row.push(ks.original.clone());
            row.extend(ks.synonyms.iter().cloned());
            expanded.push(row);
        }
        if let Some(first) = results.first_mut() {
            first.synonyms_applied = Some(applied);
            first.expanded_keywords = Some(expanded);
        }
    }

    results
}

/// Mirrors `where.SuggestSimilar` minus the walk + symbols extraction.
pub fn suggest_similar(files: &[FileInput], query: &str, limit: i64) -> Vec<Suggestion> {
    if limit <= 0 {
        return Vec::new();
    }
    let mut keywords = extract_keywords(query);
    if keywords.is_empty() {
        let trimmed = query.trim().to_lowercase();
        if trimmed.is_empty() {
            return Vec::new();
        }
        keywords = vec![trimmed];
    }

    // Build candidate universe.
    struct Candidate {
        name: String,
        lowered: String,
        kind: String,
        path: String,
        name_chars: Vec<char>,
    }
    let mut seen = std::collections::HashSet::new();
    let mut candidates: Vec<Candidate> = Vec::new();
    let add = |name: &str,
               kind: &str,
               path: &str,
               seen: &mut std::collections::HashSet<String>,
               cs: &mut Vec<Candidate>| {
        if name.is_empty() {
            return;
        }
        let lowered = name.to_lowercase();
        if seen.contains(&lowered) {
            return;
        }
        seen.insert(lowered.clone());
        let chars: Vec<char> = lowered.chars().collect();
        cs.push(Candidate {
            name: name.to_string(),
            lowered,
            kind: kind.to_string(),
            path: path.to_string(),
            name_chars: chars,
        });
    };

    for fi in files {
        if fi.is_dir || fi.path == "." {
            continue;
        }
        let path = fi.path.replace('\\', "/");
        // basename sans extension.
        let bn = path.rsplit('/').next().unwrap_or("").to_string();
        let base = match bn.rfind('.') {
            Some(idx) if idx > 0 => bn[..idx].to_string(),
            _ => bn.clone(),
        };
        add(&base, "basename", &path, &mut seen, &mut candidates);
        for sym in &fi.symbols {
            add(&sym.name, "symbol", &path, &mut seen, &mut candidates);
        }
    }

    let mut best_by_name: BTreeMap<String, Suggestion> = BTreeMap::new();
    for kw in &keywords {
        let kw_chars: Vec<char> = kw.chars().collect();
        let mut threshold = kw_chars.len() / 3;
        if threshold < 2 {
            threshold = 2;
        }
        for c in &candidates {
            let len_diff = (kw_chars.len() as i64 - c.name_chars.len() as i64).abs() as usize;
            if len_diff > threshold {
                continue;
            }
            let d = levenshtein(&kw_chars, &c.name_chars);
            if d == 0 {
                continue;
            }
            if d > threshold {
                continue;
            }
            let entry = best_by_name.entry(c.lowered.clone()).or_insert(Suggestion {
                name: c.name.clone(),
                kind: c.kind.clone(),
                path: c.path.clone(),
                distance: d as i64,
            });
            if (d as i64) < entry.distance {
                entry.name = c.name.clone();
                entry.kind = c.kind.clone();
                entry.path = c.path.clone();
                entry.distance = d as i64;
            }
        }
    }

    if best_by_name.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<Suggestion> = best_by_name.into_values().collect();
    out.sort_by(|a, b| {
        if a.distance != b.distance {
            return a.distance.cmp(&b.distance);
        }
        if a.name.len() != b.name.len() {
            return a.name.len().cmp(&b.name.len());
        }
        a.name.cmp(&b.name)
    });
    if out.len() > limit as usize {
        out.truncate(limit as usize);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mkfile(path: &str, lines: Vec<&str>, syms: Vec<(&str, i64)>) -> FileInput {
        FileInput {
            path: path.into(),
            is_dir: false,
            symbols: syms
                .into_iter()
                .map(|(n, l)| SymbolInfo {
                    name: n.into(),
                    kind: "function".into(),
                    line: l,
                })
                .collect(),
            lines: lines.into_iter().map(String::from).collect(),
        }
    }

    #[test]
    fn search_basic_basename_match() {
        let files = vec![mkfile(
            "internal/pack/relevance.go",
            vec!["package pack", "func score() {}"],
            vec![],
        )];
        let r = search_with_options(&files, "relevance", &Options::default());
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].path, "internal/pack/relevance.go");
    }

    #[test]
    fn search_orders_by_score_then_path() {
        let files = vec![
            mkfile("b/relevance.go", vec!["// b"], vec![]),
            mkfile("a/relevance.go", vec!["// a"], vec![]),
        ];
        let r = search_with_options(&files, "relevance", &Options::default());
        // Ties broken by path lex.
        assert_eq!(r[0].path, "a/relevance.go");
    }

    #[test]
    fn suggest_similar_finds_typo() {
        let files = vec![mkfile(
            "session.go",
            vec!["// noop"],
            vec![("SaveSession", 2)],
        )];
        let s = suggest_similar(&files, "SaveSessoin", 5); // typo
        assert!(!s.is_empty());
        assert_eq!(s[0].name, "SaveSession");
    }

    #[test]
    fn suggest_similar_empty_for_unrelated() {
        let files = vec![mkfile("foo.go", vec!["// noop"], vec![("Bar", 2)])];
        let s = suggest_similar(&files, "xyz", 5);
        assert!(s.is_empty());
    }
}
