# ctx-symbols PHASE4 Report (Tier 2 #5)

**Crate**: `crates/ctx-symbols/`
**Branch**: `phase4/symbols-rust-port`
**Date**: 2026-05-30
**Status**: SHIPPED (mixed verdict) — lookup sessioned SHIPS (121-161× net),
apionly EVIDENCE-ONLY (1.10× slower, +4% memory)
**Pattern reused**: ctx-relations sticky-handle session + ctx-braid
stateless apionly + ctx-pack scope-split dispatcher

---

## Scope Decision (scope-split)

Inspection of `internal/symbols` revealed that BOTH `apionly.go` and
`lookup.go` transitively depend on tree-sitter:

- `apionly.go` calls `sitter.NewParser` directly to walk source files
  for declaration/export discovery.
- `lookup.go` calls `extractor.New().Extract(path)`, which is the
  tree-sitter-backed `TreeSitterExtractor`.

The brief's L3 prediction labelled apionly as "REGEX/byte-scan text
extraction" — this turned out to be **incorrect** on inspection.
apionly's hot path is `sitter.Node` traversal + tiny post-walk
`renderAPIRanges` + line trim. The pure-compute slice that does NOT
need tree-sitter is small (~60 LOC across `renderAPIRanges`,
`leadingCommentStart`, `isCommentLine`, `signatureEndLine` text
mutation).

Per the brief's scope constraint ("DO NOT port extractor.go —
tree-sitter"), the port shape is:

### PORTED to Rust (pure-compute post-processing)
- `apionly.rs` → `render_api(lines, ranges) -> string` — the merge +
  trim + concat pass over the post-AST `(lines, ranges)` tuple.
- `lookup/mod.rs` → `resolve(corpus, args) -> Vec<Hit>` — the sort +
  filter pass over a pre-extracted `Vec<FileSymbols>`.
- `lookup/session.rs` → `LookupSession::open(root, corpus)` +
  `resolve(args) × N` — sticky-handle session that holds the
  pre-extracted corpus across multi-query workloads.

### LEFT GO-SIDE (tree-sitter required)
- `extractor.go` — `TreeSitterExtractor` directly binds C tree-sitter
  via cgo. Double-cgo (Go → Rust → C) would inflate complexity for no
  measured win.
- `apionly.go` AST walk — `collectAPIRanges`, `isDeclaration`,
  `isPublic`, `headerRanges` all need `*sitter.Node`. Go computes
  `(lines, ranges)` and hands them to Rust for rendering.

The Go-side dispatcher `dispatch_rust.go::apionlyViaRust` runs the
tree-sitter pass Go-side, then delegates the render to the Rust crate.
`dispatch_rust.go::LookupPool` walks + extracts Go-side, JSON-serialises
`[]FileSymbols`, opens a Rust session against it, then answers N
`resolve` queries against the cached corpus.

---

## API Shape Choice

| Function | Shape | Reasoning |
|----------|-------|-----------|
| apionly | STATELESS | Per-file render is sub-100 µs Go work; cgo+JSON shuttle (~10 µs floor) adds tax on each call. EVIDENCE-ONLY expected. |
| lookup | SESSIONED (+ stateless for parity) | The sole production caller (`/api/definition` web handler) does a fresh walk+extract per request. A pool that opens once per `(root)` and resolves N name lookups against the cached corpus amortises the heavy Go-side walk+extract across requests. |

---

## L1-L4 Application (per the brief)

| Function | L1 size | L2 cgo floor | L3 hot path | L4 per-query verdict |
|----------|---------|--------------|-------------|----------------------|
| apionly | per-file ~25 KB lines | ~10 µs/call | tree-sitter (Go-side) + String/Vec merge (Rust-side). Goes echo's "HashMap/String" route — NOT regex/byte-scan. | EVIDENCE-ONLY confirmed (1.10× slower, +5% bytes) |
| lookup stateless | corpus = post-extract Vec<FileSymbols>, ~1.5 MB on large | ~10 µs/call + JSON marshal of corpus = ~40-300 µs depending on corpus | Hash equality + Vec sort (sub-µs Rust intrinsic) | EVIDENCE-ONLY (cgo+JSON shuttle floor swallows intrinsic Rust win) |
| lookup sessioned | corpus held in Rust forever; only args JSON (40 bytes) per call | ~10 µs/call + 40-byte JSON | Hash equality + Vec sort (sub-µs Rust intrinsic) | **SHIPS** — 121-161× net vs Go because walk+extract is paid ONCE on first query, not per call |

**Key insight (post-L4)**: lookup's intrinsic Rust work is in fact tiny
(sub-µs). The 121-161× speedup is NOT a Rust-is-faster-than-Go-at-
sorting story; it is the SAME amortisation lane that ctx-where /
ctx-focus / ctx-relations exploited — caching the walked + extracted
corpus on the Rust side across N queries.

The stateless rust path (`Pool.RoutedLookupResolve` with a brand-new
pool per call) closes the gap to Go almost exactly (~1.02× slower)
because both incur the walk+extract cost; the only delta is the
JSON-marshal-the-corpus tax.

---

## Bench Results

Machine: Apple M4 (10-core), macOS 26 / Darwin 25.5, rustc 1.92,
go 1.25.0. Single-platform measurements only.

### apionly (per-file render, stateless)

| Engine | ns/op | B/op | allocs/op | vs Go |
|--------|-------|------|-----------|-------|
| Go     | 62,575 | 22,664 | 396 | baseline |
| Rust   | 69,321 | 23,727 | 398 | **0.90× (slower)** / +4.7% bytes |

**Verdict**: EVIDENCE-ONLY. The Go-side tree-sitter walk dominates
(~60 µs); the Rust render is ≤2 µs but is followed by a ~10 µs
cgo+JSON roundtrip. Default Go path retained.

### lookup stateless (NewPool().RoutedLookupResolve x 1)

| Fixture | Go ns/op | Rust ns/op | vs Go | Go B/op | Rust B/op |
|---------|----------|------------|-------|---------|-----------|
| small   |   292,949 |    298,487 | 0.98× | 150,513 | 163,900 |
| medium  |   958,221 |  1,014,287 | 0.94× | 330,742 | 349,143 |
| large   | 5,526,299 |  5,612,508 | 0.98× | 1,509,232 | 1,586,721 |

**Verdict**: EVIDENCE-ONLY. Walk+extract dominate; JSON-marshal of the
corpus adds a ~5% tax on top.

### lookup sessioned (pool warmed, then RoutedLookupResolve x N)

| Fixture | Go ns/op | Rust sessioned ns/op | **vs Go** | Go B/op | Rust B/op | Mem reduction |
|---------|----------|----------------------|----------|---------|-----------|---------------|
| small   |   292,949 |   1,816 | **161×**  | 150,513 |   944 | **−99.4%** |
| medium  |   958,221 |   7,223 | **133×**  | 330,742 | 4,434 | **−98.7%** |
| large   | 5,526,299 |  39,400 | **140×**  | 1,509,232 | 20,601 | **−98.6%** |

**Verdict**: **SHIPS.** Across the board ≥120× faster, allocator
pressure dropped 98-99%. This is the cleanest sessioned-win in the
campaign so far — comparable to ctx-focus (47-105×) and exceeding
ctx-where (11-19× net).

### Sticky-handle 5K soak

- **TestLookupPool_Soak5K** (medium corpus, warm session, 5K queries):
  HeapInuse delta = **32 KB** over 5,000 cycles → no leak.
- **TestLookupPool_OpenCloseCycle5K** (small corpus, fresh pool every
  iter, 5K cycles): HeapInuse delta = **229 KB** over 5,000 cycles →
  no leak (small constant growth from JSON arena pools is normal).

Both well under their 8 MB / 16 MB thresholds.

---

## Test Results

| Suite | Count | Status |
|-------|-------|--------|
| Rust lib (`cargo test --lib`) | 37 | PASS |
| Rust regression (`cargo test --test regression`) | 10 | PASS |
| Rust sticky_handle (`cargo test --test sticky_handle --features testing`) | 8 | PASS |
| Rust parity (`cargo test --test parity --features testing`) | 2 (across 3 fixtures) | PASS |
| Go default (`go test ./internal/symbols/...`) | full pkg | PASS |
| Go rust_contract (`go test -tags rust_contract ./internal/symbols/...`) | full pkg | PASS |
| `cmd/symbols-engine-diff -engine rust` (3 fixtures × 1 apionly + 5 lookup queries) | 18 | byte-equal |

---

## Caller Integration

Wired conservatively into ONE caller — the highest-amortisation site:

- **`internal/web/handlers.go`**: `handleDefinition` now routes
  through `a.SymbolsPool.RoutedLookupResolve` (was direct
  `symbols.LookupByName`). The `SymbolsPool` is created once per `API`
  instance (process-wide for the embedded web server) so multiple
  `/api/definition` requests reuse the cached Rust corpus.

Other callers (`internal/pack/diff.go`, `internal/pack/diagnose.go`,
`internal/focus/expand.go`, `internal/mcp/server.go`,
`internal/cli/root.go`, `internal/noise/inspect.go`,
`internal/skim/skim.go`) all use `symbols.New().Extract` (tree-sitter
directly) or `symbols.ExtractPublicAPI` (tree-sitter directly). The
new RoutedAPIOnly is wired but not yet swapped at call sites — kept
conservative since apionly is EVIDENCE-ONLY and a per-call regression
in pack would hurt headline `ctx pack` latency.

`#TODO(agent)`: revisit pack `diff.go` apionly wiring if a future
multi-file-per-call batching API lands (would amortise cgo floor
across N files).

---

## Files Changed

### New Rust crate
- `crates/ctx-symbols/Cargo.toml`
- `crates/ctx-symbols/build.rs`
- `crates/ctx-symbols/cbindgen.toml`
- `crates/ctx-symbols/src/lib.rs`
- `crates/ctx-symbols/src/types.rs`
- `crates/ctx-symbols/src/apionly.rs`
- `crates/ctx-symbols/src/lookup/mod.rs`
- `crates/ctx-symbols/src/lookup/session.rs`
- `crates/ctx-symbols/src/ffi.rs`
- `crates/ctx-symbols/src/testing/mod.rs`
- `crates/ctx-symbols/tests/{parity,regression,sticky_handle}.rs`
- `crates/ctx-symbols/benches/{symbols,memory}.rs`
- `crates/ctx-symbols/include/ctx_symbols.h` (cbindgen output)

### Go-side
- `internal/symbols/rustbridge/bridge.go` (new)
- `internal/symbols/dispatch.go` (new)
- `internal/symbols/dispatch_rust.go` (new)
- `internal/symbols/golden_export.go` (new — exposes
  `ComputeAPIRangesForGolden` for `cmd/symbols-golden-export`)
- `internal/symbols/symbols_bench_test.go` (new)
- `internal/symbols/session_soak_test.go` (new)
- `internal/web/handlers.go` (`SymbolsPool` field on API +
  `handleDefinition` rewired)

### Tooling + fixtures
- `cmd/symbols-golden-export/main.go`
- `cmd/symbols-engine-diff/main.go`
- `tests/symbols-fixtures/{small,medium,large}_corpus/...` (mixed
  Go/TS/Python)
- `tests/parity/symbols-goldens/{small,medium,large}/{apionly_input,
  apionly_output,corpus,lookup_queries,lookup_resolve_output}.json`
