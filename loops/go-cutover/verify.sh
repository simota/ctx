#!/usr/bin/env bash
# verify.sh — DONE gate for the Wave 3 cutover loop. Emits SUCCESS/FAILED + footer.
# Runs from within the iteration worktree (cwd = repo root of that worktree).
#
# Gates (all must pass; any failure => CONTINUE, never DONE):
#   1. gate files (goal.md, verify.sh, run-loop.sh, tests/cutover.rs) sha256-pinned
#      — mutation = REWARD_HACK/GOAL_DRIFT (AP-13/AP-16).
#   2. Go untouched (internal/**, cmd/**) vs origin/main.
#   3. go build ./... clean.
#   4. cli/web/symbols/mcp/git_parity green + cutover in-progress.
#   5. parity case counts MONOTONIC (>= pinned baseline) — cases can't be deleted.
#   6. placeholder grep on changed Rust src (AP-12).
set -u
SELF="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PINS="$SELF/.pins"
REPO="$(cd "$SELF/../.." && pwd)"
cd "$REPO"
fail() { echo "VERIFY: FAILED — $1"; echo "NEXUS_LOOP_STATUS: CONTINUE"; echo "NEXUS_LOOP_SUMMARY: verify FAILED — $1"; exit 1; }

# --- Gate 1: gate-file sha256 immutability ---
while IFS= read -r line; do
  want=${line%% *}; path=${line#* }; path=${path# }
  got=$(shasum -a 256 "$REPO/$path" 2>/dev/null | awk '{print $1}')
  [ "$got" = "$want" ] || fail "gate file mutated: $path (sha256 mismatch — REWARD_HACK/GOAL_DRIFT)"
done < "$PINS/gatefiles.sha256"

# --- Gate 2: Go untouched ---
goedits=$(git diff --name-only origin/main -- 'internal/**/*.go' 'cmd/**/*.go' 2>/dev/null | head -1)
[ -z "$goedits" ] || fail "Go source changed ($goedits) — migration must not edit Go"

# --- Gate 3: go build clean ---
go build ./... >/tmp/go-build-git.log 2>&1 || fail "go build ./... broken (see /tmp/go-build-git.log)"

# --- Gate 4: the FIVE already-complete suites must stay FULLY GREEN ---
# (cli/web/symbols/mcp/git_parity are done; the cutover must not regress them.)
run_green() {  # name manifest test extra-args  -> echoes passing count, fails if not ok
  out=$(cargo test --manifest-path "$2" --test "$3" $4 2>&1)
  echo "$out" | grep -q "test result: ok" || { echo "$out" | tail -20; fail "$1 $3 suite FAILED (must stay fully green)"; }
  echo "$out" | grep -oE "[0-9]+ passed" | awk '{s+=$1} END{print s+0}'
}
CLI_PASS=$(run_green ctx-cli     crates/ctx-cli/Cargo.toml      parity "")
WEB_PASS=$(run_green ctx-web     crates/ctx-web/Cargo.toml      parity "")
SYM_PASS=$(run_green ctx-symbols crates/ctx-symbols/Cargo.toml  parity "--features testing")
MCP_PASS=$(run_green ctx-mcp     crates/ctx-mcp/Cargo.toml      parity "")
GIT_PASS=$(run_green ctx-web     crates/ctx-web/Cargo.toml      git_parity "")

# --- ctx-cli cutover is the IN-PROGRESS suite: partial-green expected mid-loop.
# Do NOT require "ok"; count passing per-case tests. The DONE gate (run-loop.sh)
# requires the FULL count (BASE_CUTOVER_TOTAL). Here we enforce only a
# non-regression FLOOR so an iteration can't make a previously-green case red.
cut_out=$(cargo test --manifest-path crates/ctx-cli/Cargo.toml --test cutover 2>&1)
echo "$cut_out" | grep -qE "test result: (ok|FAILED)" || { echo "$cut_out" | tail -20; fail "ctx-cli cutover suite did not run (compile/spawn error — not a real RED)"; }
CUT_PASS=$(echo "$cut_out" | grep -oE "[0-9]+ passed" | awk '{s+=$1} END{print s+0}')

# --- Gate 5: case-count non-regression floor ---
if [ -f "$PINS/baseline_counts.env" ]; then
  . "$PINS/baseline_counts.env"
  [ "$CLI_PASS" -ge "${BASE_CLI:-0}" ] || fail "ctx-cli parity count regressed ($CLI_PASS < $BASE_CLI)"
  [ "$WEB_PASS" -ge "${BASE_WEB:-0}" ] || fail "ctx-web parity count regressed ($WEB_PASS < $BASE_WEB)"
  [ "$SYM_PASS" -ge "${BASE_SYM:-0}" ] || fail "ctx-symbols parity count regressed ($SYM_PASS < $BASE_SYM)"
  [ "$MCP_PASS" -ge "${BASE_MCP:-0}" ] || fail "ctx-mcp parity count regressed ($MCP_PASS < $BASE_MCP)"
  [ "$GIT_PASS" -ge "${BASE_GIT:-0}" ] || fail "ctx-web git_parity count regressed ($GIT_PASS < $BASE_GIT)"
  [ "$CUT_PASS" -ge "${BASE_CUTOVER:-0}" ] || fail "ctx-cli cutover count regressed below floor ($CUT_PASS < $BASE_CUTOVER) — a green case went red"
fi

# --- Gate 6: placeholder grep on changed Rust src (exclude tests) ---
changed_rs=$(git diff --name-only origin/main -- 'crates/**/*.rs' 2>/dev/null | grep -v '/tests/' || true)
if [ -n "$changed_rs" ]; then
  if git diff origin/main -- $changed_rs 2>/dev/null | grep -E '^\+' | grep -nE 'todo!\(|unimplemented!\(|unreachable!\("stub|// *STUB|TODO: *port' >/dev/null; then
    fail "placeholder/stub introduced in changed Rust src (AP-12)"
  fi
fi

echo "VERIFY: SUCCESS — cli=$CLI_PASS web=$WEB_PASS sym=$SYM_PASS mcp=$MCP_PASS git=$GIT_PASS cutover=$CUT_PASS green; Go untouched; go build clean; no placeholders"
echo "NEXUS_LOOP_STATUS: CONTINUE"
echo "NEXUS_LOOP_SUMMARY: gates green (cli=$CLI_PASS web=$WEB_PASS sym=$SYM_PASS mcp=$MCP_PASS git=$GIT_PASS cutover=$CUT_PASS); count drives DONE"
exit 0
