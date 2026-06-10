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
  loops/go-tui/goal.md \
  loops/go-tui/verify.sh \
  loops/go-tui/run-loop.sh \
  crates/ctx-tui/tests/snapshot.rs \
  crates/ctx-tui/tests/goldens/nav_toggle_open.txt \
  crates/ctx-tui/tests/goldens/expand_all_scroll.txt
do
  [ -f "$REPO/$f" ] || { echo "bootstrap: missing gate file $f (author the oracle first)"; exit 1; }
  echo "$(shasum -a 256 "$REPO/$f" | awk '{print $1}') $f" >> "$PINS/gatefiles.sha256"
done
echo "pinned $(wc -l < "$PINS/gatefiles.sha256") gate files"

# --- record baseline parity counts ---
# BASE_* = non-regression FLOOR (passing now); must never drop below.
# BASE_TUI_TOTAL = the DONE target (total snapshot sessions) — loop is done when
# tui snapshot passing == BASE_TUI_TOTAL.
count_pass()  { cargo test --manifest-path "$1" --test "$2" ${3:-} 2>&1 | grep -oE "[0-9]+ passed" | awk '{s+=$1} END{print s+0}'; }
# snapshot suite is RED (exits non-zero) — `|| true` so set -e doesn't abort.
tui_out=$(cargo test --manifest-path crates/ctx-tui/Cargo.toml --test snapshot 2>&1 || true)
tui_pass=$(echo "$tui_out" | grep -oE "[0-9]+ passed" | awk '{s+=$1} END{print s+0}')
tui_fail=$(echo "$tui_out" | grep -oE "[0-9]+ failed" | awk '{s+=$1} END{print s+0}')
{
  echo "BASE_CLI=$(count_pass crates/ctx-cli/Cargo.toml parity)"
  echo "BASE_WEB=$(count_pass crates/ctx-web/Cargo.toml parity)"
  echo "BASE_SYM=$(count_pass crates/ctx-symbols/Cargo.toml parity '--features testing')"
  echo "BASE_MCP=$(count_pass crates/ctx-mcp/Cargo.toml parity)"
  echo "BASE_GIT=$(count_pass crates/ctx-web/Cargo.toml git_parity)"
  echo "BASE_CUTOVER=$(count_pass crates/ctx-cli/Cargo.toml cutover)"  # cutover done on main; floor = 5
  echo "BASE_TUI=$tui_pass"                       # floor: currently-green snapshot sessions (expect 0)
  echo "BASE_TUI_TOTAL=$((tui_pass + tui_fail))"  # DONE target: all snapshot sessions (expect 2)
} > "$PINS/baseline_counts.env"
cat "$PINS/baseline_counts.env"
echo "bootstrap complete — review baseline_counts.env (BASE_TUI 0, BASE_TUI_TOTAL 2), then launch run-loop.sh"
