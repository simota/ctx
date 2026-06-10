// crates/ctx-pack/tests/parity.rs
//
// Parity tests run against golden JSON fixtures captured from the Go
// side (see cmd/pack-golden-export/main.go). Each fixture replays the
// SAME inputs through Rust and asserts byte-equal outputs.
//
// Required-feature: `testing` exposes the synth_corpus helpers we
// share with the bench harness.

use ctx_pack::diff::render as diff_render;
use ctx_pack::from_where::parse as from_where_parse;
use ctx_pack::preset::apply_preset;
use ctx_pack::redact::redact_lines;
use ctx_pack::relevance::{score_relevance, RelevanceContext};
use ctx_pack::testing::synth_corpus;
use ctx_pack::types::{DiffEntry, DiffOptions, FileInput, RelevanceResult, WarningInput};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

fn fixtures_root() -> PathBuf {
    // Hosted under tests/parity/pack-goldens/ from the repo root.
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    here.parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("tests/parity/pack-goldens"))
        .expect("repo root")
}

#[derive(Debug, Deserialize)]
struct RelevanceGolden {
    goal: String,
    budget: i64,
    files: Vec<FileInput>,
    token_counts: Vec<i64>,
    results: Vec<RelevanceResult>,
}

#[derive(Debug, Deserialize)]
struct DiffGolden {
    diffs: Vec<DiffEntry>,
    layout: String,
    rendered: String,
}

#[derive(Debug, Deserialize)]
struct RedactGolden {
    data_base64: String,
    warnings: Vec<WarningInput>,
    redacted_base64: String,
}

#[derive(Debug, Deserialize)]
struct FromWhereGolden {
    input_base64: String,
    paths: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct PresetGolden {
    cases: Vec<PresetCase>,
}

#[derive(Debug, Deserialize)]
struct PresetCase {
    name: String,
    patch: ctx_pack::types::PresetPatch,
}

fn b64_decode(s: &str) -> Vec<u8> {
    // Tiny embedded base64 decoder to avoid a dep.
    let table: [i8; 256] = build_b64_table();
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits = 0u32;
    for c in s.bytes() {
        if c == b'=' || c == b'\n' || c == b'\r' || c == b' ' {
            continue;
        }
        let v = table[c as usize];
        if v < 0 {
            panic!("bad base64 char: {c}");
        }
        buf = (buf << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xFF) as u8);
        }
    }
    out
}

const fn build_b64_table() -> [i8; 256] {
    let mut t = [-1i8; 256];
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut i = 0;
    while i < alphabet.len() {
        t[alphabet[i] as usize] = i as i8;
        i += 1;
    }
    t
}

#[test]
fn parity_relevance_small_corpus() {
    let root = fixtures_root();
    let path = root.join("small_corpus/relevance_score.json");
    if !path.exists() {
        eprintln!("skip: golden missing at {}", path.display());
        return;
    }
    let body = fs::read_to_string(&path).expect("read golden");
    let g: RelevanceGolden = serde_json::from_str(&body).expect("parse golden");
    let ctx = RelevanceContext::new(&g.goal, g.budget);
    for (i, fi) in g.files.iter().enumerate() {
        let tc = g.token_counts.get(i).copied().unwrap_or(fi.tokens);
        let r = ctx_pack::relevance::score_relevance_with_ctx(fi, &ctx, tc);
        let want = &g.results[i];
        assert_eq!(r.tier, want.tier, "tier mismatch on file {} ({})", i, fi.path);
        assert_eq!(r.score, want.score, "score mismatch on file {} ({})", i, fi.path);
        assert_eq!(r.reason, want.reason, "reason mismatch on file {} ({})", i, fi.path);
        assert_eq!(
            r.breakdown, want.breakdown,
            "breakdown mismatch on file {} ({})",
            i, fi.path
        );
        assert_eq!(r.signals, want.signals, "signals mismatch on file {} ({})", i, fi.path);
    }
}

#[test]
fn parity_diff_unified() {
    let root = fixtures_root();
    let path = root.join("small_corpus/diff.json");
    if !path.exists() {
        return;
    }
    let body = fs::read_to_string(&path).expect("read golden");
    let g: DiffGolden = serde_json::from_str(&body).expect("parse golden");
    let out = diff_render(
        &g.diffs,
        &DiffOptions {
            layout: g.layout,
            preset: String::new(),
        },
    );
    assert_eq!(out, g.rendered);
}

#[test]
fn parity_redact_round_trip() {
    let root = fixtures_root();
    let path = root.join("small_corpus/redact.json");
    if !path.exists() {
        return;
    }
    let body = fs::read_to_string(&path).expect("read golden");
    let g: RedactGolden = serde_json::from_str(&body).expect("parse golden");
    let data = b64_decode(&g.data_base64);
    let want = b64_decode(&g.redacted_base64);
    let got = redact_lines(&data, &g.warnings);
    assert_eq!(got, want);
}

#[test]
fn parity_from_where() {
    let root = fixtures_root();
    let path = root.join("small_corpus/from_where.json");
    if !path.exists() {
        return;
    }
    let body = fs::read_to_string(&path).expect("read golden");
    let g: FromWhereGolden = serde_json::from_str(&body).expect("parse golden");
    let input = b64_decode(&g.input_base64);
    let r = from_where_parse(&input).expect("parse");
    assert_eq!(r, g.paths);
}

#[test]
fn parity_preset() {
    let root = fixtures_root();
    let path = root.join("small_corpus/preset.json");
    if !path.exists() {
        return;
    }
    let body = fs::read_to_string(&path).expect("read golden");
    let g: PresetGolden = serde_json::from_str(&body).expect("parse golden");
    for case in g.cases {
        let p = apply_preset(&case.name).expect("preset");
        assert_eq!(p, case.patch, "patch mismatch for {}", case.name);
    }
}

#[test]
fn synth_corpus_round_trip_stable_under_session() {
    // Sanity check: scoring the same file with the same ctx twice
    // yields the same result. Catches accidental interior state in
    // the relevance scorer.
    let files = synth_corpus(64);
    let ctx = RelevanceContext::new("ログイン認証", 30000);
    for fi in &files {
        let a = ctx_pack::relevance::score_relevance_with_ctx(fi, &ctx, fi.tokens);
        let b = ctx_pack::relevance::score_relevance_with_ctx(fi, &ctx, fi.tokens);
        assert_eq!(a.score, b.score);
        assert_eq!(a.signals, b.signals);
    }
}

#[test]
fn synth_session_matches_stateless_for_every_file() {
    let files = synth_corpus(256);
    let goal = "auth login";
    let budget = 50000;
    let ctx = RelevanceContext::new(goal, budget);
    for fi in &files {
        let sticky = ctx_pack::relevance::score_relevance_with_ctx(fi, &ctx, fi.tokens);
        let stateless = score_relevance(fi, goal, fi.tokens, budget);
        assert_eq!(sticky.score, stateless.score, "score for {}", fi.path);
        assert_eq!(sticky.tier, stateless.tier, "tier for {}", fi.path);
        assert_eq!(sticky.reason, stateless.reason, "reason for {}", fi.path);
        assert_eq!(sticky.breakdown, stateless.breakdown, "breakdown for {}", fi.path);
        assert_eq!(sticky.signals, stateless.signals, "signals for {}", fi.path);
    }
}

#[allow(dead_code)]
fn _ensure_file_input_compiles(_fi: FileInput) {}
