#!/usr/bin/env bash
# build-go-oracle.sh — materialize and build the FROZEN Go oracle binary from the
# `go-oracle/v1` tag, NOT the working tree.
#
# ADR-0005 Wave 4: once the Go source is deleted from the tree (`internal/**`,
# `cmd/**`), the differential-parity harnesses can no longer `go build ./cmd/ctx`
# from the tree. The permanent oracle is the `go-oracle/v1` tag, which pins a
# known-good Go build (its `cmd/ctx` + `internal/` are byte-identical to the
# pre-deletion tree). This script checks out that tag's source into a cached
# worktree and builds the oracle ONCE; subsequent calls reuse the cached binary.
#
# Output: prints the absolute path of the frozen oracle binary on stdout.
# The parity harnesses honor CTX_GO_BIN; CI / local runs set it from this script:
#     export CTX_GO_BIN="$(bash ci/build-go-oracle.sh)"
set -eu

ORACLE_TAG="${ORACLE_TAG:-go-oracle/v1}"
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CACHE="${GO_ORACLE_CACHE:-$REPO/target/go-oracle-v1}"
SRC="$CACHE/src"
BIN="$CACHE/ctx-go"
LOCK="$CACHE/.build.lock"

# Already built → reuse (the oracle is frozen; never rebuild).
if [ -x "$BIN" ]; then echo "$BIN"; exit 0; fi

mkdir -p "$CACHE"
# Coarse lock so parallel test binaries don't race the worktree/build.
exec 9>"$LOCK"
flock 9 2>/dev/null || true
if [ -x "$BIN" ]; then echo "$BIN"; exit 0; fi   # built while we waited

# Materialize the frozen tag's source (detached worktree; idempotent).
if [ ! -e "$SRC/go.mod" ]; then
  git -C "$REPO" worktree add --force --detach "$SRC" "$ORACLE_TAG" >&2
fi

# Build the oracle from the FROZEN source (not the tree).
( cd "$SRC" && go build -o "$BIN" ./cmd/ctx ) >&2

# The worktree is only needed for the build; drop it but keep the binary.
git -C "$REPO" worktree remove --force "$SRC" >&2 2>/dev/null || rm -rf "$SRC"

[ -x "$BIN" ] || { echo "build-go-oracle: failed to produce $BIN" >&2; exit 1; }
echo "$BIN"
