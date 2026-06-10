#!/usr/bin/env bash
# run-loop.sh — nexus-autoloop runner, Codex CLI executor (MCP parity loop).
# Drives the PINNED differential oracle (crates/ctx-mcp/tests/parity.rs) by its
# passing-case COUNT — codex accumulates work in a worktree until all cases are
# byte-green. One iteration:
#   codex exec (advance parity, IN A WORKTREE) -> verify.sh (safety gate) ->
#   commit if non-regressing -> advance. DONE when mcp count == total.
# External termination only (iteration cap / no-work stall / circuit / USD cap).
#
# This runner is HARDENED against the failure modes that broke the prior loop
# (see loops/go-mcp/README before changing anything):
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
LOOP_BRANCH="${LOOP_BRANCH:-loop/go-mcp}"
WT="${WT:-$REPO/../ctx-loop-go-mcp}"   # worktree OUTSIDE the repo tree

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

# mcp_count: passing parity cases in the worktree. mcp_fails: names of failing
# parity_* tests (the work list handed to codex).
mcp_count() { (cd "$WT" && cargo test --manifest-path crates/ctx-mcp/Cargo.toml --test parity 2>&1) | grep -oE "[0-9]+ passed" | awk '{s+=$1} END{print s+0}'; }
TOTAL=$(awk -F= '/^BASE_MCP_TOTAL=/{print $2}' "$LOOP_DIR/.pins/baseline_counts.env" 2>/dev/null)

circuit=0; last_sig=""; nowork=0
for ((i=$(state NEXT_ITERATION); i<=MAX_ITERATIONS; i++)); do
  prev_mcp=$(mcp_count)
  if [ -n "$TOTAL" ] && [ "$prev_mcp" -ge "$TOTAL" ]; then
    log "mcp parity $prev_mcp/$TOTAL — DONE"
    setstate LAST_STATUS DONE
    footer DONE "MCP server native at JSON-RPC byte-parity ($prev_mcp/$TOTAL cases)"; exit 0
  fi
  fails=$( (cd "$WT" && cargo test --manifest-path crates/ctx-mcp/Cargo.toml --test parity 2>&1) | grep -E "^test parity_.* FAILED|FAILED$" | grep -oE "parity_[A-Za-z0-9_]+" | sort -u | tr '\n' ' ')
  log "iter $i — mcp $prev_mcp/$TOTAL; failing: ${fails:-<none>}"

  # --- USD cap (best-effort) ---
  if [ "$USD_PER_RUN_CAP" != "0" ]; then
    spent=$(cat "$LOOP_DIR/.usd_spent" 2>/dev/null || echo 0)
    awk "BEGIN{exit !($spent >= $USD_PER_RUN_CAP)}" && { log "USD_PER_RUN_CAP \$$USD_PER_RUN_CAP reached (spent \$$spent) — PAUSE"; footer FAILED "BURN cap reached; human resume required"; exit 2; }
  fi

  # --- EXECUTE in the worktree (accumulate; build on committed work) ---
  prompt="You are porting the Go MCP server to native Rust at byte-for-byte JSON-RPC parity.
Working dir is an isolated git worktree on branch $LOOP_BRANCH; Go source and main are off-limits.
The PINNED oracle crates/ctx-mcp/tests/parity.rs has 22 per-case tests; $prev_mcp pass, these FAIL: $fails
Pick the SINGLE most tractable failing case and make it FULLY byte-parity this iteration, BUILDING ON the
code already committed in crates/ctx-mcp/ (prior iterations' work is committed — do not redo it). Finish one
case before starting another. Read for context (do NOT assume): the failing test(s) in
crates/ctx-mcp/tests/parity.rs (corpus request + expect_contains guards), internal/mcp/server.go (the Go
behaviour to match byte-exact), and the current crates/ctx-mcp/src/.
HARD RULES: the test files (crates/ctx-mcp/tests/**) and loop files are READ-ONLY (chmod 0444) and sha256-
pinned — you CANNOT and MUST NOT edit them; any edit is auto-reverted and fails the gate. Make a case pass by
fixing crates/ctx-mcp/src/, NEVER by changing the test. Never edit Go (internal/**, cmd/**). Reuse the native
ctx-* crates (no logic reimplementation); make the handler run NATIVE (no delegate/stub). If a case is
genuinely not byte-parity-able, STOP and append a note to crates/ctx-mcp/DEFERRED.md instead of stubbing.
Self-check ONLY your crate: cargo test --manifest-path crates/ctx-mcp/Cargo.toml --test parity <case_name>
(works in your sandbox). Do NOT run 'bash loops/go-mcp/verify.sh' or 'go build ./...' — your workspace-write
sandbox blocks writes they need (~/.ctx, go-build cache, network) so they fail spuriously and waste budget.
The RUNNER runs the full gate outside the sandbox after you exit."

  attempt=0; verify_ok=false
  while [ $attempt -le $RETRY_LIMIT ]; do
    # B7: physically protect the pinned gate files before EVERY codex attempt.
    # codex was observed making a 71-/34-line reward-hack edit to the oracle on a
    # hard case; the sha pin caught it but the persistent mutation circuit-broke
    # the retry. Restore the gate files from HEAD (undo any prior-attempt edit)
    # then chmod 0444 so codex's workspace-write cannot rewrite them. git can
    # still replace 0444 files, so the next restore works; codex's editor cannot.
    (cd "$WT" && git checkout HEAD -- crates/ctx-mcp/tests loops/go-mcp 2>/dev/null
     chmod 0444 crates/ctx-mcp/tests/parity.rs loops/go-mcp/goal.md \
                loops/go-mcp/verify.sh loops/go-mcp/run-loop.sh 2>/dev/null) || true
    # B1: pinned sandbox, no bypass. B2: -C runs codex inside the worktree.
    # </dev/null: codex exec reads stdin (would block backgrounded otherwise).
    # codex stdout → PER-ITER log so runner.log carries only the RUNNER's gate.
    codex exec -s workspace-write -C "$WT" "$prompt" </dev/null >>"$LOOP_DIR/codex-iter-$i.log" 2>&1 &
    CODEX_PID=$!; wait "$CODEX_PID" || log "codex exec exited nonzero (attempt $attempt) — see codex-iter-$i.log"
    CODEX_PID=""
    if (cd "$WT" && bash "$WT/loops/go-mcp/verify.sh") >>"$LOG" 2>&1; then verify_ok=true; break; fi
    sig=$(tail -1 "$LOG"); [ "$sig" = "$last_sig" ] && circuit=$((circuit+1)) || circuit=1; last_sig="$sig"
    [ $circuit -ge $CIRCUIT_THRESHOLD ] && { log "circuit OPEN ($circuit same failures)"; setstate LAST_STATUS BLOCKED; footer FAILED "circuit breaker OPEN — $sig"; exit 3; }
    attempt=$((attempt+1)); sleep $((2**attempt))
  done
  $verify_ok || { log "verify failed after $RETRY_LIMIT retries"; setstate LAST_STATUS BLOCKED; footer FAILED "verify gate failed at iter $i"; exit 3; }

  # --- PROGRESS EVALUATION (the pinned oracle is the judge) ---
  new_mcp=$(mcp_count)
  if [ "$new_mcp" -lt "$prev_mcp" ]; then
    log "iter $i REGRESSED mcp $prev_mcp->$new_mcp — reverting this iteration"
    (cd "$WT" && git checkout -- . 2>/dev/null; git clean -fd 2>/dev/null)
    setstate LAST_STATUS CONTINUE; continue
  fi
  if [ -z "$(cd "$WT" && git status --porcelain)" ]; then
    nowork=$((nowork+1))
    log "iter $i — codex made NO change (mcp $new_mcp/$TOTAL); no-work $nowork/$NOWORK_LIMIT"
    [ "$nowork" -ge "$NOWORK_LIMIT" ] && { log "STALL: $nowork no-work iters"; setstate LAST_STATUS BLOCKED; footer FAILED "stalled: codex produced no change for $nowork iters at mcp $new_mcp/$TOTAL"; exit 3; }
    setstate NEXT_ITERATION $((i+1)); setstate LAST_STATUS CONTINUE; continue
  fi
  nowork=0
  # commit the verify-safe, non-regressing work (accumulate)
  if [ "$AUTOCOMMIT" = "true" ]; then
    (cd "$WT" && git add -A && git commit -q -m "Wave 2 MCP loop iter $i: parity $prev_mcp->$new_mcp/$TOTAL") 2>>"$LOG" || log "nothing to commit"
  fi
  if [ "$new_mcp" -gt "$prev_mcp" ]; then
    log "iter $i PROGRESS — mcp $prev_mcp->$new_mcp/$TOTAL (case(s) flipped byte-green)"
  else
    log "iter $i — accumulated work toward a case (mcp $new_mcp/$TOTAL, no flip yet); committed"
  fi
  setstate NEXT_ITERATION $((i+1)); setstate LAST_STATUS CONTINUE
done
log "MAX_ITERATIONS ($MAX_ITERATIONS) reached at mcp $(mcp_count)/$TOTAL"
footer CONTINUE "max iterations reached; mcp $(mcp_count)/$TOTAL; worktree at $WT"
