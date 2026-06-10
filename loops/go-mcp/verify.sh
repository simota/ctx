#!/usr/bin/env bash
# verify.sh — DONE gate for the MCP parity loop. Emits SUCCESS/FAILED + footer.
# Runs from within the iteration worktree (cwd = repo root of that worktree).
#
# Gates (all must pass; any failure => CONTINUE, never DONE):
#   1. gate files (goal.md, verify.sh, run-loop.sh, tests/parity.rs) sha256-pinned
#      — mutation = REWARD_HACK/GOAL_DRIFT (AP-13/AP-16).
#   2. Go untouched (internal/**, cmd/**) vs origin/main.
#   3. go build ./... clean.
#   4. four differential-parity suites GREEN (cli, web, symbols, MCP).
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
go build ./... >/tmp/go-build-mcp.log 2>&1 || fail "go build ./... broken (see /tmp/go-build-mcp.log)"

# --- Gate 4: the THREE already-complete suites must be FULLY GREEN ---
# (cli/web/symbols are done; the loop must not regress them — require ok.)
run_green() {  # name manifest extra-args  -> echoes passing count, fails if not ok
  out=$(cargo test --manifest-path "$2" --test parity $3 2>&1)
  echo "$out" | grep -q "test result: ok" || { echo "$out" | tail -20; fail "$1 parity suite FAILED (must stay fully green)"; }
  echo "$out" | grep -oE "[0-9]+ passed" | awk '{s+=$1} END{print s+0}'
}
CLI_PASS=$(run_green ctx-cli     crates/ctx-cli/Cargo.toml      "")
WEB_PASS=$(run_green ctx-web     crates/ctx-web/Cargo.toml      "")
SYM_PASS=$(run_green ctx-symbols crates/ctx-symbols/Cargo.toml  "--features testing")

# --- ctx-mcp is the IN-PROGRESS suite: partial-green is expected mid-loop.
# Do NOT require "ok"; count passing per-case tests. The DONE gate (in
# run-loop.sh) requires the FULL count (BASE_MCP_TOTAL). Here we only enforce
# a non-regression FLOOR so an iteration can't make a previously-green case red.
mcp_out=$(cargo test --manifest-path crates/ctx-mcp/Cargo.toml --test parity 2>&1)
echo "$mcp_out" | grep -qE "test result: (ok|FAILED)" || { echo "$mcp_out" | tail -20; fail "ctx-mcp parity suite did not run (compile/spawn error — not a real RED)"; }
MCP_PASS=$(echo "$mcp_out" | grep -oE "[0-9]+ passed" | awk '{s+=$1} END{print s+0}')

# --- Gate 5: case-count non-regression floor ---
if [ -f "$PINS/baseline_counts.env" ]; then
  . "$PINS/baseline_counts.env"
  [ "$CLI_PASS" -ge "${BASE_CLI:-0}" ] || fail "ctx-cli parity count regressed ($CLI_PASS < $BASE_CLI)"
  [ "$WEB_PASS" -ge "${BASE_WEB:-0}" ] || fail "ctx-web parity count regressed ($WEB_PASS < $BASE_WEB)"
  [ "$SYM_PASS" -ge "${BASE_SYM:-0}" ] || fail "ctx-symbols parity count regressed ($SYM_PASS < $BASE_SYM)"
  [ "$MCP_PASS" -ge "${BASE_MCP:-0}" ] || fail "ctx-mcp parity count regressed below floor ($MCP_PASS < $BASE_MCP) — a green case went red"
fi

# --- Gate 6: placeholder grep on changed Rust src (exclude tests) ---
changed_rs=$(git diff --name-only origin/main -- 'crates/**/*.rs' 2>/dev/null | grep -v '/tests/' || true)
if [ -n "$changed_rs" ]; then
  if git diff origin/main -- $changed_rs 2>/dev/null | grep -E '^\+' | grep -nE 'todo!\(|unimplemented!\(|unreachable!\("stub|// *STUB|TODO: *port' >/dev/null; then
    fail "placeholder/stub introduced in changed Rust src (AP-12)"
  fi
fi

echo "VERIFY: SUCCESS — cli=$CLI_PASS web=$WEB_PASS sym=$SYM_PASS mcp=$MCP_PASS parity green; Go untouched; go build clean; no placeholders"
echo "NEXUS_LOOP_STATUS: CONTINUE"
echo "NEXUS_LOOP_SUMMARY: gates green (cli=$CLI_PASS web=$WEB_PASS sym=$SYM_PASS mcp=$MCP_PASS); backlog drives DONE"
exit 0
