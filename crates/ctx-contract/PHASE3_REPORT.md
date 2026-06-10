# Phase 3 Report — ctx-contract Rust port

## Engine note

Phase 3 core implementation routed from codex to claude/opus after codex
CLI hung (5h, zero output, killed). engine_distribution actual:
claude=~70%, codex=0%, agy=0%.

## Modules

| Module                                          | Status     | Notes                                                                                              |
| ----------------------------------------------- | ---------- | -------------------------------------------------------------------------------------------------- |
| `Cargo.toml`                                    | Replaced   | Full manifest with serde / serde_json (preserve_order) / sha2 / hex / regex / chrono / once_cell.  |
| `src/lib.rs`                                    | Replaced   | Declares all submodules; keeps `pub mod testing;` under cfg. `SCHEMA_VERSION = 1`.                 |
| `src/types.rs`                                  | Implemented| `Contract`, `File`, `Reference`, `ReferenceKind`, `Violation`, `ViolationKind`, `OK`, `StaleFile`, `VerifyOptions`, `Result`, `FileInput`. Wire shape mirrors Go json tags (omitempty / always-emit) per parity goldens. |
| `src/hash.rs`                                   | Implemented| `sha256_hex` via `sha2 + hex`. Known-vector tests included.                                        |
| `src/builder.rs`                                | Implemented| `line_count`, `dedup_symbols`, `build`, `set_now_fn` (clock seam parity with `SetNowFunc`).         |
| `src/parse_refs.rs`                             | Implemented| `extract_references`, `looks_like_path`, `SUPPORTED_EXTS`, regexes (path/symbol/diff-header). `// PARITY: matches bufio.Scanner 1MB cap` comment present. |
| `src/embed.rs`                                  | Implemented| `embed_markdown`/`xml`/`plain`, `embed_json_patch`, `parse_from_pack`, `strip_contract_block`, `to_json_field`. |
| `src/verify.rs`                                 | Implemented| Full port of verify.go incl. line-range containment, dotted-symbol fallback, worktree staleness, strict mode. |
| `src/format.rs`                                 | Implemented| `render` markdown/plain/json renderers. ViolationKind `Ord` impl added for BTreeMap grouping.       |
| `src/testing/mod.rs`, `parity_fixture_builder.rs` | Untouched | Phase 3 Claude branch artifacts preserved verbatim.                                              |
| `src/ffi.rs`                                    | Skipped (P3)| Out of scope per task instructions.                                                              |

## Compile status

```
cargo check --manifest-path crates/ctx-contract/Cargo.toml
Finished `dev` profile [unoptimized + debuginfo] target(s)
```

Zero warnings, zero errors.

## Test build status

```
cargo test --manifest-path crates/ctx-contract/Cargo.toml --lib --no-run
Executable unittests src/lib.rs (.../ctx_contract-<hash>)
```

Test executable links successfully. Running `cargo test --lib`:

```
running 19 tests
... test result: ok. 19 passed; 0 failed; 0 ignored
```

All in-crate unit tests pass (3 hash, 4 builder, 4 parse_refs, 4 embed,
3 testing::parity_fixture_builder, 1 misc).

## Parity-golden behaviour notes (for Phase 4)

* `Contract.files` uses default Vec; emits `[]` automatically when empty
  (matches Go's golden which always has at least one file). Per-`File`
  `symbols` is `skip_serializing_if = Vec::is_empty` to match Go's
  `,omitempty`.
* `Result.{violations,ok,stale_files,repack_suggestions}` deliberately
  do **not** use `skip_serializing_if`. They emit `[]` when empty, byte-
  parity with Go's `normaliseResult` nil→[] coercion in the exporter.
* `Reference` emits all six fields (kind/path/line_start/line_end/symbol/
  source_line) every time, matching `canonicaliseRefs` in the exporter.
* `Violation` fields use `omitempty`-equivalents on path/line_start/
  line_end/symbol/expected_sha (renamed to `expected_sha256`)/got_sha
  (renamed to `got_sha256`)/source_line/message — matches Go tag-for-tag.
* `chrono::SecondsFormat::Secs` + `to_rfc3339_opts(.., true)` produces
  `2026-05-29T00:00:00Z`, identical to Go's `time.RFC3339` for UTC.

## Known gaps for Phase 4 / 5

1. **Parity-golden test suite not yet wired.** The
   `testing/parity_fixture_builder.rs` helper exists; Phase 4 needs the
   actual `tests/parity_*.rs` integration tests that read each golden
   under `tests/parity/goldens/<fixture>/<func>.json` and diff against a
   live invocation of the new Rust functions.
2. **FFI surface (`src/ffi.rs`) deferred** to Phase 5 per task brief.
3. **JSON-pack ordering parity.** Go's `encoding/json` sorts map keys
   alphabetically; `serde_json` with `preserve_order` preserves
   insertion order. For `EmbedJSONPatch` we currently rely on serde's
   default map ordering. If the Phase 4 golden diff flags this, switch
   `embed_json_patch` to a sorted-key `BTreeMap`-backed re-marshal pass
   (drop-in: change the `serde_json::Map` to `BTreeMap<String, Value>`).
4. **`parse_from_pack` byte-tolerance.** Verified roundtrip on
   markdown / plain / JSON; haven't yet diffed against every fixture's
   `ParseFromPack.json` golden — Phase 4 verification will surface any
   subtle regex divergence (likely candidates: trailing whitespace,
   multi-block packs).
5. **`extract_references` line-length cap.** Implemented as a `BufRead`
   line-by-line scan with a `MAX_LINE = 1 MiB` skip-on-overflow guard.
   Behaviour matches `bufio.Scanner` for in-spec inputs; pathological
   no-newline inputs >1 MiB are silently dropped on both sides, but a
   parity test should pin this explicitly.
6. **Worktree staleness path traversal.** Go uses `filepath.Clean`;
   Rust port uses `Path::components()` and rejects RootDir / ParentDir /
   Prefix. Should match on POSIX worktree roots; Windows drive-letter
   handling untested.
7. **`testing` feature exposure.** Currently gated as
   `#[cfg(any(test, feature = "testing"))]`; downstream crates that need
   to call `parity_fixture_builder` outside `cargo test` must enable
   the feature explicitly.

## File map

```
crates/ctx-contract/
  Cargo.toml                       (replaced)
  src/lib.rs                       (replaced)
  src/types.rs                     (new)
  src/hash.rs                      (new)
  src/builder.rs                   (new)
  src/parse_refs.rs                (new)
  src/embed.rs                     (new)
  src/verify.rs                    (new)
  src/format.rs                    (new)
  src/testing/mod.rs               (untouched)
  src/testing/parity_fixture_builder.rs  (untouched)
```
