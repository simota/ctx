// crates/ctx-echo/src/evaluate.rs
//
// Port of internal/echo/echo.go's `Evaluate` orchestrator. The pipeline
// is straight-line: ChunkPack -> Tokenize(goal) -> score -> coverage +
// spread -> build EchoResult.

use crate::chunk::chunk_pack;
use crate::score::{coverage, score_chunks, spread_index, unique_token_list};
use crate::tokenize::tokenize;
use crate::types::{ChunkStrategy, EchoResult, FileConcStats, Options, TopEntry};

use std::collections::HashSet;

/// Mirrors `echo.Evaluate`. The `pack_path` argument is only used for
/// display; reading happens at the call boundary.
pub fn evaluate(pack_path: &str, pack_body: &str, opts_in: &Options) -> EchoResult {
    // Apply Go's defaults: top<=0 -> 10, chunk_by="" -> paragraph,
    // chunk_size<=0 -> 40. We clone opts_in into a mutable local.
    let mut opts = opts_in.clone();
    if opts.top <= 0 {
        opts.top = 10;
    }
    let strategy = ChunkStrategy::from_str(if opts.chunk_by.is_empty() {
        "paragraph"
    } else {
        opts.chunk_by.as_str()
    });
    if opts.chunk_size <= 0 {
        opts.chunk_size = 40;
    }

    let chunks = chunk_pack(pack_body, strategy, opts.chunk_size);
    let goal_tokens = tokenize(&opts.goal);
    let scored = score_chunks(&chunks, &goal_tokens);
    let (cov_score, covered) = coverage(&scored, &goal_tokens, opts.top);
    let spread = spread_index(&scored);

    let mut res = EchoResult {
        pack_file: pack_path.to_string(),
        goal: opts.goal.clone(),
        chunks_total: chunks.len() as i32,
        chunks_covered: covered,
        coverage_score: cov_score,
        spread_index: spread,
        top: Vec::new(),
        goal_tokens: unique_token_list(&goal_tokens),
        threshold: opts.threshold,
        exit_code: 0,
        concentration: FileConcStats::default(),
    };

    // Top-N entries. Drop zero-score entries from the visible top —
    // matches the Go `if sc.Score == 0 { break }` behaviour.
    let mut top_n = opts.top as usize;
    if top_n > scored.len() {
        top_n = scored.len();
    }
    for i in 0..top_n {
        let sc = &scored[i];
        if sc.score == 0.0 {
            break;
        }
        res.top.push(TopEntry {
            rank: (i + 1) as i32,
            path: sc.chunk.source_path.clone(),
            line_start: sc.chunk.line_start,
            line_end: sc.chunk.line_end,
            score: sc.score,
            matches: sc.matches.clone(),
        });
    }

    // File concentration: distinct files contributing to the top-N.
    let mut seen: HashSet<String> = HashSet::new();
    for t in res.top.iter() {
        if t.path.is_empty() {
            continue;
        }
        if seen.contains(&t.path) {
            continue;
        }
        seen.insert(t.path.clone());
        res.concentration.files.push(t.path.clone());
    }
    res.concentration.file_count = res.concentration.files.len() as i32;

    if res.coverage_score < opts.threshold {
        res.exit_code = 1;
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_PACK: &str = include_str!("../../../internal/echo/testdata/sample_pack.md");

    #[test]
    fn empty_pack_zero_chunks_exit_zero() {
        let opts = Options {
            goal: "anything".into(),
            ..Default::default()
        };
        let res = evaluate("empty.md", "", &opts);
        assert_eq!(res.chunks_total, 0);
        assert_eq!(res.exit_code, 0);
    }

    #[test]
    fn single_chunk_top_path() {
        let body = "## File contents\n\n### foo/bar.go\n\n```go\npackage bar\n\nfunc BurstHandler() {}\n```\n";
        let opts = Options {
            goal: "burst handler".into(),
            top: 5,
            ..Default::default()
        };
        let res = evaluate("inline", body, &opts);
        assert!(!res.top.is_empty(), "expected at least one scored chunk");
        assert_eq!(res.top[0].path, "foo/bar.go");
        assert!(
            res.top[0].matches.contains_key("burst")
                || res.top[0].matches.contains_key("handler")
        );
    }

    #[test]
    fn multi_chunk_ranking() {
        let opts = Options {
            goal: "rate limit burst".into(),
            top: 5,
            ..Default::default()
        };
        let res = evaluate("sample_pack.md", SAMPLE_PACK, &opts);
        assert!(res.top.len() >= 2);
        assert!(res.top[0].path.contains("limit"));
        assert!(res.coverage_score > 0.0);
    }

    #[test]
    fn threshold_fail_exits_one() {
        let opts = Options {
            goal: "non-existent-token-xyz123".into(),
            top: 5,
            threshold: 0.99,
            ..Default::default()
        };
        let res = evaluate("sample_pack.md", SAMPLE_PACK, &opts);
        assert_eq!(res.exit_code, 1);
    }
}
