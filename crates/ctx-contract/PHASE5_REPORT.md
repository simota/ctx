# Phase 5 Improvement Report

Branch: `summit/contract-rust-port`
Date: 2026-05-29
Mode: compressed orbit (1 loop)
Inputs: `PHASE4_REVIEW.md` (4 CONFIRMED + 4 LIKELY findings)

## Loop count: 1

A single improvement pass cleared every CONFIRMED finding and the
trivially-scoped LIKELY ones. No second loop required.

## Fixes applied

| id   | file                                          | LOC changed | regression test                                     | status   |
|------|-----------------------------------------------|-------------|------------------------------------------------------|----------|
| F-01 | `crates/ctx-contract/src/parse_refs.rs`       | ~10         | `f01_oversized_line_terminates_scanning_like_go_bufio` | RESOLVED |
| F-02 | `crates/ctx-contract/src/verify.rs`           | ~20         | `f02_relative_dotdot_collapses_against_preceding_component` | RESOLVED |
| F-03 | `crates/ctx-contract/src/embed.rs`            | ~8          | `f03_contract_null_decodes_as_zero_contract_with_schema_version` | RESOLVED |
| F-04 | `crates/ctx-contract/src/parse_refs.rs`, `src/embed.rs` | ~8 | `f04_diff_header_rejects_unicode_whitespace_separator`, `f04_diff_header_rejects_ideographic_space_separator`, `f04_strip_contract_block_treats_nbsp_as_non_whitespace` | RESOLVED |
| F-05 | `crates/ctx-contract/src/embed.rs`            | ~8          | `f05_embed_json_patch_sorts_top_level_keys_alphabetically` | RESOLVED |
| F-07 | `crates/ctx-contract/src/verify.rs`           | ~5          | (covered indirectly via parity goldens; no targeted regression) | RESOLVED |
| F-13 | `crates/ctx-contract/src/verify.rs`           | ~3          | (dead-code removal; no test needed)                  | RESOLVED |

Implementation notes:

- **F-01** — flipped `continue` → `break` for the >1 MiB-line case and hoisted
  `line_no += 1` to the top of the loop iteration so source-line numbering
  matches Go's `for scanner.Scan() { lineNo++ }` form. Inline PARITY comment
  added that names Phase 1 L-01.
- **F-02** — replaced the strict `Path::components()` walker in
  `worktree_sha` with a `filepath.Clean`-style fold: `Normal` components push,
  `ParentDir` pops the previous push, and only an *uncancellable* `..`
  (would escape the root) or an absolute prefix is rejected. Matches Go's
  `filepath.Clean(filepath.FromSlash(rel))` then `strings.HasPrefix(rel,
  "../")` check.
- **F-03** — kept the `raw.is_null()` short-circuit but redirected it to
  return `Contract::default()` (with `schema_version = 1` after the existing
  `if c.schema_version == 0` line) instead of `None`. This mirrors Go's
  `json.Unmarshal(rawNull, &c)` leaving `c` zero-valued so the caller still
  reports `(Contract{SchemaVersion:1}, true)`. Note: a naive
  `serde_json::from_value(Value::Null)` on `Contract` returns `Err`, so the
  explicit `is_null()` branch is required — this is *not* a no-op edit.
- **F-04** — could not use the obvious `(?-u:\s)` / `(?-u:\S)` in the
  string-based `regex::Regex` because that crate forbids patterns that
  could match invalid UTF-8. Used explicit ASCII character classes
  (`[\t\n\x0C\r ]` / `[^\t\n\x0C\r ]`) for `DIFF_HEADER_RE` instead — same
  semantics, UTF-8-safe. The bytes-based `regex::bytes::Regex` used for
  `CONTRACT_BLOCK_RE` accepts `(?-u:\s)*` directly, so that one uses the
  documented form.
- **F-05** — `embed_json_patch` now round-trips through a `BTreeMap<String,
  Value>` before re-serialising, so the top-level keys land in alphabetical
  order to match Go's `json.Marshal(map[string]RawMessage)`. The
  `preserve_order` feature on `serde_json` is still in effect for the rest
  of the crate; only this one call-site needed the deterministic ordering.
- **F-07** — swapped `to_ascii_lowercase()` for `to_lowercase()` on both
  sides of the case-insensitive `lookup_path` fallback so non-ASCII contract
  paths (`İ`, `ß`, etc.) match the way Go's `strings.ToLower` does.
- **F-13** — removed the `_force_use(_r: &Reference)` dummy and its
  `#[allow(dead_code)]`. `Reference` is exercised through `extract_references
  → for r in &refs`, so the dummy was unnecessary. The `use crate::types`
  list lost the now-orphaned `Reference` import in the same edit.

## Test results

- `cargo test --lib`: **19 passed / 0 failed**
- `cargo test --test parity --features testing`: **40 passed / 0 failed**
- `cargo test --test regression`: **7 passed / 0 failed**
- `cargo check --tests --features testing`: clean, zero warnings.

Full command:

```
cargo test --manifest-path crates/ctx-contract/Cargo.toml --features testing
```

Output (aggregated):

```
test result: ok. 19 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 40 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok.  7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

## CONFIRMED bugs remaining

*(empty — all four CONFIRMED findings from PHASE4_REVIEW.md are fixed)*

## LIKELY findings addressed

- **F-05** — `embed_json_patch` alphabetical key ordering via BTreeMap.
- **F-07** — `lookup_path` Unicode lowercase fallback.
- **F-13** — `_force_use` dead-code dummy removed.

## Deferred to follow-up Summit / apex

- **F-06** (OS error string leak in `worktree_sha`) — needs cross-platform
  format alignment (Go `open <path>: <kind>` vs Rust
  `<kind> (os error N)`); deferred because cleanly mirroring Go requires a
  small `errno-to-string` table that's noisier than the rest of this loop.
  Doesn't block any current golden because none of them exercise a
  permission-denied path.
- **F-08** (`BufReader::lines` drops non-UTF-8 lines) — needs a switch to
  `read_until(b'\n', &mut buf)` plus `from_utf8_lossy`; deferred because
  the only known trigger is Latin-1 bytes inside the response, and we have
  no parity golden that ships such bytes yet. Track for the parity-golden
  regeneration step.
- **F-09 / F-10 / F-11 / F-12** — `CANDIDATE`-tier items from PHASE4_REVIEW;
  cosmetic / perf / API-tidy.
- **`Result` → `VerifyResult` rename (F-11)** — API tidy-up; needs a coordinated
  edit with any downstream caller, defer to the FFI / CLI integration milestone.
- **FFI completion (`ffi.rs`) / CLI integration (T-26 / T-27)** — out of scope
  for Phase 5 per the brief.
- **L-02 hardened mode** (`follow_symlinks` opt) — still deferred per Phase 3
  brief; track for the security-hardening sweep.
- **L-08 unbounded JSON parse** — pre-existing Go + Rust risk, needs CLI-level
  `--max-pack-bytes` guard.

## Notes on parity-vs-correctness tradeoff

The F-02 fix relaxes the `worktree_sha` validation: contract paths
containing internal `..` segments are now followed if they cancel out
within the root. This **trades** a small piece of defence-in-depth (the
old code rejected ANY `..` even when harmless) for byte-parity with Go.
Symlinks remain followed (Phase 1 L-02, still tracked as DEFERRED). If
the security posture changes in a future milestone, the right escape
hatch is a `VerifyOptions::follow_traversal: bool` (default `true` for
parity, opt-in `false` for hardened mode) — same shape as the proposed
`follow_symlinks` flag. No tests broke from the relaxation.

## Verdict

**RESOLVED** — all 4 CONFIRMED parity bugs fixed, 3 LIKELY items also
addressed, full test matrix green (19 lib + 40 parity + 7 regression = 66
tests passing). Ready for Phase 6 delivery.
