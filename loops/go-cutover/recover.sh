#!/usr/bin/env bash
# recover.sh — reset loop state after a circuit OPEN / pause / drift.
#   --reset-circuit  clear the circuit counter (state only; next run probes again)
#   --reset-state    set NEXT_ITERATION=1, LAST_STATUS=READY
#   --drop-worktree  remove the isolation worktree (discards uncommitted iter work)
set -eu
SELF="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$SELF/../.." && pwd)"
WT="${WT:-$REPO/../ctx-loop-go-cutover}"
setstate() { local t; t=$(mktemp); grep -v "^$1=" "$SELF/state.env" 2>/dev/null > "$t"; echo "$1=$2" >> "$t"; mv "$t" "$SELF/state.env"; }

case "${1:-}" in
  --reset-circuit) setstate LAST_STATUS READY; echo "circuit/state reset to READY" ;;
  --reset-state)   setstate NEXT_ITERATION 1; setstate LAST_STATUS READY; echo "state reset (iter 1, READY)" ;;
  --drop-worktree) git -C "$REPO" worktree remove --force "$WT" 2>/dev/null && echo "worktree removed: $WT" || echo "no worktree at $WT" ;;
  *) echo "usage: recover.sh [--reset-circuit|--reset-state|--drop-worktree]"; exit 1 ;;
esac
