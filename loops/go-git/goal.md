# Loop goal — port the git web routes to native Rust at byte-parity

## Objective
Make ctx-web serve `/api/git/diff`, `/api/git/file-log`, `/api/git/commit-diff`
**natively in Rust at byte-for-byte HTTP/JSON parity** with the Go web server,
so these 3 routes stop delegating to Go (the last byte-parity-able Wave 2 web
work). Go internals: `internal/web/handlers.go` (handleGitDiff/handleFileLog/
handleCommitDiff), `internal/git/diff.go` + `file_log.go`, response shapes in
`internal/web/api.go`.

## Proven recipe (a feasibility spike confirmed byte-exact, 15/15)
- **Diff core**: go-git `utildiff.Do` == Rust crate **`dmp` 0.2.3** (SurrealDB)
  run as: line-encode (DiffLinesToRunes equivalent; surrogate-skip `+2048` at
  boundary `55296`) → `diff_main(enc_a, enc_b, /*checklines=*/false)` →
  `diff_chars_tolines`, with **NO** semantic/efficiency cleanup. Then port
  `renderDiffLines` (diff.go:373) to flat add/del/eq lines with old/new numbers.
- **Git reads**: `gix` crate — `repo.rev_parse_single(rev) → peel_to_commit →
  tree → lookup_entry_by_path → blob.data`; worktree side = `std::fs::read`.
- **Binary sniff**: NUL byte in the first 8000 bytes.
- **Truncation**: a maxBytes cap and a 5000-line cap set `Truncated`.
- Put shared git logic in a new `crates/ctx-git` crate reused by ctx-web handlers.

## Acceptance criteria (measurable, verify.sh-gated)
1. **AC1 — git_parity GREEN.** `crates/ctx-web/tests/git_parity.rs` (the PINNED
   13-case differential oracle: boots Go + Rust ctx-web, byte-compares each git
   route on a deterministic git fixture) passes 13/13.
2. **AC2 — no collateral regression.** cli/web(main)/symbols/mcp parity suites
   stay fully green; counts monotonic.
3. **AC3 — Go untouched.** `git diff origin/main -- 'internal/**' 'cmd/**'` empty.
4. **AC4 — go build clean.** 5. **AC5 — no placeholders** in changed Rust src.

## OUT OF SCOPE
- Any Go change. If a case is genuinely not byte-parity-able, STOP and append to
  `crates/ctx-web/GIT_DEFERRED.md` — never stub.
- Flipping the dispatcher default / removing the Go routes — that is the Wave 3
  cutover (separate, needs sign-off).

## Verification command
`bash loops/go-git/verify.sh`

NEXUS_LOOP_STATUS: READY
NEXUS_LOOP_SUMMARY: git web routes Go->Rust byte-parity loop; oracle pinned (13 cases); recipe = gix + dmp 0.2.3
