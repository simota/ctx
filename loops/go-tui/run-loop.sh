#!/usr/bin/env bash
# run-loop.sh — nexus-autoloop runner, Codex CLI executor (Wave 4 tui-ratatui loop).
# Drives the PINNED differential oracle (crates/ctx-tui/tests/snapshot.rs) by its
# passing-case COUNT — codex accumulates work in a worktree until all cases are
# byte-green. One iteration:
#   codex exec (advance parity, IN A WORKTREE) -> verify.sh (safety gate) ->
#   commit if non-regressing -> advance. DONE when tui snapshot count == total.
# External termination only (iteration cap / no-work stall / circuit / USD cap).
#
# This runner is HARDENED against the failure modes that broke the prior loop
# (see loops/go-tui/README before changing anything):
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
LOOP_BRANCH="${LOOP_BRANCH:-loop/go-tui}"
WT="${WT:-$REPO/../ctx-loop-go-tui}"   # worktree OUTSIDE the repo tree

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

# tui_count: passing tui snapshot cases in the worktree (the IN-PROGRESS suite).
tui_count() { (cd "$WT" && cargo test --manifest-path crates/ctx-tui/Cargo.toml --test snapshot 2>&1) | grep -oE "[0-9]+ passed" | awk '{s+=$1} END{print s+0}'; }
TOTAL=$(awk -F= '/^BASE_TUI_TOTAL=/{print $2}' "$LOOP_DIR/.pins/baseline_counts.env" 2>/dev/null)

circuit=0; last_sig=""; nowork=0
for ((i=$(state NEXT_ITERATION); i<=MAX_ITERATIONS; i++)); do
  prev=$(tui_count)
  if [ -n "$TOTAL" ] && [ "$prev" -ge "$TOTAL" ]; then
    log "tui $prev/$TOTAL — DONE"
    setstate LAST_STATUS DONE
    footer DONE "Wave 4 prereq: tui ported to ratatui ($prev/$TOTAL snapshot sessions)"; exit 0
  fi
  fails=$( (cd "$WT" && cargo test --manifest-path crates/ctx-tui/Cargo.toml --test snapshot 2>&1) | grep -E "^test snapshot_.* FAILED|FAILED$" | grep -oE "snapshot_[A-Za-z0-9_]+" | sort -u | tr '\n' ' ')
  log "iter $i — tui $prev/$TOTAL; failing: ${fails:-<none>}"

  # --- USD cap (best-effort) ---
  if [ "$USD_PER_RUN_CAP" != "0" ]; then
    spent=$(cat "$LOOP_DIR/.usd_spent" 2>/dev/null || echo 0)
    awk "BEGIN{exit !($spent >= $USD_PER_RUN_CAP)}" && { log "USD_PER_RUN_CAP \$$USD_PER_RUN_CAP reached (spent \$$spent) — PAUSE"; footer FAILED "BURN cap reached; human resume required"; exit 2; }
  fi

  # --- EXECUTE in the worktree (accumulate; build on committed work) ---
  prompt="You are porting the ctx terminal UI ('tui') from Go (Bubble Tea) to native Rust (ratatui) —
ADR-0005 Wave 4 prerequisite. The crate to build is crates/ctx-tui (ratatui).
Working dir is an isolated git worktree on branch $LOOP_BRANCH; Go source (internal/**, cmd/**) and main are off-limits.
The PINNED oracle crates/ctx-tui/tests/snapshot.rs has $TOTAL frame-snapshot sessions; $prev pass, these FAIL: $fails
Each session drives a FIXED scripted key sequence against your ctx-tui and asserts the rendered 80x24 frame TEXT
GRID (ANSI-stripped — content/layout only, NOT colors/styling) BYTE-EQUALS a golden captured from the frozen Go
tui (crates/ctx-tui/tests/goldens/<session>.txt). Make a failing session pass by implementing the ratatui render
+ update logic in crates/ctx-tui/src/, BUILDING ON committed work; finish one session before another.
Read for context (do NOT assume): the failing snapshot test in crates/ctx-tui/tests/snapshot.rs (the scripted
inputs + how it extracts the cell text grid), the golden files crates/ctx-tui/tests/goldens/*.txt (the EXACT
target frames), internal/tui/app.go (the Go behaviour to match byte-exact: Model state, Update key handling
↑↓ nav / Space toggle / Enter open / left collapse / g,G jump / p pack / q quit, View() layout, renderRow format
'%s%s%s %s %s%s' with tree connectors │  ├─ └─, markers ▾ ▸, [ ]/[x], 'N tokens', the header 'ctx tokens: N / M'
and the help line, the viewport scroll math start/end vs cursor/height, and the quirk that the root '.' renders
with the └─ connector at depth 0). The tree DATA comes from the same model the goldens used — reproduce the
exact text, spacing, and scrolling window so the ANSI-stripped grid matches byte-for-byte.
HARD RULES: the test file (crates/ctx-tui/tests/**) and the goldens (crates/ctx-tui/tests/goldens/**) and loop
files are READ-ONLY (chmod 0444) and sha256-pinned — you CANNOT and MUST NOT edit or regenerate them; any edit is
auto-reverted and fails the gate. Make a session pass by fixing crates/ctx-tui/src/, NEVER by changing the test
or goldens. Never edit Go (internal/**, cmd/**). Do not weaken existing parity in other crates. If a session is
genuinely not reproducible, STOP and append a note to crates/ctx-tui/TUI_DEFERRED.md instead of faking it.
Self-check ONLY this suite: cargo test --manifest-path crates/ctx-tui/Cargo.toml --test snapshot <session_name>
(works in your sandbox). Do NOT run 'bash loops/go-tui/verify.sh' or 'go build ./...' — your workspace-write
sandbox blocks writes they need (~/.ctx, go-build cache, network) so they fail spuriously and waste budget.
The RUNNER runs the full gate outside the sandbox after you exit."

  attempt=0; verify_ok=false
  while [ $attempt -le $RETRY_LIMIT ]; do
    # B7: physically protect the pinned gate files before EVERY codex attempt
    # (restore from HEAD to undo any prior-attempt reward-hack edit, then chmod
    # 0444 — git can still replace 0444 files so restores/commits work; codex
    # cannot). The oracle was observed being edited on hard cases under go-mcp.
    (cd "$WT" && git checkout HEAD -- crates/ctx-tui/tests loops/go-tui 2>/dev/null
     chmod 0444 crates/ctx-tui/tests/snapshot.rs crates/ctx-tui/tests/goldens/*.txt \
                loops/go-tui/goal.md loops/go-tui/verify.sh loops/go-tui/run-loop.sh 2>/dev/null) || true
    # B1: pinned sandbox, no bypass. B2: -C runs codex inside the worktree.
    # </dev/null: codex exec reads stdin (would block backgrounded otherwise).
    # codex stdout → PER-ITER log so runner.log carries only the RUNNER's gate.
    codex exec -s workspace-write -C "$WT" "$prompt" </dev/null >>"$LOOP_DIR/codex-iter-$i.log" 2>&1 &
    CODEX_PID=$!; wait "$CODEX_PID" || log "codex exec exited nonzero (attempt $attempt) — see codex-iter-$i.log"
    CODEX_PID=""
    if (cd "$WT" && bash "$WT/loops/go-tui/verify.sh") >>"$LOG" 2>&1; then verify_ok=true; break; fi
    sig=$(tail -1 "$LOG"); [ "$sig" = "$last_sig" ] && circuit=$((circuit+1)) || circuit=1; last_sig="$sig"
    [ $circuit -ge $CIRCUIT_THRESHOLD ] && { log "circuit OPEN ($circuit same failures)"; setstate LAST_STATUS BLOCKED; footer FAILED "circuit breaker OPEN — $sig"; exit 3; }
    attempt=$((attempt+1)); sleep $((2**attempt))
  done
  $verify_ok || { log "verify failed after $RETRY_LIMIT retries"; setstate LAST_STATUS BLOCKED; footer FAILED "verify gate failed at iter $i"; exit 3; }

  # --- PROGRESS EVALUATION (the pinned oracle is the judge) ---
  new=$(tui_count)
  if [ "$new" -lt "$prev" ]; then
    log "iter $i REGRESSED tui $prev->$new — reverting this iteration"
    (cd "$WT" && git checkout -- . 2>/dev/null; git clean -fd 2>/dev/null)
    setstate LAST_STATUS CONTINUE; continue
  fi
  if [ -z "$(cd "$WT" && git status --porcelain)" ]; then
    nowork=$((nowork+1))
    log "iter $i — codex made NO change (tui $new/$TOTAL); no-work $nowork/$NOWORK_LIMIT"
    [ "$nowork" -ge "$NOWORK_LIMIT" ] && { log "STALL: $nowork no-work iters"; setstate LAST_STATUS BLOCKED; footer FAILED "stalled: codex produced no change for $nowork iters at tui $new/$TOTAL"; exit 3; }
    setstate NEXT_ITERATION $((i+1)); setstate LAST_STATUS CONTINUE; continue
  fi
  nowork=0
  # commit the verify-safe, non-regressing work (accumulate)
  if [ "$AUTOCOMMIT" = "true" ]; then
    (cd "$WT" && git add -A && git commit -q -m "Wave 4 tui-ratatui loop iter $i: $prev->$new/$TOTAL sessions") 2>>"$LOG" || log "nothing to commit"
  fi
  if [ "$new" -gt "$prev" ]; then
    log "iter $i PROGRESS — tui $prev->$new/$TOTAL (case(s) flipped byte-green)"
  else
    log "iter $i — accumulated work toward a session (tui $new/$TOTAL, no flip yet); committed"
  fi
  setstate NEXT_ITERATION $((i+1)); setstate LAST_STATUS CONTINUE
done
log "MAX_ITERATIONS ($MAX_ITERATIONS) reached at tui $(tui_count)/$TOTAL"
footer CONTINUE "max iterations reached; tui $(tui_count)/$TOTAL; worktree at $WT"
