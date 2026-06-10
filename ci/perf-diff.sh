#!/usr/bin/env bash
#
# ci/perf-diff.sh
#
# Phase 3 perf-regression helper invoked by .github/workflows/perf-regression.yml.
#
# Inputs:
#   $1  baseline directory (artifact downloaded from latest push-to-main run)
#   $2  current directory  (the bench output the PR just produced)
#   $3  output markdown path (the verdict table written into the PR comment)
#
# Behaviour:
#   1. Parse criterion text + go-test -bench output from both directories.
#   2. For each (engine, package, benchmark) found in BOTH sets, compute the
#      relative delta (current / baseline).
#   3. If the current run is missing the baseline (first execution after
#      merge), emit a baseline-only table and exit 0.
#   4. Otherwise emit a comparison table and a PASS/FAIL verdict.
#      Thresholds:
#        - Rust regression > 10% → FAIL  (touch perf-out/VERDICT_FAIL).
#        - Go   regression >  5% → FAIL  (touch perf-out/VERDICT_FAIL).
#   5. Always exit 0 — the GitHub workflow step that follows reads the
#      VERDICT_FAIL marker file and fails the job there. Doing it that way
#      keeps the comment posting step from being skipped.

set -euo pipefail

baseline_dir="${1:?baseline dir required}"
current_dir="${2:?current dir required}"
out_md="${3:?output markdown required}"

if [[ ! -d "${current_dir}" ]]; then
  echo "current dir not found: ${current_dir}" >&2
  exit 1
fi

mkdir -p "${current_dir}"

# Parse a criterion text output file. Each criterion line we care about
# has the shape:
#   <group>/<bench>  time:   [<low> <mid> <high>]
# We emit "<group>::<bench>\t<mid_ns>" lines on stdout.
parse_criterion() {
  local file="$1"
  awk '
    /time:[[:space:]]+\[/ {
      # Look up to 4 lines back for the "Benchmarking <name>:" or the
      # "<name>  time: ..." form. criterion prints both styles.
      line = $0
      # Strip ANSI colour codes.
      gsub(/\x1b\[[0-9;]*[mGKHF]/, "", line)
      # Format: "<name>  time:   [low mid high]"
      split(line, parts, /time:/)
      name = parts[1]
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", name)
      if (name == "") next
      tail = parts[2]
      gsub(/[\[\]]/, "", tail)
      n = split(tail, vals, /[[:space:]]+/)
      # Find the middle "<num> <unit>" pair (positions 3 & 4 after split).
      val = ""; unit = ""
      for (i = 1; i <= n - 1; i++) {
        if (vals[i] ~ /^[0-9.]+$/ && vals[i+1] ~ /^[a-zµ]+$/) {
          mid_count++
          if (mid_count == 2) { val = vals[i]; unit = vals[i+1]; break }
        }
      }
      mid_count = 0
      if (val == "" || unit == "") next
      ns = val + 0
      if (unit == "ms")      ns = ns * 1e6
      else if (unit == "µs" || unit == "us") ns = ns * 1e3
      else if (unit == "s")  ns = ns * 1e9
      printf "%s\t%.2f\n", name, ns
    }
  ' "${file}"
}

# Parse a go test -bench output file. Lines we care about:
#   BenchmarkX/sub-10    1234   567 ns/op
# Emits "<name>\t<ns_per_op>".
parse_go_bench() {
  local file="$1"
  awk '
    /^Benchmark/ {
      name = $1
      for (i = 1; i <= NF; i++) {
        if ($(i+1) == "ns/op") {
          printf "%s\t%s\n", name, $i
          break
        }
      }
    }
  ' "${file}"
}

dump_all() {
  local dir="$1"
  local out="$2"
  : > "${out}"
  if [[ -d "${dir}/rust" ]]; then
    for f in "${dir}/rust"/*.txt; do
      [[ -f "$f" ]] || continue
      local crate
      crate="$(basename "${f}" .txt)"
      while IFS=$'\t' read -r name ns; do
        echo -e "rust\t${crate}\t${name}\t${ns}" >> "${out}"
      done < <(parse_criterion "${f}")
    done
  fi
  if [[ -d "${dir}/go" ]]; then
    for f in "${dir}/go"/*.txt; do
      [[ -f "$f" ]] || continue
      local pkg
      pkg="$(basename "${f}" .txt)"
      while IFS=$'\t' read -r name ns; do
        echo -e "go\t${pkg}\t${name}\t${ns}" >> "${out}"
      done < <(parse_go_bench "${f}")
    done
  fi
}

curr_flat=$(mktemp)
base_flat=$(mktemp)
dump_all "${current_dir}" "${curr_flat}"

if [[ ! -d "${baseline_dir}" ]] || [[ -z "$(ls -A "${baseline_dir}" 2>/dev/null)" ]]; then
  cat > "${out_md}" <<EOF
### perf-regression: BASELINE ONLY

No main-branch baseline artifact found. This PR's run will be used as
the baseline once it merges. Subsequent PRs will gate against it.

#### Current bench summary
$(printf '\n%s\n' "$(cat "${curr_flat}" | column -t -s $'\t' | head -200)")
EOF
  rm -f "${curr_flat}" "${base_flat}"
  exit 0
fi

dump_all "${baseline_dir}" "${base_flat}"

# Join the two on (engine, package, name) and compute delta = curr/base.
join_tmp=$(mktemp)
awk -F'\t' -v base="${base_flat}" '
  BEGIN {
    while ((getline line < base) > 0) {
      n = split(line, a, "\t")
      if (n != 4) continue
      key = a[1] SUBSEP a[2] SUBSEP a[3]
      baseline[key] = a[4]
    }
  }
  {
    key = $1 SUBSEP $2 SUBSEP $3
    if (key in baseline) {
      b = baseline[key] + 0
      c = $4 + 0
      if (b > 0) {
        ratio = c / b
        delta_pct = (ratio - 1.0) * 100.0
        printf "%s\t%s\t%s\t%.2f\t%.2f\t%+.1f%%\n", $1, $2, $3, b, c, delta_pct
      }
    }
  }
' "${curr_flat}" > "${join_tmp}"

# Walk the joined rows and determine PASS/FAIL.
fail=0
{
  echo "### perf-regression results"
  echo
  echo "Threshold: Rust regression >10% or Go regression >5% → FAIL."
  echo
  echo "| Engine | Pkg/Crate | Benchmark | Baseline (ns) | Current (ns) | Δ |"
  echo "|---|---|---|---:|---:|---:|"
  while IFS=$'\t' read -r engine pkg name base_ns curr_ns delta; do
    echo "| ${engine} | ${pkg} | ${name} | ${base_ns} | ${curr_ns} | ${delta} |"
    # Strip non-numeric to test threshold.
    pct_raw="${delta%\%}"
    # Cross-platform safe float comparison via awk.
    is_fail=$(awk -v engine="${engine}" -v pct="${pct_raw}" 'BEGIN {
      pct += 0
      if (engine == "rust" && pct > 10.0) print 1; else
      if (engine == "go"   && pct >  5.0) print 1; else print 0
    }')
    if [[ "${is_fail}" == "1" ]]; then
      fail=1
    fi
  done < "${join_tmp}"
  echo
  if [[ ${fail} -eq 1 ]]; then
    echo "**Verdict: FAIL** — at least one benchmark regressed beyond the gate."
  else
    echo "**Verdict: PASS** — no benchmark regressed beyond the gate."
  fi
} > "${out_md}"

if [[ ${fail} -eq 1 ]]; then
  : > "${current_dir}/VERDICT_FAIL"
fi

rm -f "${curr_flat}" "${base_flat}" "${join_tmp}"
exit 0
