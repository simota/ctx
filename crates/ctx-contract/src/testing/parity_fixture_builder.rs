// PARITY-FIXTURE-BUILDER (T-01b)
//
// Test-only helper for the parity-golden test suite. Lives under
// `crates/ctx-contract/src/testing/` so it is reachable from the
// crate's own tests without exposing it as part of the public API.
//
// IMPORTANT (Phase 3 hand-off):
// This file was authored by the Claude branch ahead of the Codex
// branch landing `src/lib.rs` and `src/types.rs`. The references to
// `crate::Contract` etc. will resolve once the Codex side merges its
// type definitions. Until then the file builds only when the crate
// root declares `pub mod testing;` and the parity-critical collection
// types (Files / Symbols / Violations / OK) live at `crate::types::*`
// as agreed in the Phase 2 plan.
//
// If the type paths the Codex branch chooses differ, the only required
// edits are the `use` block below and the `T: Serialize` bounds in
// `serialize_with_always_emit`. The trait, constants, and FS helpers
// are stable.

#![cfg(any(test, feature = "testing"))]

use std::path::PathBuf;

use serde::Serialize;

/// The single frozen instant the parity exporter (`cmd/contract-golden-export`
/// on the Go side) injects into `Build()` via `contract.SetNowFunc`. Keep in
/// sync with `FrozenClockISO` in that file.
///
/// Stored as a string rather than a `chrono::DateTime` so this module
/// stays dependency-free until the Codex side decides on a time crate.
pub const FROZEN_INSTANT: &str = "2026-05-29T00:00:00Z";

/// Clock seam mirroring the Go side's `contract.SetNowFunc`. Tests that
/// drive `Build` indirectly should call `Instant::frozen()` rather than
/// reading the wall clock.
///
/// The Codex branch should wire its `Build` impl to call this trait
/// through a `nowFn`-equivalent generic; until then the trait stands as
/// the contract for that wiring.
pub trait FrozenClock {
    /// Returns the timestamp `Build` will stamp into a Contract's
    /// `created` field. Implementations should return a string in
    /// RFC3339 form, UTC, second-precision, identical to the Go side's
    /// `nowFn().UTC().Format(time.RFC3339)` output.
    fn now_rfc3339(&self) -> &'static str;
}

/// Concrete frozen clock used by parity tests.
pub struct Instant(&'static str);

impl Instant {
    /// Construct a frozen clock at the canonical parity instant.
    pub const fn frozen() -> Self {
        Self(FROZEN_INSTANT)
    }

    /// Construct a frozen clock at an arbitrary RFC3339 string. Used
    /// only by negative-path tests that want to verify the parity
    /// pipeline catches clock-skew between Go and Rust.
    pub const fn at(rfc3339: &'static str) -> Self {
        Self(rfc3339)
    }
}

impl FrozenClock for Instant {
    fn now_rfc3339(&self) -> &'static str {
        self.0
    }
}

/// Serialise `value` with the four parity-critical collections forced
/// to `[]` rather than omitted/`null`. The Go exporter normalises nil
/// slices to `[]` in `normaliseResult`; this helper guarantees the Rust
/// side ships the same shape regardless of `serde(skip_serializing_if)`
/// attributes on the underlying types.
///
/// The four collections that must always emit are:
///   1. `Contract.files` (Vec<File>)
///   2. `Result.violations` (Vec<Violation>)
///   3. `Result.ok` (Vec<OK>)
///   4. `Result.stale_files` (Vec<StaleFile>) and
///      `Result.repack_suggestions` (Vec<String>)
///
/// The implementation here uses serde_json's value model rather than
/// reaching into the concrete types because, at the time of writing,
/// the concrete types live on the Codex branch and have not landed.
/// Once they have, callers may wish to specialise this helper per-type
/// for performance; for parity-golden generation the value-tree pass is
/// fast enough (well under the 100ms budget per fixture).
pub fn serialize_with_always_emit<T: Serialize>(value: &T) -> Vec<u8> {
    // serde_json::to_value is infallible for any Serialize impl that
    // doesn't deliberately return an error; we treat any failure as a
    // bug in the caller's type and panic with the actual error message
    // so the parity test points at the right file.
    let mut root = serde_json::to_value(value)
        .expect("parity_fixture_builder: input value failed to serialize");

    normalise_always_emit(&mut root);

    // `to_vec_pretty` with two-space indent matches the Go exporter's
    // `json.Encoder.SetIndent("", "  ")` output exactly.
    let mut out = serde_json::to_vec_pretty(&root)
        .expect("parity_fixture_builder: post-normalisation value failed to serialize");
    out.push(b'\n'); // mirror Go's encoder trailing newline
    out
}

/// Walk the JSON value tree and replace any `Null` value at one of the
/// parity-critical field names with an empty array. Recurses through
/// objects and arrays so nested results (e.g. a `Result` embedded in a
/// `Report`) are normalised too.
fn normalise_always_emit(v: &mut serde_json::Value) {
    use serde_json::Value;
    match v {
        Value::Object(map) => {
            for key in [
                "files",
                "violations",
                "ok",
                "stale_files",
                "repack_suggestions",
                "symbols",
            ] {
                if let Some(entry) = map.get_mut(key) {
                    if entry.is_null() {
                        *entry = Value::Array(Vec::new());
                    }
                }
            }
            for (_, child) in map.iter_mut() {
                normalise_always_emit(child);
            }
        }
        Value::Array(arr) => {
            for child in arr.iter_mut() {
                normalise_always_emit(child);
            }
        }
        _ => {}
    }
}

/// Returns the absolute path to the parity-fixture directory shared
/// with the Go side: `<repo_root>/internal/contract/testdata/`. Resolved
/// at call time via `CARGO_MANIFEST_DIR` so the helper works whether
/// the crate is built from the repo root or from a workspace context.
///
/// Falls back to a relative path if `CARGO_MANIFEST_DIR` is unset
/// (which only happens outside a cargo invocation, e.g. when an IDE
/// runs the file standalone — best-effort, not a parity guarantee).
pub fn fixture_dir() -> PathBuf {
    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| String::from("crates/ctx-contract"));
    // crates/ctx-contract/  →  ../../internal/contract/testdata
    PathBuf::from(manifest_dir)
        .join("..")
        .join("..")
        .join("internal")
        .join("contract")
        .join("testdata")
}

/// Returns the absolute path to the goldens directory the Go exporter
/// writes to: `<repo_root>/tests/parity/goldens/`. Same resolution
/// strategy as `fixture_dir`.
pub fn goldens_dir() -> PathBuf {
    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| String::from("crates/ctx-contract"));
    PathBuf::from(manifest_dir)
        .join("..")
        .join("..")
        .join("tests")
        .join("parity")
        .join("goldens")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn frozen_clock_returns_canonical_instant() {
        let clock = Instant::frozen();
        assert_eq!(clock.now_rfc3339(), FROZEN_INSTANT);
    }

    #[test]
    fn always_emit_replaces_null_collections() {
        let input = json!({
            "files": null,
            "violations": null,
            "ok": null,
            "stale_files": null,
            "repack_suggestions": null,
            "symbols": null,
            "other_field": null,
        });
        let bytes = serialize_with_always_emit(&input);
        let s = String::from_utf8(bytes).unwrap();
        // Parity-critical fields must be arrays.
        assert!(s.contains("\"files\": []"), "files not normalised: {s}");
        assert!(
            s.contains("\"violations\": []"),
            "violations not normalised: {s}"
        );
        assert!(s.contains("\"ok\": []"), "ok not normalised: {s}");
        assert!(
            s.contains("\"stale_files\": []"),
            "stale_files not normalised: {s}"
        );
        assert!(
            s.contains("\"repack_suggestions\": []"),
            "repack_suggestions not normalised: {s}"
        );
        assert!(s.contains("\"symbols\": []"), "symbols not normalised: {s}");
        // Non-parity null fields are left alone.
        assert!(
            s.contains("\"other_field\": null"),
            "other_field changed: {s}"
        );
    }

    #[test]
    fn fixture_dir_points_at_go_side_testdata() {
        let dir = fixture_dir();
        let s = dir.to_string_lossy();
        assert!(
            s.contains("internal/contract/testdata") || s.contains("internal\\contract\\testdata"),
            "unexpected fixture dir: {s}"
        );
    }
}
