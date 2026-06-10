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
  loops/go-cutover/goal.md \
  loops/go-cutover/verify.sh \
  loops/go-cutover/run-loop.sh \
  crates/ctx-cli/tests/cutover.rs
do
  [ -f "$REPO/$f" ] || { echo "bootstrap: missing gate file $f (author the oracle first)"; exit 1; }
  echo "$(shasum -a 256 "$REPO/$f" | awk '{print $1}') $f" >> "$PINS/gatefiles.sha256"
done
echo "pinned $(wc -l < "$PINS/gatefiles.sha256") gate files"

# --- record baseline parity counts ---
# BASE_* = non-regression FLOOR (passing now); must never drop below.
# BASE_CUTOVER_TOTAL = the DONE target (total cutover cases) — loop is done when
# cutover passing == BASE_CUTOVER_TOTAL.
count_pass()  { cargo test --manifest-path "$1" --test "$2" ${3:-} 2>&1 | grep -oE "[0-9]+ passed" | awk '{s+=$1} END{print s+0}'; }
# cutover suite is RED (exits non-zero) — `|| true` so set -e doesn't abort.
cut_out=$(cargo test --manifest-path crates/ctx-cli/Cargo.toml --test cutover 2>&1 || true)
cut_pass=$(echo "$cut_out" | grep -oE "[0-9]+ passed" | awk '{s+=$1} END{print s+0}')
cut_fail=$(echo "$cut_out" | grep -oE "[0-9]+ failed" | awk '{s+=$1} END{print s+0}')
{
  echo "BASE_CLI=$(count_pass crates/ctx-cli/Cargo.toml parity)"
  echo "BASE_WEB=$(count_pass crates/ctx-web/Cargo.toml parity)"
  echo "BASE_SYM=$(count_pass crates/ctx-symbols/Cargo.toml parity '--features testing')"
  echo "BASE_MCP=$(count_pass crates/ctx-mcp/Cargo.toml parity)"
  echo "BASE_GIT=$(count_pass crates/ctx-web/Cargo.toml git_parity)"  # git done on main; floor = 13
  echo "BASE_CUTOVER=$cut_pass"                       # floor: currently-green cutover cases (expect 2 locks)
  echo "BASE_CUTOVER_TOTAL=$((cut_pass + cut_fail))"  # DONE target: all cutover cases (expect 5)
} > "$PINS/baseline_counts.env"
cat "$PINS/baseline_counts.env"
echo "bootstrap complete — review baseline_counts.env (BASE_CUTOVER 2, BASE_CUTOVER_TOTAL 5), then launch run-loop.sh"
