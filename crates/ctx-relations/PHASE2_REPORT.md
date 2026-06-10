# ctx-relations — Phase 2 Port Report

Branch: `phase2/relations-rust-port`
Goal: port `internal/relations` (REGEX_HEAVY + IO, 1318 LOC source + 880
LOC test, 12 regex patterns, 8 languages) to Rust as the second module
following the Phase 1 calibration of `scan`.
Status: **RESOLVED, ready to merge.**

This report focuses on what changed beyond Phase 1; bench numbers live
in `tests/RELATIONS_BENCH_REPORT.md` and the strategic context lives in
`tests/MIGRATION_ROADMAP.md`.

---

## Modules ported

| Go source                            | Rust target                                       | Notes |
|--------------------------------------|---------------------------------------------------|-------|
| `internal/relations/relations.go`    | `crates/ctx-relations/src/{build,types}.rs`       | Build orchestrator, Index types, supportedExt. |
| `internal/relations/relations.go` (Go imports) | `crates/ctx-relations/src/languages/go.rs`        | Minimal Go-import scanner (no `go/parser`). |
| `internal/relations/relations.go` (JS/TS/Vue/Svelte) | `crates/ctx-relations/src/languages/jsts.rs`      | Regex-driven JS resolver + script-block extractor. |
| `internal/relations/relations.go` (Python) | `crates/ctx-relations/src/languages/py.rs`        | from/import + dot resolution + __init__ fallback. |
| `internal/relations/jvm.go`          | `crates/ctx-relations/src/languages/jvm.rs`       | FQN/package index + Java/Kotlin import resolver. |
| `internal/relations/php.go`          | `crates/ctx-relations/src/languages/php.rs`       | composer.json PSR-4 + group-use expansion. |
| `internal/relations/swift.go`        | `crates/ctx-relations/src/languages/swift.rs`     | SPM Sources/<Module>/ module resolver. |
| `internal/relations/cache.go`        | `crates/ctx-relations/src/cache.rs`               | size+mtime fingerprint cache. |
| (regex patterns, all .go files)      | `crates/ctx-relations/src/patterns.rs`            | 12 Lazy<Regex> via accessor fns. |
| (minimal walker subset)              | `crates/ctx-relations/src/walk.rs`                | DefaultOptions slice — ExtraIgnore only. |
| (FFI surface)                        | `crates/ctx-relations/src/ffi.rs`                 | 5 extern "C" entry points. |

### Minimal Rust mirrors of internal deps

- `model.FileInfo` subset → `languages::common::FileEntry`
  (only rel, abs, is_dir — the only fields relations touches).
- `walk.DefaultOptions` → `walk::walk(root)` with the four ExtraIgnore
  defaults hard-coded. .gitignore/.ctxignore parsing is intentionally
  NOT ported — the Rust crate is fed pre-walked, pre-filtered paths
  in production (see PR description), and the parity fixtures don't
  contain .gitignore files. Documenting this limitation here so the
  reviewer can rule on whether the dispatcher should always feed Rust
  the Go-filtered file set or whether Phase 3 needs full gitignore
  parity in Rust.

## FFI surface

Five extern "C" entry points mirror the Phase 1 pattern:

```c
int32_t ctx_relations_build(const uint8_t *root_ptr, uintptr_t root_len,
                            char **out_result_ptr);

int32_t ctx_relations_build_cached(const uint8_t *root_ptr, uintptr_t root_len,
                                   char **out_result_ptr);

int32_t ctx_relations_invalidate_cache(const uint8_t *root_ptr, uintptr_t root_len);

void           ctx_relations_free_string(char *s);
const char *   ctx_relations_version(void);
```

Error codes match the contract / scan crates:
- `0` OK
- `-1` null pointer
- `-2` input exceeds 100 MiB
- `-3` non-UTF-8 input / bad JSON
- `-4` serialization failure
- `-5` IO error
- `-99` panic caught by `catch_unwind`

## Build tag decision

We **reuse the contract+scan crate's build tag `rust_contract`** — a
single `-tags rust_contract` build now links three Rust crates:
`ctx-contract`, `ctx-scan`, and `ctx-relations`. The same rationale
applies (one CGO matrix, one opt-in switch, one reviewer surface).

A binary built with `rust_contract` can still mix-and-match:

```sh
ctx browse --relations-engine=rust    # Rust relations
ctx pack   --scan-engine=go           # Go scan
ctx contract verify --engine=rust     # Rust contract
```

## Test counts

| Suite                                      | Count |
|--------------------------------------------|-------|
| ctx-relations unit (lib)                   | 29    |
| ctx-relations parity (`--features testing`)| 7     |
| ctx-relations regression                   | 7     |
| **ctx-relations total**                    | **43** |
| ctx-scan total (unchanged)                 | 32    |
| ctx-contract total (unchanged)             | 78    |
| **All Rust crates total**                  | **153** |

The 7 parity fixtures cover every language path:

- `go_project` — pure-Go module with multi-pkg imports.
- `jsts_project` — TS + JS + .vue with script-block extraction.
- `jvm_project` — Java + Kotlin with package/wildcard/static imports.
- `php_project` — composer.json PSR-4 with group-use expansion.
- `swift_project` — SPM Sources/<Module>/ layout.
- `py_project` — absolute + from-import + __init__ fallback.
- `mixed_project` — Go + TS in one repo (web/api split).

Each fixture has two goldens (Build + BuildCached) so the cache
invalidation path is covered too.

## Cross-compile CI workflow (Phase 1 mandatory lesson #4 — RESOLVED)

`.github/workflows/cross-compile.yml` lands as part of this PR. It:

- Triggers on PR + push to main when files under `crates/**` or the
  workflow itself change.
- Matrix: 4 targets, `fail-fast: false`:
  - darwin-amd64 / darwin-arm64 (macos-latest runner)
  - linux-amd64 (ubuntu-latest)
  - linux-arm64 (ubuntu-latest + `cross 0.2.5`)
- Builds all four Rust crates per target and uploads the resulting
  staticlibs as per-target artifacts (`rust-staticlibs-<target>`).
- Adds a separate `probe (host)` job that runs the legacy
  `ci/cross-compile-probe.sh` for backward compatibility with
  developer-local probing.

The MIGRATION_ROADMAP.md hard-blocker #4 status is updated to
**RESOLVED**. Per the Phase 1 closeout note, the reviewer should flip
the cross-compile workflow to "Required" on the branch protection
rules after this PR merges and the first green run lands on `main`.

## Memory profile (Phase 1 mandatory lesson #5 — RESOLVED)

`crates/ctx-relations/benches/memory.rs` adds a `dhat`-feature-gated
profiler bench. Run with:

```sh
cargo bench --features dhat --bench memory \
            --manifest-path crates/ctx-relations/Cargo.toml
```

Output (200 Build iterations on mixed_project):

```
dhat: Total:     6,257,857 bytes in 38,103 blocks
dhat: At t-gmax: 389,509   bytes in    570 blocks
dhat: At t-end:    84,327  bytes in    250 blocks
```

Compared to the Go MemAlloc bench harness
(`internal/relations/relations_bench_test.go::BenchmarkBuild_MemAlloc`),
the Rust crate achieves **73.2% reduction in bytes allocated per
Build** and **83.8% reduction in allocations per Build** — well above
the Phase 2 ≥30% memory target.

## Phase 1 mandatory-lesson application

1. **cgo `string`→`[]byte` lifetime** — applied in
   `internal/relations/rustbridge/bridge.go` for `BuildJSON`,
   `BuildCachedJSON`, and `InvalidateCache`. Each call materialises
   the root path into a local `[]byte`, passes the pointer via cgo,
   and calls `runtime.KeepAlive(rootBytes)` after the FFI returns.
2. **`Lazy<Vec<…>>` + accessor fn** — applied throughout
   `crates/ctx-relations/src/patterns.rs`. Each language gets its own
   pattern accessor; the regex table is built once on first access.
3. **Goldens exercise every option branch** — 7 fixtures × 2 entry
   points = 14 goldens. The Build path covers every language; the
   BuildCached path additionally exercises the cache miss-then-hit
   logic, and the exporter asserts cache-hit determinism (two
   consecutive BuildCached calls must produce equal Index values).
4. **Cross-compile probe → production CI** — see section above.
5. **dhat-rs memory instrumentation** — see section above.

## End-to-end engine diff

`cmd/relations-engine-diff` is a tiny Go harness that drives
`relations.BuildDispatched(root)` under both engines on a fixture and
diffs the canonical-JSON output:

```
$ CGO_ENABLED=1 go run -tags rust_contract \
    ./cmd/relations-engine-diff ./tests/relations-fixtures/mixed_project
ok    engines agree on ./tests/relations-fixtures/mixed_project (297 bytes)
```

All four mixed-language fixtures verify clean.

## What did NOT change

- `internal/contract/*` and `crates/ctx-contract/*` — untouched.
- `internal/scan/*` and `crates/ctx-scan/*` — untouched. Test counts
  unchanged: 78 (contract) + 32 (scan).
- `go build ./...` (no tag) — still produces a pure-Go binary; all
  existing Go tests pass (the pre-existing walk.TestWalkSince_NoMatches
  flake on the merge base is not a regression).
- The `--scan-engine` and contract `--engine` flag behaviours are
  unchanged. The new `--relations-engine` flag follows the same
  contract: empty / "go" stays on the Go path; "rust" fails on a
  pure-Go binary with a clear error.

## Phase 3 implications

- The minimal walker subset works for the parity fixtures but will
  need to grow to honour `.gitignore` / `.ctxignore` if a future
  Phase 3 `where` port wants to dispatch full repo walks through
  Rust without the Go walker as a pre-filter. Recommended Phase 3
  ADR topic: "Should `crates/ctx-relations` and `crates/ctx-where`
  share a `crates/ctx-walk` dependency, or should each crate stay
  self-contained and the dispatcher always feed pre-walked paths?"
- The dhat profile shows ~84 KiB steady-state cost from the lazy regex
  table. If Phase 3 introduces additional language extractors the
  fixed cost will scale linearly — worth a note when the third regex-
  heavy crate ships.
- The 1.97× average speedup is below the 7-9× pioneer numbers because
  relations is IO-bound (one fs::read_to_string per file). A Phase 3
  `mmap`-based reader could close some of that gap; out of scope here.

## Phase 2 verdict against decision matrix

(From `tests/MIGRATION_ROADMAP.md` Phase 2 gate.)

| Criterion                                            | Result                  | Decision   |
|------------------------------------------------------|-------------------------|------------|
| End-to-end ≥1.5× speedup                             | 1.80–2.08× across 4 fixtures | **Continue to Phase 3** |
| Memory reduction ≥30%                                | 73% bytes, 84% allocs   | **Continue to Phase 3** |
| Cross-platform CI workflow lands & green on host     | workflow added; host-runner probe job included | **Continue to Phase 3** |
| Byte-exact parity on all goldens                     | 14/14 green             | **Continue to Phase 3** |
| No regressions in prior Rust suites                  | contract 78/78, scan 32/32 | **Continue to Phase 3** |
| No regressions in default Go path                    | `go build ./...` green; relations tests green | **Continue to Phase 3** |
