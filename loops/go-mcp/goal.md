# Loop goal — port the MCP server to native Rust at byte-parity

## Objective
Bring `crates/ctx-mcp` (Rust) to **byte-for-byte JSON-RPC parity** with the Go
MCP server (`internal/mcp`, invoked as `ctx mcp serve --root <dir>`), so the
`ctx mcp serve` command can run native instead of delegating to Go.

This is ADR-0005 Wave 2's largest single unit: `internal/mcp` is ~5333 LOC
(incl. tests); a 689-LOC unverified Rust draft exists on branch
`salvage/codex-loop-output` (`crates/ctx-mcp/src/lib.rs`) as a starting point.

## Why
MCP is the last large self-contained Go surface with a clean differential
oracle: a stdio JSON-RPC server is fully deterministic per request, so a
fixed request corpus sent to both servers byte-compares cleanly — the same
strangler/byte-parity model already proven across 20 web routes + 210 CLI cases.

## Acceptance criteria (measurable, verify.sh-gated)
1. **AC1 — differential parity harness GREEN.** A PINNED
   `crates/ctx-mcp/tests/parity.rs` boots BOTH `ctx-go mcp serve --root <fixture>`
   and the Rust `ctx-mcp` server over stdio, sends a fixed JSON-RPC request
   corpus, and asserts **byte-identical responses** (with documented `Norm`
   for non-deterministic fields only: abs paths, wall-clock). ALL cases pass.
2. **AC2 — corpus coverage + anti-escape guards.** The corpus exercises every
   method the Go server handles: `initialize`, `tools/list`, `tools/call` for
   all 8 tools (`ctx_budget`, `ctx_digest`, `ctx_focus`, `ctx_pack`,
   `ctx_roots_list`, `ctx_skim`, `ctx_symbols`, `ctx_tree`, `ctx_where`),
   `prompts/list`, `prompts/get`, `resources/list`, `resources/read`,
   `resources/templates/list`, plus the error paths (unknown method, bad params,
   path-outside-root rejection). Each case carries an `expect_contains` guard
   asserting the Go body carries meaningful shape (no both-empty/both-error
   false-PASS).
3. **AC3 — Go untouched.** `git diff origin/main -- 'internal/**' 'cmd/**'` empty.
4. **AC4 — go build clean.** `go build ./...` succeeds.
5. **AC5 — no placeholders.** No `todo!`/`unimplemented!`/`unreachable!("stub`/
   `// STUB`/`TODO: port` in added Rust src.
6. **AC6 — no collateral regression.** Existing cli/web/symbols parity suites
   stay GREEN and case counts stay **monotonic** (>= pinned baseline) — the
   loop cannot delete or weaken other routes' cases to pass.

## OUT OF SCOPE (do not attempt; not byte-parity-able in this loop)
- Non-stdio MCP transports (Go rejects them; Rust mirrors the rejection only).
- Coverage/timing/non-deterministic fields beyond the documented `Norm` set.
- Any Go-side change. If a behavior cannot be reproduced byte-exact, STOP and
  write a note to `crates/ctx-mcp/DEFERRED.md` + `fix_plan.md` — never stub.

## Precondition (PHASE A — orchestrated, NOT loop-authored)
The harness in AC1/AC2 is the immutable oracle. It MUST be authored and
sha256-pinned by a human-supervised step BEFORE the loop launches, and must be
RED against the current draft (proving it discriminates). The loop's job is to
make `ctx-mcp` pass the pinned harness — it may NEVER edit `tests/`, `verify.sh`,
`goal.md`, or `run-loop.sh` (sha256-pinned; mutation = REWARD_HACK → ABORT).

## Verification command
`bash loops/go-mcp/verify.sh`

NEXUS_LOOP_STATUS: READY
NEXUS_LOOP_SUMMARY: MCP server Go->Rust byte-parity loop; oracle pinned before launch; B1-B4 fixed
