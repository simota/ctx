#!/usr/bin/env bash
# bootstrap.sh — pin the gate files + record the parity baseline counts.
# Run ONCE, AFTER the PHASE A oracle (crates/ctx-mcp/tests/parity.rs) exists and
# is RED against the current draft. Re-run only to intentionally re-pin.
set -eu
SELF="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PINS="$SELF/.pins"; mkdir -p "$PINS"
REPO="$(cd "$SELF/../.." && pwd)"
cd "$REPO"

# --- pin gate files (mutation during a run = ABORT) ---
: > "$PINS/gatefiles.sha256"
for f in \
  loops/go-mcp/goal.md \
  loops/go-mcp/verify.sh \
  loops/go-mcp/run-loop.sh \
  crates/ctx-mcp/tests/parity.rs
do
  [ -f "$REPO/$f" ] || { echo "bootstrap: missing gate file $f (author the oracle first)"; exit 1; }
  echo "$(shasum -a 256 "$REPO/$f" | awk '{print $1}') $f" >> "$PINS/gatefiles.sha256"
done
echo "pinned $(wc -l < "$PINS/gatefiles.sha256") gate files"

# --- record baseline parity counts ---
# BASE_* = non-regression FLOOR (passing now); must never drop below.
# BASE_MCP_TOTAL = the DONE target (total mcp cases) — loop is done when
# mcp passing == BASE_MCP_TOTAL.
count_pass()  { cargo test --manifest-path "$1" --test parity ${2:-} 2>&1 | grep -oE "[0-9]+ passed" | awk '{s+=$1} END{print s+0}'; }
# mcp suite is RED (exits non-zero) — `|| true` so set -e doesn't abort here.
mcp_out=$(cargo test --manifest-path crates/ctx-mcp/Cargo.toml --test parity 2>&1 || true)
mcp_pass=$(echo "$mcp_out" | grep -oE "[0-9]+ passed" | awk '{s+=$1} END{print s+0}')
mcp_fail=$(echo "$mcp_out" | grep -oE "[0-9]+ failed" | awk '{s+=$1} END{print s+0}')
{
  echo "BASE_CLI=$(count_pass crates/ctx-cli/Cargo.toml)"
  echo "BASE_WEB=$(count_pass crates/ctx-web/Cargo.toml)"
  echo "BASE_SYM=$(count_pass crates/ctx-symbols/Cargo.toml '--features testing')"
  echo "BASE_MCP=$mcp_pass"                       # floor: currently-green mcp cases (expect 3)
  echo "BASE_MCP_TOTAL=$((mcp_pass + mcp_fail))"  # DONE target: all mcp cases (expect 22)
} > "$PINS/baseline_counts.env"
cat "$PINS/baseline_counts.env"
echo "bootstrap complete — review baseline_counts.env (BASE_MCP should be 3, BASE_MCP_TOTAL 22), then launch run-loop.sh"
