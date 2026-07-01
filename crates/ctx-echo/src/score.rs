// crates/ctx-echo/src/score.rs
//
// Port of internal/echo/score.go. The arithmetic order matters for
// floating-point parity — both Go and Rust use IEEE 754 f64, but
// reordering the multiplications/divisions can produce different
// last-bit results. We replicate the Go expression structure verbatim:
//
//     numer = float64(count) * (bm25K1 + 1.0)
//     denom = float64(count) + bm25K1 * (1.0 - bm25B + bm25B *
//             float64(c.TokenLen)/avgLen)
//     s    += idf[t] * (numer / denom)
//
// The same applies to idf:
//
//     idf[t] = math.Log(1.0 + (N - float64(dfT) + 0.5) / (float64(dfT) + 0.5))
//
// We mirror Go's stable-sort tiebreaker (score desc, then path asc,
// then line_start asc).

use crate::types::{Chunk, ScoredChunk};
use std::collections::{BTreeMap, HashMap, HashSet};

pub const BM25_K1: f64 = 1.5;
pub const BM25_B: f64 = 0.75;

/// Compute BM25 against `chunks` for the given `goal_tokens`. The
/// output is sorted by score descending and stable on path/line for
/// ties. Mirrors `score()` in Go.
pub fn score_chunks(chunks: &[Chunk], goal_tokens: &[String]) -> Vec<ScoredChunk> {
    if chunks.is_empty() || goal_tokens.is_empty() {
        let mut out: Vec<ScoredChunk> = Vec::with_capacity(chunks.len());
        for c in chunks.iter() {
            out.push(ScoredChunk {
                chunk: c.clone(),
                score: 0.0,
                matches: BTreeMap::new(),
            });
        }
        return out;
    }

    // Document frequency per goal token. Initialise to 0 so even
    // unseen tokens get an idf computed below (matches Go's
    // `df[t] = 0` then `df[t]++` walk).
    //
    // We carry a HashSet of want-tokens for O(1) "is this a goal
    // token?" lookups; the df map keys are the canonical goal-token
    // set.
    let want_set: HashSet<&str> = goal_tokens.iter().map(|s| s.as_str()).collect();
    let mut df: HashMap<String, i32> = HashMap::with_capacity(goal_tokens.len());
    for t in goal_tokens.iter() {
        df.entry(t.clone()).or_insert(0);
    }

    for c in chunks.iter() {
        // One increment per (chunk, token) — use `seen` to dedupe
        // within a chunk, matching Go's `seen := make(map[string]bool)`.
        let mut seen: HashSet<&str> = HashSet::new();
        for tok in c.tokens.iter() {
            if want_set.contains(tok.as_str()) && !seen.contains(tok.as_str()) {
                *df.get_mut(tok).unwrap() += 1;
                seen.insert(tok);
            }
        }
    }

    // Average chunk length.
    let mut sum_len: i64 = 0;
    for c in chunks.iter() {
        sum_len += c.token_len as i64;
    }
    let mut avg_len: f64 = 1.0;
    if !chunks.is_empty() {
        avg_len = sum_len as f64 / chunks.len() as f64;
        if avg_len == 0.0 {
            avg_len = 1.0;
        }
    }

    // IDF per goal token. Mirrors Go's smoothing formula exactly.
    let n = chunks.len() as f64;
    let mut idf: HashMap<String, f64> = HashMap::with_capacity(df.len());
    for (t, df_t) in df.iter() {
        let df_t_f = *df_t as f64;
        idf.insert(t.clone(), (1.0 + (n - df_t_f + 0.5) / (df_t_f + 0.5)).ln());
    }

    let mut out: Vec<ScoredChunk> = Vec::with_capacity(chunks.len());
    for c in chunks.iter() {
        // tf per goal token in this chunk.
        let mut tf: HashMap<String, i32> = HashMap::new();
        for tok in c.tokens.iter() {
            if idf.contains_key(tok.as_str()) {
                *tf.entry(tok.clone()).or_insert(0) += 1;
            }
        }

        let mut s: f64 = 0.0;
        let mut matches: BTreeMap<String, i32> = BTreeMap::new();
        // PARITY (BM25 score f64 last-bit divergence vs Go — TWO causes,
        // both bounded to ~1 ULP / 1.3e-16 relative; verified empirically in
        // ctx-cli tests/parity.rs's echo suite):
        //
        //   1. DOMINANT, deterministic: `idf` below is computed with
        //      f64::ln, while Go uses math.Log. The two stdlib natural-log
        //      implementations disagree in the last bit, so every score
        //      inherits a 1-ULP idf difference even when the summation has a
        //      single term (no associativity in play). This is stable across
        //      runs on each side but Go≠Rust.
        //
        //   2. SECONDARY, run-to-run in Go only: when a chunk matches 2+
        //      goal tokens, Go iterates `tf` (a map) in randomised order and
        //      f64 addition is non-associative, so Go's OWN output varies in
        //      the last ULP across runs. The Rust BTreeMap/HashMap order is
        //      fixed, so Rust is deterministic; it just won't match Go's
        //      arbitrary per-run sum.
        //
        // markdown/plain renderers round to %.2f/%.4f and ARE byte-identical
        // to Go. JSON exposes the raw f64, so JSON parity uses a tight 1e-12
        // tolerance on the score field only — see assert_echo_json_parity_in.
        for (t, count) in tf.iter() {
            if *count == 0 {
                continue;
            }
            matches.insert(t.clone(), *count);
            let cnt = *count as f64;
            let numer = cnt * (BM25_K1 + 1.0);
            let denom = cnt + BM25_K1 * (1.0 - BM25_B + BM25_B * (c.token_len as f64) / avg_len);
            s += idf.get(t).copied().unwrap_or(0.0) * (numer / denom);
        }
        out.push(ScoredChunk {
            chunk: c.clone(),
            score: s,
            matches,
        });
    }

    // Sort: score desc, then source_path asc, then line_start asc.
    // Use stable sort so equal-score equal-path equal-line entries
    // retain their input order (matches Go's sort.SliceStable).
    out.sort_by(|a, b| {
        if a.score != b.score {
            return b
                .score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal);
        }
        if a.chunk.source_path != b.chunk.source_path {
            return a.chunk.source_path.cmp(&b.chunk.source_path);
        }
        a.chunk.line_start.cmp(&b.chunk.line_start)
    });
    out
}

/// Mirrors `coverage()` in Go. Returns (coverage_score, num_covered).
pub fn coverage(scored: &[ScoredChunk], goal_tokens: &[String], top_k: i32) -> (f64, i32) {
    if goal_tokens.is_empty() {
        return (0.0, 0);
    }

    // Which goal tokens appear at least once across the entire pack?
    let mut present: HashSet<&str> = HashSet::new();
    let mut covered: i32 = 0;
    for sc in scored.iter() {
        for tok in sc.matches.keys() {
            present.insert(tok.as_str());
        }
        if sc.score > 0.0 {
            covered += 1;
        }
    }
    let token_coverage = present.len() as f64 / unique_tokens(goal_tokens) as f64;

    let mut top_k_usize = top_k as i64;
    if top_k_usize <= 0 || top_k_usize > scored.len() as i64 {
        top_k_usize = scored.len() as i64;
    }
    let top_k = top_k_usize as usize;

    let mut top_sum: f64 = 0.0;
    let mut total_sum: f64 = 0.0;
    for (i, sc) in scored.iter().enumerate() {
        if i < top_k {
            top_sum += sc.score;
        }
        total_sum += sc.score;
    }
    let concentration = if total_sum > 0.0 {
        top_sum / total_sum
    } else {
        0.0
    };
    (token_coverage * concentration, covered)
}

/// Mirrors `spreadIndex()` in Go — population Gini on per-chunk hit
/// counts; returns 1 - gini.
pub fn spread_index(scored: &[ScoredChunk]) -> f64 {
    let mut counts: Vec<i32> = Vec::new();
    for sc in scored.iter() {
        let hits: i32 = sc.matches.values().sum();
        if hits > 0 {
            counts.push(hits);
        }
    }
    if counts.len() < 2 {
        return 0.0;
    }
    counts.sort();
    let n = counts.len() as f64;
    let mut cum: f64 = 0.0;
    let mut total: f64 = 0.0;
    for (i, v) in counts.iter().enumerate() {
        cum += (i as f64 + 1.0) * (*v as f64);
        total += *v as f64;
    }
    if total == 0.0 {
        return 0.0;
    }
    let mut gini = (2.0 * cum) / (n * total) - (n + 1.0) / n;
    if gini < 0.0 {
        gini = 0.0;
    }
    if gini > 1.0 {
        gini = 1.0;
    }
    1.0 - gini
}

pub fn unique_tokens(tokens: &[String]) -> usize {
    let set: HashSet<&str> = tokens.iter().map(|s| s.as_str()).collect();
    if set.is_empty() {
        return 1;
    }
    set.len()
}

pub fn unique_token_list(tokens: &[String]) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::with_capacity(tokens.len());
    let mut out: Vec<String> = Vec::with_capacity(tokens.len());
    for t in tokens.iter() {
        if seen.insert(t.clone()) {
            out.push(t.clone());
        }
    }
    out.sort();
    out
}
