# Phase 4 Verification Review — ctx-contract Rust port

Branch: `summit/contract-rust-port`
Reviewed by: Claude (judge + Sentinel)
Reviewed against: `internal/contract/*.go` (source of truth)

## Engine mode: dual-engine (Claude judge + Claude sentinel)

> Codex unavailable for Phase 4 after Phase 3 hang; quorum logic adapted —
> CONFIRMED items here would also be raised by a parity-runner pass (each
> finding cites both the Rust and Go line numbers so a future codex/agy
> diff can corroborate without re-reading the port).

Compile status: `cargo check` clean, `cargo test --lib` 19/19 pass.

---

## Findings

### CONFIRMED (Critical/High — must address Phase 5)

#### F-01 — `extract_references` over-long-line behaviour diverges from Go scanner

* Severity: **HIGH** (parity bug; affects `ExtractReferences.json` golden for any pack >1 MiB unbroken line)
* File: `crates/ctx-contract/src/parse_refs.rs:64-184`
  vs `internal/contract/parse_refs.go:78-137`
* Concurrence: **CONFIRMED (2/2)**
* Description: Go's `bufio.Scanner` with `scanner.Buffer(..., 1024*1024)` returns `false` from `Scan()` the **first time** a line exceeds 1 MiB and the loop **terminates entirely** — every subsequent line is silently dropped. The Rust port (`parse_refs.rs:93-95`) explicitly **continues** past the long line and keeps scanning. Phase 1 finding L-01 noted the PARITY comment is present, but the implementation is wrong: it advertises bufio parity but actually preserves more references than Go on pathological input.
  Additionally, the Rust loop increments `line_no` **after** the MAX_LINE skip (`parse_refs.rs:96`), while Go increments at the top of the loop (`parse_refs.go:82`). Combined, the source-line numbering for any line after a too-long line will be off-by-one (Rust uses original line numbering minus skipped lines; Go uses original line numbering until it stops).
* Fix: change the `if line.len() > MAX_LINE { continue; }` to `break;` and move `line_no += 1` to the top of the loop (immediately after the `Ok(s)` arm).

#### F-02 — `worktree_sha` rejects relative paths containing `..` that Go accepts

* Severity: **HIGH** (parity bug; trivially triggers a divergent `Verify.json` golden)
* File: `crates/ctx-contract/src/verify.rs:207-243`
  vs `internal/contract/verify.go:191-204`
* Concurrence: **CONFIRMED (2/2)**
* Description: Go runs `filepath.Clean(filepath.FromSlash(rel))` first, which collapses `a/../b` to `b`, then rejects only paths that remain absolute or start with `../`. The Rust port iterates `Path::components()` directly and rejects any `ParentDir` even when it would be cancelled by a preceding `Normal`. So for a contract path like `pkg/sub/../sub/foo.go`, Go reads `<root>/pkg/sub/foo.go`; Rust returns `"contract path cannot be resolved inside worktree"`. This affects any contract whose embedded paths weren't pre-cleaned by the pack writer.
* Fix: implement a small `clean` helper that collapses `..` against preceding `Normal` components (mirroring `filepath.Clean`), then reject only if the cleaned path is empty / absolute / still starts with `..`. Alternative: use `path-clean` crate.

#### F-03 — `parse_from_json_pack` treats `"contract": null` as absent; Go treats it as zero-Contract success

* Severity: **MEDIUM-HIGH** (parity bug; rare but golden-visible)
* File: `crates/ctx-contract/src/embed.rs:101-116`
  vs `internal/contract/embed.go:128-149`
* Concurrence: **CONFIRMED (2/2)**
* Description: Go decodes the outer object as `map[string]json.RawMessage`, looks up `"contract"`, and gates on `len(bytes.TrimSpace(raw)) == 0`. A literal JSON `null` is 4 bytes (`null`), not empty, so Go proceeds to `json.Unmarshal(raw, &c)` — which succeeds and produces a zero `Contract`, then `c.SchemaVersion == 0 → SchemaVersion`, returning `(Contract{SchemaVersion:1}, true)`. Rust short-circuits on `raw.is_null()` and returns `None`, falling back to the regex path. For a JSON pack like `{"pack":"x","contract":null}` Go reports "contract found"; Rust reports "no contract".
* Fix: drop the `raw.is_null()` guard, or replace with the byte-length check Go uses. The cleanest mirror is `if raw.to_string().trim().is_empty() { return None; }` (preserves the Go length-check but allows JSON `null` through).

#### F-04 — `(?-u:\s)` ASCII-whitespace guards missing in `parse_refs` and `embed` regexes (C-01)

* Severity: **MEDIUM** (parity bug for inputs containing Unicode whitespace — NBSP, ideographic space)
* File: `crates/ctx-contract/src/parse_refs.rs:44`
  and `crates/ctx-contract/src/embed.rs:24`
* Concurrence: **CONFIRMED (2/2)**
* Description: Go's `regexp` (RE2) treats `\s` and `\S` as ASCII-only (`[\t\n\f\r ]`). Rust's `regex` crate defaults to Unicode-aware classes — `\s` matches U+00A0, U+3000, and ~25 other code points. The diff-header regex `(?m)^\+\+\+\s+b/(\S+)` and the embed-block regex `<!-- ctx:contract v1\s*(.*?)\s*-->` will accept Unicode whitespace separators that Go rejects, producing extra references / parsing more lenient pack bodies. Phase 1 C-01 flagged exactly this; the port did not address it.
* Fix: wrap each `\s`/`\S` in `(?-u:\s)` / `(?-u:\S)`. Concretely: `r"(?m)^\+\+\+(?-u:\s)+b/((?-u:\S)+)"` and `r"(?s)(?:<!-- ctx:contract v1(?-u:\s)*(.*?)(?-u:\s)*-->|# CTX-CONTRACT v1:(?-u:\s)*(\{.*?\})(?-u:\s)*(?:\n|$))"`.

---

### LIKELY (Medium — should address)

#### F-05 — `embed_json_patch` map ordering not stable / not Go-compatible

* Severity: **MEDIUM** (byte-parity but not semantic)
* File: `crates/ctx-contract/src/embed.rs:58-70`
  vs `internal/contract/embed.go:163-185`
* Concurrence: **LIKELY (1/1)** — Phase 3 report flagged this explicitly as a known unresolved gap.
* Description: Go's `json.Marshal(map[string]json.RawMessage)` always sorts map keys alphabetically. Rust uses `serde_json::Map` with the `preserve_order` feature → insertion order. For input `{"z":1,"a":2}` Go outputs `{"a":2,"contract":...,"z":1}`; Rust outputs `{"z":1,"a":2,"contract":...}`. Any `EmbedJSONPatch.json` golden that exercises >1 key in the input pack will fail byte-diff.
* Fix: replace the `serde_json::Map<String, Value>` with a `BTreeMap<String, Value>` for the temporary re-marshal pass, then convert back through `serde_json::to_vec`. Cargo.toml already has `preserve_order` so leave that for other call-sites.

#### F-06 — `worktree_sha` leaks raw OS error strings (SENTINEL-005 + parity)

* Severity: **MEDIUM** (security info-leak + parity drift)
* File: `crates/ctx-contract/src/verify.rs:241`
  vs `internal/contract/verify.go:201`
* Concurrence: **LIKELY (1/1)**
* Description: Both implementations propagate raw OS error strings to the verify Result (`err.Error()` / `e.to_string()`). The strings differ across platforms (Go: "open /path: permission denied"; Rust: "Permission denied (os error 13)"), so any golden that exercises a permission error will diverge. Also leaks absolute paths into JSON output that may end up in logs.
* Fix: collapse to a fixed string (`"worktree file is unreadable"`) on both sides, OR have the Rust port replicate Go's exact format (`format!("open {}: {}", full.display(), e.kind_string())`) — the latter is uglier but preserves parity.

#### F-07 — `lookup_path` lowercase fallback uses ASCII-only lowercasing; Go uses full Unicode

* Severity: **MEDIUM** (parity drift; affects non-ASCII paths)
* File: `crates/ctx-contract/src/verify.rs:250-256`
  vs `internal/contract/verify.go:216-221`
* Concurrence: **LIKELY (1/1)**
* Description: Go calls `strings.ToLower(k)` which Unicode-lowercases (e.g. `İ → i̇`). Rust calls `to_ascii_lowercase()`, leaving non-ASCII chars untouched. A contract path containing Turkish/German uppercase letters would match in Go but not in Rust.
* Fix: switch both `lp` and `k.to_ascii_lowercase()` to `k.to_lowercase()` (allocates a String) — costs are negligible in this O(n) fallback and we only enter it on a miss.

#### F-08 — `extract_references` line decode silently drops non-UTF-8 input

* Severity: **MEDIUM** (parity bug; Go scanner is byte-oriented and does NOT drop)
* File: `crates/ctx-contract/src/parse_refs.rs:84-91`
  vs `internal/contract/parse_refs.go:81-83`
* Concurrence: **LIKELY (1/1)**
* Description: `BufReader::lines()` yields `io::Result<String>`, and any line containing invalid UTF-8 surfaces as `Err(InvalidData)`. The Rust port `continue`s over those, while Go's `bufio.Scanner.Text()` returns a Go string that is just a byte alias and processes happily. A response with a Latin-1 byte will produce references in Go but be silently dropped in Rust.
* Fix: switch to `BufReader::read_until(b'\n', &mut buf)` plus `String::from_utf8_lossy(&buf)` to mirror Go's "treat bytes as text" behaviour without panicking on garbage.

---

### CANDIDATE (Low — track only)

#### F-09 — `dedup_symbols` returns `Vec::new()` where Go returns `nil`

* Severity: **LOW**
* File: `crates/ctx-contract/src/builder.rs:69-87`
* Concurrence: **CANDIDATE (1/1)**
* Description: Phase 3 report calls this out — Go's `nil` slice marshals identically to an empty slice with `omitempty`, so the wire shape is the same when `File.symbols` is empty. No action needed unless future callers inspect `Some(vec)` vs `None`.

#### F-10 — Unnecessary clones in `verify::verify` hot path

* Severity: **LOW** (perf, idiomatic Rust)
* File: `crates/ctx-contract/src/verify.rs:22-32`
* Concurrence: **CANDIDATE (1/1)**
* Description: `by_path` clones each `File` into a `BTreeMap`, and `lookup_path` clones again on every hit (`f.clone()` at line 248/253). For a contract with thousands of files this doubles memory. The cleaner idiom is `BTreeMap<String, &File>` keyed by `f.path.as_str()` with the lookup returning `Option<&File>`.
* Fix: lifetime refactor — store references and propagate `&File` through the match arms. Touches the `OK { path: f.path.clone(), ... }` lines but keeps the same wire output.

#### F-11 — `Result` type-name collision with `std::result::Result`

* Severity: **LOW** (API ergonomic)
* File: `crates/ctx-contract/src/types.rs:199`
* Concurrence: **CANDIDATE (1/1)**
* Description: `pub struct Result` shadows `std::result::Result` when consumers `use ctx_contract::*`. The re-export in `lib.rs:25-28` already aliases to `VerifyResult` — but the struct itself is `Result` in `types`. Recommend renaming the struct to `VerifyResult` to match the re-export and avoid the shadow.

#### F-12 — Public API exposes `&'static str` enum reprs instead of `Display`

* Severity: **LOW** (idiomatic)
* File: `crates/ctx-contract/src/types.rs:50-59, 91-99`
* Concurrence: **CANDIDATE (1/1)**
* Description: `ReferenceKind::as_str` and `ViolationKind::as_str` should be `impl Display` so format strings (`{kind}` in renderers) work without an extra `.as_str()` call. Backward-compat: keep `as_str` as a thin wrapper.

#### F-13 — `_force_use` dead-code dummy can be deleted

* Severity: **LOW** (code hygiene)
* File: `crates/ctx-contract/src/verify.rs:279-280`
* Concurrence: **CANDIDATE (1/1)**
* Description: `Reference` is already used via the call chain `extract_references → for r in &refs`, so the dummy `_force_use` and `#[allow(dead_code)]` are unnecessary. Remove both lines.

---

## Phase 1 risk follow-up checklist

| ID | Risk | Status | Location |
|----|------|--------|----------|
| C-01 | regex parity `(?-u:\s)` | **NO** (F-04) | parse_refs.rs:44, embed.rs:24 |
| C-02 | JSON byte-divergence (collections) | **YES** | types.rs:198-213 (no skip_serializing_if on the four parity collections); Reference always-emit at types.rs:65-73 |
| C-03 | build.rs naming collision | **YES** | builder.rs (file name + module are `builder`, not `build`) |
| C-04 | Created clock seam | **YES** | builder.rs:21-40 + FROZEN_INSTANT at testing/parity_fixture_builder.rs:33 |
| C-05 | empty collections emit `[]` not `null` | **YES** | types.rs Result fields use `#[serde(default)]` without skip; default `Vec::new()` → `[]` |
| C-07 | sort order — files by path, symbols dedup+sort | **YES** | builder.rs:85, 110 |
| C-09 | FFI design | **DEFERRED** (skipped per Phase 3 brief) |
| L-01 | bufio 1MB silent drop | **PARTIAL** (F-01) — comment present but semantics wrong (`continue` instead of `break`) |
| L-02 | symlink escape | **NO** (parity mode follows symlinks via `std::fs::read`; no `follow_symlinks` opt; no PARITY note in verify.rs) |
| L-04 | `range_contained` normalises inverted range | **YES** | verify.rs:259-269 |
| L-05 | `lookupPath` whole-path lowercase fallback | **PARTIAL** (F-07) — fallback present, ASCII-only |
| L-07 | NoSymbols skips OK/Violation but counts refs | **YES** | verify.rs:97-100 (early `continue` only inside symbol arm; `references_found` is set before the loop at line 38) |
| L-08 | unbounded JSON parse in embed | **NO** (`parse_from_json_pack` calls `serde_json::from_slice(trimmed)` with no size cap; same as Go but Go is also unbounded — pre-existing risk, not regressed) |
| L-17 | High-risk Phase 2 task list | T-11 (NO, F-04), T-13 (PARTIAL, F-01/F-08), T-16 (PARTIAL, F-03), T-20 (PARTIAL, F-02), T-26 (SKIPPED), T-27 (SKIPPED) |
| SCOUT-002/007/008 | edge cases | **YES** (all preserved: `--- a/` / `+++ b/` companion skip, `looks_like_path` symbol filter, diff-header before path matcher) |
| SENTINEL-003 | ReDoS in parse regex | **YES** (no nested quantifiers, no fancy-regex; Rust `regex` crate is non-backtracking) |
| SENTINEL-005 | OS error leak | **NO** (F-06) |

---

## Verdict

**NEEDS-IMPROVEMENT** — Phase 5 mandatory.

Four CONFIRMED parity bugs (F-01 through F-04) each can produce a byte-divergent golden snapshot, so the wire-format-parity SLO is not met yet. None of them is an architectural fault — all four are localised fixes in `parse_refs.rs`, `verify.rs`, and `embed.rs` that should land in a single follow-up commit.

The port itself is structurally sound: module layout mirrors the Go source one-for-one, all 19 hand-written unit tests pass, no fancy-regex, no `unsafe`, no fabricated APIs. The CONFIRMED bugs are precisely the kind that hand-written tests can't catch — they all require the upcoming parity-golden integration suite to surface.

---

## Recommendations for Phase 5 (improvement loop)

1. **F-01** (HIGH) — flip `continue` to `break` in `extract_references` and move `line_no += 1` to the top of the loop. ~3 LOC.
2. **F-02** (HIGH) — replace the `Path::components()` walk in `worktree_sha` with a `filepath.Clean`-equivalent (use `path-clean` or hand-roll the `..`-cancelling reduce). ~15 LOC.
3. **F-04** (MEDIUM) — wrap every `\s`/`\S` in the two regexes with `(?-u:\s)` / `(?-u:\S)`. ~2 LOC. Add a regression test with `"+++\u{00A0}b/foo.go"` to lock the parity.
4. **F-03** (MEDIUM) — drop the `raw.is_null()` short-circuit in `parse_from_json_pack`. ~1 LOC.
5. **F-05** (MEDIUM) — switch the temporary map in `embed_json_patch` to `BTreeMap<String, Value>` for deterministic alphabetical key order. ~3 LOC.

Estimated patch size: ~25 LOC + ~4 new unit tests. Should fit a single Phase 5 commit comfortably under the 1-hour budget.

---

## Recommendations for follow-up Summit / apex / feature runs

* **`ffi.rs`** — full implementation (T-26). Needs cbindgen header generation, panic-catching wrappers, and a parity contract suite that exercises the C ABI from a Go callsite (so the Go `internal/contract` package can eventually be deleted).
* **CLI integration (T-27)** — wire `ctx contract verify` to call the Rust crate via FFI once F-01..F-05 land and goldens go green.
* **Hardened mode for L-02** — add a `VerifyOptions::follow_symlinks: Option<bool>` (default = parity mode `true`, opt-in `false` rejects symlinks). Doc the security trade-off explicitly.
* **L-08 unbounded JSON parse** — both Go and Rust call `Unmarshal`/`from_slice` on the entire pack body. Add a `--max-pack-bytes` guard before parsing on the CLI shim.
* **Parity-golden test suite (`tests/parity_*.rs`)** — the gap Phase 3 explicitly carved out. Should drive each golden fixture in `tests/parity/goldens/<fixture>/` through the Rust functions and `assert_eq` the JSON bytes. This is where the F-01..F-05 bugs will actually become visible.
* **`tests/parity/goldens/<fixture>/ExtractReferences.json` regeneration** — once F-04 lands (Unicode `\s` fix), regenerate the goldens from the Go side to lock the new behaviour.
* **`Result` → `VerifyResult` rename (F-11)** — public API tidy-up before any downstream crate depends on the type name.
