// tests/parity.rs
//
// Phase 4 parity integration tests.
//
// For each (fixture, function) pair we:
//   1. Load the fixture body from internal/contract/testdata/<fixture>.<ext>
//   2. Drive the Rust implementation with the same inputs the Go exporter used
//   3. Load the Go-side golden from tests/parity/goldens/<fixture>/<Func>.json
//   4. Assert byte-exact match (canonically sorted keys on both sides)
//
// Run with:
//   cargo test --manifest-path crates/ctx-contract/Cargo.toml \
//              --test parity --features testing 2>&1
//
// Frozen clock: all Build calls use FrozenClockGuard so the clock is stable
// across the entire test, not just during the build() call.
//
// Naming: tests are named parity_<fixture>_<func> (lowercase, underscores).

#![cfg(feature = "testing")]

use std::collections::HashMap;
use std::path::PathBuf;

use pretty_assertions::assert_eq;

use ctx_contract::builder::{build, set_now_fn, NowFn};
use ctx_contract::embed::{
    embed_json_patch, embed_markdown, embed_plain, embed_xml, parse_from_pack, strip_contract_block,
};
use ctx_contract::format::render;
use ctx_contract::parse_refs::extract_references;
use ctx_contract::testing::parity_fixture_builder::{goldens_dir, FROZEN_INSTANT};
use ctx_contract::verify::verify;
use ctx_contract::{Contract, FileInput, VerifyOptions};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns absolute path to the repo-side testdata directory.
fn testdata_dir() -> PathBuf {
    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| "crates/ctx-contract".to_string());
    PathBuf::from(manifest_dir)
        .join("..")
        .join("..")
        .join("internal")
        .join("contract")
        .join("testdata")
}

/// Load a golden JSON file as a serde_json::Value.
fn load_golden(fixture: &str, func_name: &str) -> serde_json::Value {
    let path = goldens_dir()
        .join(fixture)
        .join(format!("{func_name}.json"));
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read golden {}: {e}", path.display()));
    serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("failed to parse golden {}: {e}", path.display()))
}

/// Fixture file: reads the raw body; discovers the extension automatically.
fn load_fixture(stem: &str) -> (Vec<u8>, String) {
    for ext in &[".md", ".json", ".xml", ".txt"] {
        let p = testdata_dir().join(format!("{stem}{ext}"));
        if p.exists() {
            let body = std::fs::read(&p)
                .unwrap_or_else(|e| panic!("failed to read fixture {}: {e}", p.display()));
            return (body, ext.to_string());
        }
    }
    panic!(
        "no fixture file found for stem '{stem}' under {}",
        testdata_dir().display()
    );
}

/// Build the same Contract the Go exporter's buildFromFixture produced.
///
/// Rule (from cmd/contract-golden-export/main.go buildFromFixture):
///   - If the fixture has a parsed contract with >0 files, synthesise
///     FileInputs from parsed.files with synthetic content
///     `// synthetic content for <path>\n` and the original symbols.
///   - Otherwise use a single synthetic input:
///     path = `<stem>.synthetic.go`, content = `package synthetic\n`,
///     symbols = ["Synthetic"].
///
/// NOTE: Callers MUST hold a `FrozenClockGuard` before calling this function.
fn build_contract_for_fixture(stem: &str, body: &[u8]) -> Contract {
    match parse_from_pack(body) {
        Some(parsed) if !parsed.files.is_empty() => {
            let inputs: Vec<FileInput> = parsed
                .files
                .iter()
                .map(|f| FileInput {
                    path: f.path.clone(),
                    content: format!("// synthetic content for {}\n", f.path).into_bytes(),
                    symbols: f.symbols.clone(),
                })
                .collect();
            build(inputs)
        }
        _ => build(vec![FileInput {
            path: format!("{stem}.synthetic.go"),
            content: b"package synthetic\n".to_vec(),
            symbols: vec!["Synthetic".to_string()],
        }]),
    }
}

/// Global serialization lock: since `NOW_FN` is a single global Mutex and all
/// parity tests freeze it to `FROZEN_INSTANT`, we must prevent two tests from
/// overlapping their freeze/restore cycles. Holding this lock for the entire
/// body of each test ensures correct clock state regardless of how many threads
/// the test harness spawns.
static CLOCK_TEST_MUTEX: std::sync::LazyLock<std::sync::Mutex<()>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(()));

/// Holds the frozen clock AND the serialization lock for the lifetime of one
/// test. Restores the previous function pointer when dropped.
struct FrozenClockGuard {
    prev: NowFn,
    // Keep the MutexGuard alive until the whole test is done.
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl FrozenClockGuard {
    fn new() -> Self {
        // Acquire serialization lock first, then freeze the clock.
        let lock = CLOCK_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let prev = set_now_fn(Some(|| FROZEN_INSTANT.to_string()));
        Self { prev, _lock: lock }
    }
}

impl Drop for FrozenClockGuard {
    fn drop(&mut self) {
        // Restore clock before releasing the serialization lock.
        set_now_fn(Some(self.prev));
        // _lock drops here, releasing CLOCK_TEST_MUTEX.
    }
}

/// Recursively sort all object keys alphabetically so Rust's struct-field
/// ordering and Go's map-key alphabetical ordering both produce the same
/// canonical string. This matches Go's `encoding/json` behaviour of sorting
/// map keys when marshalling `map[string]any` wrappers (which the Go exporter
/// uses for EmbedJSONPatch, StripContractBlock, ExtractReferences, etc.).
fn sort_keys_deep(v: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match v {
        Value::Object(map) => {
            let mut sorted: std::collections::BTreeMap<String, Value> =
                std::collections::BTreeMap::new();
            for (k, child) in map {
                sorted.insert(k, sort_keys_deep(child));
            }
            let new_map: serde_json::Map<String, Value> = sorted.into_iter().collect();
            Value::Object(new_map)
        }
        Value::Array(arr) => Value::Array(arr.into_iter().map(sort_keys_deep).collect()),
        other => other,
    }
}

/// Serialise a value to pretty-printed JSON + trailing newline, with all
/// object keys sorted alphabetically (matching Go's encoding/json output).
fn to_golden_json(v: serde_json::Value) -> String {
    let sorted = sort_keys_deep(v);
    let mut s = serde_json::to_string_pretty(&sorted).expect("failed to serialise value to JSON");
    s.push('\n');
    s
}

/// Normalise a Value: replace null collection fields with [] so both sides
/// match Go's `normaliseResult` / `normaliseAlwaysEmit` behaviour.
fn normalise_value(v: &mut serde_json::Value) {
    use serde_json::Value;
    match v {
        Value::Object(map) => {
            for key in &[
                "files",
                "violations",
                "ok",
                "stale_files",
                "repack_suggestions",
                "symbols",
            ] {
                if let Some(entry) = map.get_mut(*key) {
                    if entry.is_null() {
                        *entry = Value::Array(Vec::new());
                    }
                }
            }
            for (_, child) in map.iter_mut() {
                normalise_value(child);
            }
        }
        Value::Array(arr) => {
            for child in arr.iter_mut() {
                normalise_value(child);
            }
        }
        _ => {}
    }
}

/// Build the embed output JSON (body + bytes) for EmbedMarkdown / EmbedXML /
/// EmbedPlain and return it as a serde_json::Value.
fn embed_to_value(body: &str) -> serde_json::Value {
    serde_json::json!({
        "body": body,
        "bytes": body.len()
    })
}

/// Assert a Rust-produced serde_json::Value exactly equals the golden.
/// Both sides are canonicalised (keys sorted alphabetically) before comparison
/// so that Go's map-key alphabetical order matches Rust's struct-field order.
/// Uses pretty_assertions::assert_eq so failures show a coloured diff.
fn assert_value_eq(rust_val: serde_json::Value, golden: serde_json::Value, label: &str) {
    let rust_str = to_golden_json(rust_val);
    let gold_str = to_golden_json(golden);
    assert_eq!(rust_str, gold_str, "parity divergence in {label}");
}

// ---------------------------------------------------------------------------
// ── Build ───────────────────────────────────────────────────────────────────
// ---------------------------------------------------------------------------

#[test]
fn parity_sample_pack_build() {
    let _clock = FrozenClockGuard::new();
    let stem = "sample_pack";
    let (body, _ext) = load_fixture(stem);
    let c = build_contract_for_fixture(stem, &body);
    let mut rust_val = serde_json::to_value(&c).expect("serialise contract");
    normalise_value(&mut rust_val);
    let golden = load_golden(stem, "Build");
    assert_value_eq(rust_val, golden, &format!("{stem}/Build"));
}

#[test]
fn parity_empty_pack_build() {
    let _clock = FrozenClockGuard::new();
    let stem = "empty_pack";
    let (body, _ext) = load_fixture(stem);
    let c = build_contract_for_fixture(stem, &body);
    let mut rust_val = serde_json::to_value(&c).expect("serialise contract");
    normalise_value(&mut rust_val);
    let golden = load_golden(stem, "Build");
    assert_value_eq(rust_val, golden, &format!("{stem}/Build"));
}

#[test]
fn parity_json_pack_build() {
    let _clock = FrozenClockGuard::new();
    let stem = "json_pack";
    let (body, _ext) = load_fixture(stem);
    let c = build_contract_for_fixture(stem, &body);
    let mut rust_val = serde_json::to_value(&c).expect("serialise contract");
    normalise_value(&mut rust_val);
    let golden = load_golden(stem, "Build");
    assert_value_eq(rust_val, golden, &format!("{stem}/Build"));
}

#[test]
fn parity_multi_lang_pack_build() {
    let _clock = FrozenClockGuard::new();
    let stem = "multi_lang_pack";
    let (body, _ext) = load_fixture(stem);
    let c = build_contract_for_fixture(stem, &body);
    let mut rust_val = serde_json::to_value(&c).expect("serialise contract");
    normalise_value(&mut rust_val);
    let golden = load_golden(stem, "Build");
    assert_value_eq(rust_val, golden, &format!("{stem}/Build"));
}

// ---------------------------------------------------------------------------
// ── EmbedMarkdown ───────────────────────────────────────────────────────────
// ---------------------------------------------------------------------------

#[test]
fn parity_sample_pack_embed_markdown() {
    let _clock = FrozenClockGuard::new();
    let stem = "sample_pack";
    let (body, _ext) = load_fixture(stem);
    let c = build_contract_for_fixture(stem, &body);
    let mut buf = Vec::new();
    embed_markdown(&mut buf, &c).expect("embed_markdown");
    let out = String::from_utf8(buf).expect("utf8");
    let rust_val = embed_to_value(&out);
    let golden = load_golden(stem, "EmbedMarkdown");
    assert_value_eq(rust_val, golden, &format!("{stem}/EmbedMarkdown"));
}

#[test]
fn parity_empty_pack_embed_markdown() {
    let _clock = FrozenClockGuard::new();
    let stem = "empty_pack";
    let (body, _ext) = load_fixture(stem);
    let c = build_contract_for_fixture(stem, &body);
    let mut buf = Vec::new();
    embed_markdown(&mut buf, &c).expect("embed_markdown");
    let out = String::from_utf8(buf).expect("utf8");
    let rust_val = embed_to_value(&out);
    let golden = load_golden(stem, "EmbedMarkdown");
    assert_value_eq(rust_val, golden, &format!("{stem}/EmbedMarkdown"));
}

#[test]
fn parity_json_pack_embed_markdown() {
    let _clock = FrozenClockGuard::new();
    let stem = "json_pack";
    let (body, _ext) = load_fixture(stem);
    let c = build_contract_for_fixture(stem, &body);
    let mut buf = Vec::new();
    embed_markdown(&mut buf, &c).expect("embed_markdown");
    let out = String::from_utf8(buf).expect("utf8");
    let rust_val = embed_to_value(&out);
    let golden = load_golden(stem, "EmbedMarkdown");
    assert_value_eq(rust_val, golden, &format!("{stem}/EmbedMarkdown"));
}

#[test]
fn parity_multi_lang_pack_embed_markdown() {
    let _clock = FrozenClockGuard::new();
    let stem = "multi_lang_pack";
    let (body, _ext) = load_fixture(stem);
    let c = build_contract_for_fixture(stem, &body);
    let mut buf = Vec::new();
    embed_markdown(&mut buf, &c).expect("embed_markdown");
    let out = String::from_utf8(buf).expect("utf8");
    let rust_val = embed_to_value(&out);
    let golden = load_golden(stem, "EmbedMarkdown");
    assert_value_eq(rust_val, golden, &format!("{stem}/EmbedMarkdown"));
}

// ---------------------------------------------------------------------------
// ── EmbedXML ────────────────────────────────────────────────────────────────
// ---------------------------------------------------------------------------

#[test]
fn parity_sample_pack_embed_xml() {
    let _clock = FrozenClockGuard::new();
    let stem = "sample_pack";
    let (body, _ext) = load_fixture(stem);
    let c = build_contract_for_fixture(stem, &body);
    let mut buf = Vec::new();
    embed_xml(&mut buf, &c).expect("embed_xml");
    let out = String::from_utf8(buf).expect("utf8");
    let rust_val = embed_to_value(&out);
    let golden = load_golden(stem, "EmbedXML");
    assert_value_eq(rust_val, golden, &format!("{stem}/EmbedXML"));
}

#[test]
fn parity_empty_pack_embed_xml() {
    let _clock = FrozenClockGuard::new();
    let stem = "empty_pack";
    let (body, _ext) = load_fixture(stem);
    let c = build_contract_for_fixture(stem, &body);
    let mut buf = Vec::new();
    embed_xml(&mut buf, &c).expect("embed_xml");
    let out = String::from_utf8(buf).expect("utf8");
    let rust_val = embed_to_value(&out);
    let golden = load_golden(stem, "EmbedXML");
    assert_value_eq(rust_val, golden, &format!("{stem}/EmbedXML"));
}

#[test]
fn parity_json_pack_embed_xml() {
    let _clock = FrozenClockGuard::new();
    let stem = "json_pack";
    let (body, _ext) = load_fixture(stem);
    let c = build_contract_for_fixture(stem, &body);
    let mut buf = Vec::new();
    embed_xml(&mut buf, &c).expect("embed_xml");
    let out = String::from_utf8(buf).expect("utf8");
    let rust_val = embed_to_value(&out);
    let golden = load_golden(stem, "EmbedXML");
    assert_value_eq(rust_val, golden, &format!("{stem}/EmbedXML"));
}

#[test]
fn parity_multi_lang_pack_embed_xml() {
    let _clock = FrozenClockGuard::new();
    let stem = "multi_lang_pack";
    let (body, _ext) = load_fixture(stem);
    let c = build_contract_for_fixture(stem, &body);
    let mut buf = Vec::new();
    embed_xml(&mut buf, &c).expect("embed_xml");
    let out = String::from_utf8(buf).expect("utf8");
    let rust_val = embed_to_value(&out);
    let golden = load_golden(stem, "EmbedXML");
    assert_value_eq(rust_val, golden, &format!("{stem}/EmbedXML"));
}

// ---------------------------------------------------------------------------
// ── EmbedPlain ──────────────────────────────────────────────────────────────
// ---------------------------------------------------------------------------

#[test]
fn parity_sample_pack_embed_plain() {
    let _clock = FrozenClockGuard::new();
    let stem = "sample_pack";
    let (body, _ext) = load_fixture(stem);
    let c = build_contract_for_fixture(stem, &body);
    let mut buf = Vec::new();
    embed_plain(&mut buf, &c).expect("embed_plain");
    let out = String::from_utf8(buf).expect("utf8");
    let rust_val = embed_to_value(&out);
    let golden = load_golden(stem, "EmbedPlain");
    assert_value_eq(rust_val, golden, &format!("{stem}/EmbedPlain"));
}

#[test]
fn parity_empty_pack_embed_plain() {
    let _clock = FrozenClockGuard::new();
    let stem = "empty_pack";
    let (body, _ext) = load_fixture(stem);
    let c = build_contract_for_fixture(stem, &body);
    let mut buf = Vec::new();
    embed_plain(&mut buf, &c).expect("embed_plain");
    let out = String::from_utf8(buf).expect("utf8");
    let rust_val = embed_to_value(&out);
    let golden = load_golden(stem, "EmbedPlain");
    assert_value_eq(rust_val, golden, &format!("{stem}/EmbedPlain"));
}

#[test]
fn parity_json_pack_embed_plain() {
    let _clock = FrozenClockGuard::new();
    let stem = "json_pack";
    let (body, _ext) = load_fixture(stem);
    let c = build_contract_for_fixture(stem, &body);
    let mut buf = Vec::new();
    embed_plain(&mut buf, &c).expect("embed_plain");
    let out = String::from_utf8(buf).expect("utf8");
    let rust_val = embed_to_value(&out);
    let golden = load_golden(stem, "EmbedPlain");
    assert_value_eq(rust_val, golden, &format!("{stem}/EmbedPlain"));
}

#[test]
fn parity_multi_lang_pack_embed_plain() {
    let _clock = FrozenClockGuard::new();
    let stem = "multi_lang_pack";
    let (body, _ext) = load_fixture(stem);
    let c = build_contract_for_fixture(stem, &body);
    let mut buf = Vec::new();
    embed_plain(&mut buf, &c).expect("embed_plain");
    let out = String::from_utf8(buf).expect("utf8");
    let rust_val = embed_to_value(&out);
    let golden = load_golden(stem, "EmbedPlain");
    assert_value_eq(rust_val, golden, &format!("{stem}/EmbedPlain"));
}

// ---------------------------------------------------------------------------
// ── EmbedJSONPatch ──────────────────────────────────────────────────────────
// ---------------------------------------------------------------------------

/// Build the EmbedJSONPatch parity value. Clock must already be frozen.
fn build_embed_json_patch_val(stem: &str, body: &[u8], ext: &str) -> serde_json::Value {
    let c = build_contract_for_fixture(stem, body);
    // Go exporter: if not .json, use `{"pack":"placeholder"}` as the pack input
    let pack: &[u8] = if ext == ".json" {
        body
    } else {
        b"{\"pack\":\"placeholder\"}"
    };
    let patched = embed_json_patch(pack, &c).expect("embed_json_patch");
    let patched_val: serde_json::Value =
        serde_json::from_slice(&patched).expect("parse patched JSON");
    serde_json::json!({
        "input_pack_bytes": pack.len(),
        "patched_pack": patched_val
    })
}

#[test]
fn parity_sample_pack_embed_json_patch() {
    let _clock = FrozenClockGuard::new();
    let stem = "sample_pack";
    let (body, ext) = load_fixture(stem);
    let rust_val = build_embed_json_patch_val(stem, &body, &ext);
    let golden = load_golden(stem, "EmbedJSONPatch");
    assert_value_eq(rust_val, golden, &format!("{stem}/EmbedJSONPatch"));
}

#[test]
fn parity_empty_pack_embed_json_patch() {
    let _clock = FrozenClockGuard::new();
    let stem = "empty_pack";
    let (body, ext) = load_fixture(stem);
    let rust_val = build_embed_json_patch_val(stem, &body, &ext);
    let golden = load_golden(stem, "EmbedJSONPatch");
    assert_value_eq(rust_val, golden, &format!("{stem}/EmbedJSONPatch"));
}

#[test]
fn parity_json_pack_embed_json_patch() {
    let _clock = FrozenClockGuard::new();
    let stem = "json_pack";
    let (body, ext) = load_fixture(stem);
    let rust_val = build_embed_json_patch_val(stem, &body, &ext);
    let golden = load_golden(stem, "EmbedJSONPatch");
    assert_value_eq(rust_val, golden, &format!("{stem}/EmbedJSONPatch"));
}

#[test]
fn parity_multi_lang_pack_embed_json_patch() {
    let _clock = FrozenClockGuard::new();
    let stem = "multi_lang_pack";
    let (body, ext) = load_fixture(stem);
    let rust_val = build_embed_json_patch_val(stem, &body, &ext);
    let golden = load_golden(stem, "EmbedJSONPatch");
    assert_value_eq(rust_val, golden, &format!("{stem}/EmbedJSONPatch"));
}

// ---------------------------------------------------------------------------
// ── ParseFromPack ───────────────────────────────────────────────────────────
// ---------------------------------------------------------------------------

fn build_parse_from_pack_val(body: &[u8]) -> serde_json::Value {
    match parse_from_pack(body) {
        Some(c) => {
            let mut cv = serde_json::to_value(&c).expect("serialise contract");
            normalise_value(&mut cv);
            serde_json::json!({ "contract": cv, "ok": true })
        }
        None => serde_json::json!({ "contract": null, "ok": false }),
    }
}

#[test]
fn parity_sample_pack_parse_from_pack() {
    let stem = "sample_pack";
    let (body, _ext) = load_fixture(stem);
    let rust_val = build_parse_from_pack_val(&body);
    let golden = load_golden(stem, "ParseFromPack");
    assert_value_eq(rust_val, golden, &format!("{stem}/ParseFromPack"));
}

#[test]
fn parity_empty_pack_parse_from_pack() {
    let stem = "empty_pack";
    let (body, _ext) = load_fixture(stem);
    let rust_val = build_parse_from_pack_val(&body);
    let golden = load_golden(stem, "ParseFromPack");
    assert_value_eq(rust_val, golden, &format!("{stem}/ParseFromPack"));
}

#[test]
fn parity_json_pack_parse_from_pack() {
    let stem = "json_pack";
    let (body, _ext) = load_fixture(stem);
    let rust_val = build_parse_from_pack_val(&body);
    let golden = load_golden(stem, "ParseFromPack");
    assert_value_eq(rust_val, golden, &format!("{stem}/ParseFromPack"));
}

#[test]
fn parity_multi_lang_pack_parse_from_pack() {
    let stem = "multi_lang_pack";
    let (body, _ext) = load_fixture(stem);
    let rust_val = build_parse_from_pack_val(&body);
    let golden = load_golden(stem, "ParseFromPack");
    assert_value_eq(rust_val, golden, &format!("{stem}/ParseFromPack"));
}

// ---------------------------------------------------------------------------
// ── StripContractBlock ──────────────────────────────────────────────────────
// ---------------------------------------------------------------------------

fn build_strip_val(body: &[u8]) -> serde_json::Value {
    let stripped = strip_contract_block(body);
    let stripped_str = String::from_utf8_lossy(&stripped).into_owned();
    serde_json::json!({
        "stripped_bytes": stripped.len(),
        "stripped_body": stripped_str
    })
}

#[test]
fn parity_sample_pack_strip_contract_block() {
    let stem = "sample_pack";
    let (body, _ext) = load_fixture(stem);
    let rust_val = build_strip_val(&body);
    let golden = load_golden(stem, "StripContractBlock");
    assert_value_eq(rust_val, golden, &format!("{stem}/StripContractBlock"));
}

#[test]
fn parity_empty_pack_strip_contract_block() {
    let stem = "empty_pack";
    let (body, _ext) = load_fixture(stem);
    let rust_val = build_strip_val(&body);
    let golden = load_golden(stem, "StripContractBlock");
    assert_value_eq(rust_val, golden, &format!("{stem}/StripContractBlock"));
}

#[test]
fn parity_json_pack_strip_contract_block() {
    let stem = "json_pack";
    let (body, _ext) = load_fixture(stem);
    let rust_val = build_strip_val(&body);
    let golden = load_golden(stem, "StripContractBlock");
    assert_value_eq(rust_val, golden, &format!("{stem}/StripContractBlock"));
}

#[test]
fn parity_multi_lang_pack_strip_contract_block() {
    let stem = "multi_lang_pack";
    let (body, _ext) = load_fixture(stem);
    let rust_val = build_strip_val(&body);
    let golden = load_golden(stem, "StripContractBlock");
    assert_value_eq(rust_val, golden, &format!("{stem}/StripContractBlock"));
}

// ---------------------------------------------------------------------------
// ── ExtractReferences ───────────────────────────────────────────────────────
// ---------------------------------------------------------------------------

fn build_extract_refs_val(body: &[u8]) -> serde_json::Value {
    let refs = extract_references(body);
    let arr: Vec<serde_json::Value> = refs
        .iter()
        .map(|r| {
            serde_json::json!({
                "kind": r.kind,
                "path": r.path,
                "line_start": r.line_start,
                "line_end": r.line_end,
                "symbol": r.symbol,
                "source_line": r.source_line
            })
        })
        .collect();
    serde_json::Value::Array(arr)
}

#[test]
fn parity_sample_pack_extract_references() {
    let stem = "sample_pack";
    let (body, _ext) = load_fixture(stem);
    let rust_val = build_extract_refs_val(&body);
    let golden = load_golden(stem, "ExtractReferences");
    assert_value_eq(rust_val, golden, &format!("{stem}/ExtractReferences"));
}

#[test]
fn parity_empty_pack_extract_references() {
    let stem = "empty_pack";
    let (body, _ext) = load_fixture(stem);
    let rust_val = build_extract_refs_val(&body);
    let golden = load_golden(stem, "ExtractReferences");
    assert_value_eq(rust_val, golden, &format!("{stem}/ExtractReferences"));
}

#[test]
fn parity_json_pack_extract_references() {
    let stem = "json_pack";
    let (body, _ext) = load_fixture(stem);
    let rust_val = build_extract_refs_val(&body);
    let golden = load_golden(stem, "ExtractReferences");
    assert_value_eq(rust_val, golden, &format!("{stem}/ExtractReferences"));
}

#[test]
fn parity_multi_lang_pack_extract_references() {
    let stem = "multi_lang_pack";
    let (body, _ext) = load_fixture(stem);
    let rust_val = build_extract_refs_val(&body);
    let golden = load_golden(stem, "ExtractReferences");
    assert_value_eq(rust_val, golden, &format!("{stem}/ExtractReferences"));
}

// ---------------------------------------------------------------------------
// ── Verify ──────────────────────────────────────────────────────────────────
// ---------------------------------------------------------------------------

/// Go exporter uses the parsed contract when available (including empty-files
/// case), otherwise falls back to the built contract.
/// VerifyOptions is zero value. PackFile is set to `<stem>` (stable, no abs path).
fn build_verify_val(stem: &str, body: &[u8]) -> serde_json::Value {
    let contract_for_verify = match parse_from_pack(body) {
        Some(parsed) => parsed,
        None => build_contract_for_fixture(stem, body),
    };
    let mut res = verify(&contract_for_verify, body, &VerifyOptions::default());
    res.pack_file = stem.to_string();
    let mut val = serde_json::to_value(&res).expect("serialise verify result");
    normalise_value(&mut val);
    val
}

#[test]
fn parity_sample_pack_verify() {
    let _clock = FrozenClockGuard::new();
    let stem = "sample_pack";
    let (body, _ext) = load_fixture(stem);
    let rust_val = build_verify_val(stem, &body);
    let golden = load_golden(stem, "Verify");
    assert_value_eq(rust_val, golden, &format!("{stem}/Verify"));
}

#[test]
fn parity_empty_pack_verify() {
    let _clock = FrozenClockGuard::new();
    let stem = "empty_pack";
    let (body, _ext) = load_fixture(stem);
    let rust_val = build_verify_val(stem, &body);
    let golden = load_golden(stem, "Verify");
    assert_value_eq(rust_val, golden, &format!("{stem}/Verify"));
}

#[test]
fn parity_json_pack_verify() {
    let _clock = FrozenClockGuard::new();
    let stem = "json_pack";
    let (body, _ext) = load_fixture(stem);
    let rust_val = build_verify_val(stem, &body);
    let golden = load_golden(stem, "Verify");
    assert_value_eq(rust_val, golden, &format!("{stem}/Verify"));
}

#[test]
fn parity_multi_lang_pack_verify() {
    let _clock = FrozenClockGuard::new();
    let stem = "multi_lang_pack";
    let (body, _ext) = load_fixture(stem);
    let rust_val = build_verify_val(stem, &body);
    let golden = load_golden(stem, "Verify");
    assert_value_eq(rust_val, golden, &format!("{stem}/Verify"));
}

// ---------------------------------------------------------------------------
// ── Render ──────────────────────────────────────────────────────────────────
// ---------------------------------------------------------------------------

/// Build the Render parity value. Clock must already be frozen when building
/// the fallback contract (Render itself is clock-independent).
fn build_render_val(stem: &str, body: &[u8]) -> serde_json::Value {
    let contract_for_verify = match parse_from_pack(body) {
        Some(parsed) => parsed,
        None => build_contract_for_fixture(stem, body),
    };
    let mut res = verify(&contract_for_verify, body, &VerifyOptions::default());
    res.pack_file = stem.to_string();

    let mut rendered = HashMap::new();
    for fmt in &["markdown", "plain", "json"] {
        let mut buf = Vec::new();
        render(&mut buf, &res, fmt).expect("render");
        rendered.insert(*fmt, String::from_utf8(buf).expect("utf8"));
    }
    serde_json::json!({
        "json": rendered["json"],
        "markdown": rendered["markdown"],
        "plain": rendered["plain"]
    })
}

#[test]
fn parity_sample_pack_render() {
    let _clock = FrozenClockGuard::new();
    let stem = "sample_pack";
    let (body, _ext) = load_fixture(stem);
    let rust_val = build_render_val(stem, &body);
    let golden = load_golden(stem, "Render");
    assert_value_eq(rust_val, golden, &format!("{stem}/Render"));
}

#[test]
fn parity_empty_pack_render() {
    let _clock = FrozenClockGuard::new();
    let stem = "empty_pack";
    let (body, _ext) = load_fixture(stem);
    let rust_val = build_render_val(stem, &body);
    let golden = load_golden(stem, "Render");
    assert_value_eq(rust_val, golden, &format!("{stem}/Render"));
}

#[test]
fn parity_json_pack_render() {
    let _clock = FrozenClockGuard::new();
    let stem = "json_pack";
    let (body, _ext) = load_fixture(stem);
    let rust_val = build_render_val(stem, &body);
    let golden = load_golden(stem, "Render");
    assert_value_eq(rust_val, golden, &format!("{stem}/Render"));
}

#[test]
fn parity_multi_lang_pack_render() {
    let _clock = FrozenClockGuard::new();
    let stem = "multi_lang_pack";
    let (body, _ext) = load_fixture(stem);
    let rust_val = build_render_val(stem, &body);
    let golden = load_golden(stem, "Render");
    assert_value_eq(rust_val, golden, &format!("{stem}/Render"));
}
