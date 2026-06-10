#!/usr/bin/env bash
# run-loop.sh — nexus-autoloop runner, Codex CLI executor (Wave 3 cutover loop).
# Drives the PINNED differential oracle (crates/ctx-cli/tests/cutover.rs) by its
# passing-case COUNT — codex accumulates work in a worktree until all cases are
# byte-green. One iteration:
#   codex exec (advance parity, IN A WORKTREE) -> verify.sh (safety gate) ->
#   commit if non-regressing -> advance. DONE when cutover count == total.
# External termination only (iteration cap / no-work stall / circuit / USD cap).
#
# This runner is HARDENED against the failure modes that broke the prior loop
# (see loops/go-cutover/README before changing anything):
#   B1  exec sandbox: uses `-s workspace-write` explicitly (NOT --full-auto,
#       NEVER --dangerously-bypass-*). Sandbox is pinned, not env-overridable.
#   B2  isolation: codex runs in a DEDICATED git worktree on branch
#       $LOOP_BRANCH — it can NEVER mutate the primary checkout / main. A
#       cleanup trap kills the codex process + children on any exit (no orphans).
#   B5  progress model: NO model-critic (the pinned Go oracle IS the independent
#       judge — a green case is genuine byte-parity that codex cannot fake; it
#       can't edit the sha256-pinned test). Work is ACCUMULATED (committed each
#       verify-passing iter, reverted ONLY on regression), so large all-or-
#       nothing cases (e.g. tools/list = 9 schemas) can be built across iters
#       instead of being wiped by a per-iteration revert.
#   B6  stall guard: codex producing NO change for NOWORK_LIMIT iters -> BLOCK.
set -u

# --- Tunables (env overrides; sandbox + bypass are NOT overridable) ---
MAX_ITERATIONS="${MAX_ITERATIONS:-20}"
RETRY_LIMIT="${RETRY_LIMIT:-2}"
CIRCUIT_THRESHOLD="${CIRCUIT_THRESHOLD:-3}"   # consecutive identical verify FAILs
NOWORK_LIMIT="${NOWORK_LIMIT:-3}"             # consecutive iters with no codex change
USD_PER_RUN_CAP="${USD_PER_RUN_CAP:-40}"      # hard PAUSE; set 0 to disable (NOT recommended unattended)
AUTOCOMMIT="${AUTOCOMMIT:-true}"

LOOP_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$LOOP_DIR/../.." && pwd)"
LOG="$LOOP_DIR/runner.log"
LOOP_BRANCH="${LOOP_BRANCH:-loop/go-cutover}"
WT="${WT:-$REPO/../ctx-loop-go-cutover}"   # worktree OUTSIDE the repo tree

state()    { awk -F= -v k="$1" '$1==k{print $2}' "$LOOP_DIR/state.env" 2>/dev/null; }
setstate() { local t; t=$(mktemp); grep -v "^$1=" "$LOOP_DIR/state.env" 2>/dev/null > "$t"; echo "$1=$2" >> "$t"; mv "$t" "$LOOP_DIR/state.env"; }
log()      { echo "[$(date -u +%FT%TZ)] $*" | tee -a "$LOG"; }
footer()   { echo "NEXUS_LOOP_STATUS: $1"; echo "NEXUS_LOOP_SUMMARY: $2"; }

# --- B2: cleanup trap — never leave orphaned codex children ---
# No `setsid` on macOS; codex always runs with -C "$WT" so it is worktree-scoped
# (main is safe regardless). We track the codex PID and kill it + its direct
# children on any exit.
CODEX_PID=""
cleanup() {
  if [ -n "$CODEX_PID" ]; then
    pkill -TERM -P "$CODEX_PID" 2>/dev/null || true
    kill -TERM "$CODEX_PID" 2>/dev/null || true
  fi
  log "cleanup: worktree left at $WT for inspection (git worktree remove '$WT' to discard)"
}
trap cleanup EXIT INT TERM

[ -f "$LOOP_DIR/.pins/gatefiles.sha256" ] || { log "no pins — run bootstrap.sh first"; footer FAILED "not bootstrapped"; exit 1; }

# --- B2: set up the isolated worktree (idempotent) ---
cd "$REPO"
if ! git worktree list | grep -q "$WT"; then
  git show-ref --verify --quiet "refs/heads/$LOOP_BRANCH" \
    && git worktree add "$WT" "$LOOP_BRANCH" \
    || git worktree add -b "$LOOP_BRANCH" "$WT" origin/main
  log "worktree created: $WT on $LOOP_BRANCH"
fi

# cutover_count: passing cutover dispatch-assertion cases (the IN-PROGRESS suite).
cutover_count() { (cd "$WT" && cargo test --manifest-path crates/ctx-cli/Cargo.toml --test cutover 2>&1) | grep -oE "[0-9]+ passed" | awk '{s+=$1} END{print s+0}'; }
TOTAL=$(awk -F= '/^BASE_CUTOVER_TOTAL=/{print $2}' "$LOOP_DIR/.pins/baseline_counts.env" 2>/dev/null)

circuit=0; last_sig=""; nowork=0
for ((i=$(state NEXT_ITERATION); i<=MAX_ITERATIONS; i++)); do
  prev=$(cutover_count)
  if [ -n "$TOTAL" ] && [ "$prev" -ge "$TOTAL" ]; then
    log "cutover $prev/$TOTAL — DONE"
    setstate LAST_STATUS DONE
    footer DONE "Wave 3 cutover complete (mcp + web default native) ($prev/$TOTAL cases)"; exit 0
  fi
  fails=$( (cd "$WT" && cargo test --manifest-path crates/ctx-cli/Cargo.toml --test cutover 2>&1) | grep -E "^test cutover_.* FAILED|FAILED$" | grep -oE "cutover_[A-Za-z0-9_]+" | sort -u | tr '\n' ' ')
  log "iter $i — cutover $prev/$TOTAL; failing: ${fails:-<none>}"

  # --- USD cap (best-effort) ---
  if [ "$USD_PER_RUN_CAP" != "0" ]; then
    spent=$(cat "$LOOP_DIR/.usd_spent" 2>/dev/null || echo 0)
    awk "BEGIN{exit !($spent >= $USD_PER_RUN_CAP)}" && { log "USD_PER_RUN_CAP \$$USD_PER_RUN_CAP reached (spent \$$spent) — PAUSE"; footer FAILED "BURN cap reached; human resume required"; exit 2; }
  fi

  # --- EXECUTE in the worktree (accumulate; build on committed work) ---
  prompt="You are performing ADR-0005 Wave 3 CUTOVER: flip the ctx-cli dispatcher so the native Rust
implementations become the DEFAULT (no longer delegating to Go) for the MCP server and the web server.
Working dir is an isolated git worktree on branch $LOOP_BRANCH; Go source (internal/**, cmd/**) and main are off-limits.
The PINNED oracle crates/ctx-cli/tests/cutover.rs has $TOTAL dispatch-assertion tests; $prev pass, these FAIL: $fails
Make the FAILING cutover cases pass by wiring native dispatch in crates/ctx-cli/src/ (mainly main.rs), one case
at a time, BUILDING ON committed work. The two cutover changes:
  1. MCP: add the 'ctx-mcp' crate (path = ../ctx-mcp) as a ctx-cli dependency, and route 'ctx mcp serve --root <dir>
     [--allow-outside-root] [--log-file <f>]' through try_run_native to ctx_mcp::serve(stdin, stdout, ServeOptions{..})
     — mirror internal/cli/mcp.go's flag parsing. It must run NATIVE (try_run_native returns Some), no delegate.
  2. WEB DEFAULT: in parse_browse_args, change the web engine DEFAULT from the unset-empty (Go) to 'rust' so
     'ctx browse' with no --web-engine flag and no CTX_WEB_ENGINE env uses the native axum server (which already
     serves all ported routes incl. git). Honor an explicit '--web-engine go' / CTX_WEB_ENGINE=go to still delegate.
KEEP 'tui' DELEGATING to Go — it is a deliberate carve-out (not yet ported); cutover_tui_still_delegates_to_go
MUST stay green. Read crates/ctx-cli/tests/cutover.rs to see exactly how each case probes the dispatch decision
(it points CTX_GO_BIN at a sentinel stub; a delegating command hits the stub, a native one does not). Match the Go
behaviour for mcp flags by reading internal/cli/mcp.go and internal/mcp/server.go ServeOptions.
HARD RULES: the test files (crates/ctx-cli/tests/**) and loop files are READ-ONLY (chmod 0444) and sha256-pinned —
you CANNOT and MUST NOT edit them; any edit is auto-reverted and fails the gate. Make a case pass by fixing
crates/ (src), NEVER by changing the test. Never edit Go. Do not weaken existing parity. If a case is genuinely
not achievable, STOP and append a note to crates/ctx-cli/CUTOVER_DEFERRED.md instead of faking it.
Self-check ONLY this suite: cargo test --manifest-path crates/ctx-cli/Cargo.toml --test cutover <case_name>
(works in your sandbox). Do NOT run 'bash loops/go-cutover/verify.sh' or 'go build ./...' — your workspace-write
sandbox blocks writes they need (~/.ctx, go-build cache, network) so they fail spuriously and waste budget.
The RUNNER runs the full gate outside the sandbox after you exit."

  attempt=0; verify_ok=false
  while [ $attempt -le $RETRY_LIMIT ]; do
    # B7: physically protect the pinned gate files before EVERY codex attempt
    # (restore from HEAD to undo any prior-attempt reward-hack edit, then chmod
    # 0444 — git can still replace 0444 files so restores/commits work; codex
    # cannot). The oracle was observed being edited on hard cases under go-mcp.
    (cd "$WT" && git checkout HEAD -- crates/ctx-cli/tests/cutover.rs loops/go-cutover 2>/dev/null
     chmod 0444 crates/ctx-cli/tests/cutover.rs loops/go-cutover/goal.md \
                loops/go-cutover/verify.sh loops/go-cutover/run-loop.sh 2>/dev/null) || true
    # B1: pinned sandbox, no bypass. B2: -C runs codex inside the worktree.
    # </dev/null: codex exec reads stdin (would block backgrounded otherwise).
    # codex stdout → PER-ITER log so runner.log carries only the RUNNER's gate.
    codex exec -s workspace-write -C "$WT" "$prompt" </dev/null >>"$LOOP_DIR/codex-iter-$i.log" 2>&1 &
    CODEX_PID=$!; wait "$CODEX_PID" || log "codex exec exited nonzero (attempt $attempt) — see codex-iter-$i.log"
    CODEX_PID=""
    if (cd "$WT" && bash "$WT/loops/go-cutover/verify.sh") >>"$LOG" 2>&1; then verify_ok=true; break; fi
    sig=$(tail -1 "$LOG"); [ "$sig" = "$last_sig" ] && circuit=$((circuit+1)) || circuit=1; last_sig="$sig"
    [ $circuit -ge $CIRCUIT_THRESHOLD ] && { log "circuit OPEN ($circuit same failures)"; setstate LAST_STATUS BLOCKED; footer FAILED "circuit breaker OPEN — $sig"; exit 3; }
    attempt=$((attempt+1)); sleep $((2**attempt))
  done
  $verify_ok || { log "verify failed after $RETRY_LIMIT retries"; setstate LAST_STATUS BLOCKED; footer FAILED "verify gate failed at iter $i"; exit 3; }

  # --- PROGRESS EVALUATION (the pinned oracle is the judge) ---
  new=$(cutover_count)
  if [ "$new" -lt "$prev" ]; then
    log "iter $i REGRESSED cutover $prev->$new — reverting this iteration"
    (cd "$WT" && git checkout -- . 2>/dev/null; git clean -fd 2>/dev/null)
    setstate LAST_STATUS CONTINUE; continue
  fi
  if [ -z "$(cd "$WT" && git status --porcelain)" ]; then
    nowork=$((nowork+1))
    log "iter $i — codex made NO change (cutover $new/$TOTAL); no-work $nowork/$NOWORK_LIMIT"
    [ "$nowork" -ge "$NOWORK_LIMIT" ] && { log "STALL: $nowork no-work iters"; setstate LAST_STATUS BLOCKED; footer FAILED "stalled: codex produced no change for $nowork iters at cutover $new/$TOTAL"; exit 3; }
    setstate NEXT_ITERATION $((i+1)); setstate LAST_STATUS CONTINUE; continue
  fi
  nowork=0
  # commit the verify-safe, non-regressing work (accumulate)
  if [ "$AUTOCOMMIT" = "true" ]; then
    (cd "$WT" && git add -A && git commit -q -m "Wave 2 git-routes loop iter $i: parity $prev->$new/$TOTAL") 2>>"$LOG" || log "nothing to commit"
  fi
  if [ "$new" -gt "$prev" ]; then
    log "iter $i PROGRESS — cutover $prev->$new/$TOTAL (case(s) flipped byte-green)"
  else
    log "iter $i — accumulated work toward a case (cutover $new/$TOTAL, no flip yet); committed"
  fi
  setstate NEXT_ITERATION $((i+1)); setstate LAST_STATUS CONTINUE
done
log "MAX_ITERATIONS ($MAX_ITERATIONS) reached at cutover $(cutover_count)/$TOTAL"
footer CONTINUE "max iterations reached; cutover $(cutover_count)/$TOTAL; worktree at $WT"
