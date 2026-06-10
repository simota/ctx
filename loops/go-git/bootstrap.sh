#!/usr/bin/env bash
# bootstrap.sh — pin the gate files + record the parity baseline counts.
# Run ONCE, AFTER the PHASE A oracle (crates/ctx-web/tests/git_parity.rs) exists
# and is RED against current Rust. Re-run only to intentionally re-pin.
set -eu
SELF="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PINS="$SELF/.pins"; mkdir -p "$PINS"
REPO="$(cd "$SELF/../.." && pwd)"
cd "$REPO"

# --- pin gate files (mutation during a run = ABORT) ---
: > "$PINS/gatefiles.sha256"
for f in \
  loops/go-git/goal.md \
  loops/go-git/verify.sh \
  loops/go-git/run-loop.sh \
  crates/ctx-web/tests/git_parity.rs
do
  [ -f "$REPO/$f" ] || { echo "bootstrap: missing gate file $f (author the oracle first)"; exit 1; }
  echo "$(shasum -a 256 "$REPO/$f" | awk '{print $1}') $f" >> "$PINS/gatefiles.sha256"
done
echo "pinned $(wc -l < "$PINS/gatefiles.sha256") gate files"

# --- record baseline parity counts ---
# BASE_* = non-regression FLOOR (passing now); must never drop below.
# BASE_GIT_TOTAL = the DONE target (total git_parity cases) — loop is done when
# git_parity passing == BASE_GIT_TOTAL.
count_pass()  { cargo test --manifest-path "$1" --test "$2" ${3:-} 2>&1 | grep -oE "[0-9]+ passed" | awk '{s+=$1} END{print s+0}'; }
# git_parity suite is RED (exits non-zero) — `|| true` so set -e doesn't abort.
git_out=$(cargo test --manifest-path crates/ctx-web/Cargo.toml --test git_parity 2>&1 || true)
git_pass=$(echo "$git_out" | grep -oE "[0-9]+ passed" | awk '{s+=$1} END{print s+0}')
git_fail=$(echo "$git_out" | grep -oE "[0-9]+ failed" | awk '{s+=$1} END{print s+0}')
{
  echo "BASE_CLI=$(count_pass crates/ctx-cli/Cargo.toml parity)"
  echo "BASE_WEB=$(count_pass crates/ctx-web/Cargo.toml parity)"
  echo "BASE_SYM=$(count_pass crates/ctx-symbols/Cargo.toml parity '--features testing')"
  echo "BASE_MCP=$(count_pass crates/ctx-mcp/Cargo.toml parity)"   # mcp is DONE on main; floor = full count (22)
  echo "BASE_GIT=$git_pass"                       # floor: currently-green git cases (expect 0)
  echo "BASE_GIT_TOTAL=$((git_pass + git_fail))"  # DONE target: all git cases (expect 13)
} > "$PINS/baseline_counts.env"
cat "$PINS/baseline_counts.env"
echo "bootstrap complete — review baseline_counts.env (BASE_GIT should be 0, BASE_GIT_TOTAL 13), then launch run-loop.sh"
