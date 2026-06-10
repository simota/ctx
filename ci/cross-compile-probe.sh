#!/usr/bin/env bash
# ci/cross-compile-probe.sh — drive cargo build across every target the
# parity pipeline needs, recording per-target success/skip/fail to
# /tmp/cross-compile-probe-results.txt without aborting the whole run on
# the first missing toolchain.
#
# Intended to be invoked from CI (or locally, as a developer smoke
# check). Exits 0 as long as the WRITE to the results file succeeds,
# regardless of per-target outcome — the parity job downstream is what
# enforces the actual matrix. That separation lets a developer on
# darwin-arm64 run this script without installing the cross-linker for
# Linux targets.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROBE_MANIFEST="${REPO_ROOT}/crates/ctx-contract-probe/Cargo.toml"
RESULTS_FILE="${CTX_PROBE_RESULTS:-/tmp/cross-compile-probe-results.txt}"

# Targets we care about. The pairing of "name" / "rust triple" keeps the
# results file readable while letting us call cargo with the canonical
# triple.
declare -a TARGETS=(
  "darwin-amd64:x86_64-apple-darwin"
  "darwin-arm64:aarch64-apple-darwin"
  "linux-amd64:x86_64-unknown-linux-gnu"
  "linux-arm64:aarch64-unknown-linux-gnu"
)

if ! command -v cargo >/dev/null 2>&1; then
  echo "cross-compile-probe: cargo not on PATH; cannot run smoke probe" >&2
  echo "skip\tall\tcargo-missing" > "${RESULTS_FILE}"
  exit 0
fi

if ! command -v rustup >/dev/null 2>&1; then
  echo "cross-compile-probe: rustup not on PATH; falling back to host target only" >&2
  installed_targets=""
else
  installed_targets="$(rustup target list --installed 2>/dev/null || true)"
fi

# Always reset the results file so a stale run from a previous CI step
# never leaks through.
: > "${RESULTS_FILE}"

started_at="$(date -u +%FT%TZ)"
echo "# cross-compile-probe run ${started_at}" >> "${RESULTS_FILE}"
echo "# host: $(uname -sm)" >> "${RESULTS_FILE}"
echo "# manifest: ${PROBE_MANIFEST}" >> "${RESULTS_FILE}"
echo "" >> "${RESULTS_FILE}"

overall_status=0

for entry in "${TARGETS[@]}"; do
  name="${entry%%:*}"
  triple="${entry##*:}"

  if [[ -n "${installed_targets}" ]] && ! grep -qx "${triple}" <<< "${installed_targets}"; then
    printf 'skip\t%s\t%s\tnot-installed\n' "${name}" "${triple}" \
      | tee -a "${RESULTS_FILE}"
    continue
  fi

  log_file="$(mktemp -t "cross-compile-probe.${name}.XXXXXX")"
  if cargo build \
      --release \
      --target "${triple}" \
      --manifest-path "${PROBE_MANIFEST}" \
      > "${log_file}" 2>&1; then
    printf 'ok\t%s\t%s\n' "${name}" "${triple}" \
      | tee -a "${RESULTS_FILE}"
  else
    overall_status=1
    last_line="$(tail -n 1 "${log_file}" | tr -d '\t')"
    printf 'fail\t%s\t%s\t%s\n' "${name}" "${triple}" "${last_line}" \
      | tee -a "${RESULTS_FILE}"
    # Tee the full log into the results file too so CI archives the
    # diagnostic without us needing to upload extra artifacts.
    {
      echo "--- begin ${triple} build log ---"
      cat "${log_file}"
      echo "--- end ${triple} build log ---"
    } >> "${RESULTS_FILE}"
  fi
  rm -f "${log_file}"
done

echo "" >> "${RESULTS_FILE}"
echo "# overall_status=${overall_status}" >> "${RESULTS_FILE}"

# We intentionally exit 0 even when overall_status is 1 — the script is a
# diagnostic, not a gate. A wrapping CI job can grep for "^fail" in the
# results file if it wants to fail the build.
exit 0
