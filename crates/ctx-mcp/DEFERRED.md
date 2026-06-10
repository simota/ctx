# ctx-mcp parity — deferred / normalized fields

This file documents every place where the Go MCP server (`internal/mcp`) emits
something that is **not byte-parity-able** over stdio against the Rust port, and
how `tests/parity.rs` handles it. The default is `Norm::Exact` (byte-for-byte);
anything listed here is the explicit exception, with the reason.

## Wire framing (NOT deferred — replicated exactly)

The Go server frames JSON-RPC as **NDJSON**: `bufio.Scanner` reads one request
per line; `json.Encoder.Encode` writes one response per line (it appends a
single `\n`). There is **no** LSP `Content-Length` framing. The harness sends
each request object + `\n` and reads exactly one `\n`-terminated line per
expected response. This is fully parity-able and is compared byte-exact.

## Normalized fields (compared after a documented transform)

### 1. Absolute fixture path — `Norm::AbsPath`
- **Where:** `ctx_skim` header (`path=/abs/.../main.go`), and the
  path-sandbox error hint (`server root is "/abs/..."`,
  `internal/mcp/server.go:pathHint`).
- **Why deferred:** the resolved root is a machine-specific absolute path (plus
  the macOS `/private` symlink variant). It is not a protocol field — only the
  message *shape* is meaningful.
- **Transform:** replace the canonicalized fixture root (and its `/private`
  prefix form) with `<ROOT>` on **both** sides before comparison. The
  surrounding bytes still compare exactly.

### 2. Pack `**Generated**` timestamp — `Norm::Timestamp`
- **Where:** `ctx_pack` markdown/plain header line
  `**Generated**: <RFC3339 wall-clock>`.
- **Why deferred:** wall-clock; differs every run.
- **Transform:** replace the value after `**Generated**: ` up to the next
  (escaped) newline with `<TS>` on both sides. Everything else in the pack —
  budget line, included/skipped lists, file bodies — is compared exact.

### 3. `serverInfo.version` (`initialize`) — `Norm::Version`
- **Where:** `initialize` result, `serverInfo.version`.
- **Why deferred:** Go's `serverVersion()` (`internal/mcp/server.go:519`)
  returns the **12-char `vcs.revision` commit hash** (or `info.Main.Version`)
  under `go build`. The harness builds the oracle via `go build`, so Go emits a
  real hash that changes every commit — byte-unreproducible by Rust. WITHOUT a
  Norm this field is **unsatisfiable**: the loop could fix structure/ordering
  but never byte-match the hash, so the case would churn forever.
- **Transform:** `Norm::Version` replaces ONLY the `serverInfo.version` string
  VALUE with `<VERSION>` on **both** sides. The substitution is anchored on the
  literal key token `"version":"` (quote + lowercase `version` + `":"`), which
  cannot match inside `"protocolVersion":"…"` (there `version` is preceded by
  the letters `protocol`, not a quote). So **`protocolVersion` stays byte-exact**
  — the deterministic constant `"2024-11-05"` is never clobbered.
- **What remains RED (and SATISFIABLE):** after this Norm, the only remaining
  `initialize` diff is **key ordering**. Go (`encoding/json`) sorts map keys;
  the Rust draft preserves insertion order. The two normalized bodies are:
  - Go:   `…"result":{"capabilities":{"prompts":{},"resources":{},"tools":{}},"protocolVersion":"2024-11-05","serverInfo":{"name":"ctx","version":"<VERSION>"}}}`
  - Rust: `…"result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{},"resources":{},"prompts":{}},"serverInfo":{"name":"ctx","version":"<VERSION>"}}}`
  The loop fixes this by emitting alphabetically-sorted keys (outer:
  `capabilities, protocolVersion, serverInfo`; inner: `prompts, resources,
  tools`). Once sorted + correct shape + ANY version → the case goes GREEN. This
  is a REAL defect, deliberately left RED, but RED-for-satisfiable-reasons only.

## Pinned / structurally-deterministic (NOT deferred — corpus-rescan verdict)

### 4. `ctx_roots_list` registry contents — pinned via `CTX_ROOTS_FILE`
- Reads `~/.ctx/roots.toml`: per-machine paths + `LastOpenedAt` wall-clock + a
  `*` marker on the currently-served root.
- **Handling — pinned, NOT deferred:** both servers are launched with
  `CTX_ROOTS_FILE=tests/roots-registry/roots.toml` (a fixture with no
  `last_opened` → `-`, and absolute `/tmp/ctx-mcp-parity-*` paths that never
  equal the served fixture root → no `*`). Go reads this var at
  `internal/config/roots.go:29`; the Rust ecosystem already honors the SAME var
  (`crates/ctx-cli/src/main.rs:2860`, `crates/ctx-web/src/handlers/roots.rs:142`),
  so the ported `ctx_roots_list` tool will read the identical file. With the
  registry pinned the output is a fixed literal and is compared **byte-exact**.
  Currently RED only because the tool is unported in the draft.

### 4b. `ctx_tree` / `ctx_budget` git-status — structurally empty
- Go enriches `ctx_tree` entries with per-file `git_status` and walks the tree
  for `ctx_budget`. The served `--root` is the fixtures **subdir**, not a repo
  root. `git status --porcelain` keys are repo-root-relative
  (`crates/ctx-mcp/tests/fixtures/main.go`) while the walk emits subdir-relative
  paths (`main.go`), so `applyGitStatus` never matches → `git_status` is ALWAYS
  empty (and `omitempty` drops the field) regardless of whether the fixtures are
  untracked / staged / committed / dirty. Verified across all four states.
  Tokens come from tiktoken over fixed file bytes → content-deterministic and
  stable across runs. **No Norm needed; compared byte-exact.**

### 5. `ctx_digest` on a non-git fixture
- `tests/fixtures/` is intentionally NOT a git repo, so Go's `ctx_digest`
  returns the deterministic tool-error envelope
  `{"content":[{"type":"text","text":"repository does not exist"}],"isError":true}`.
- This is stable and parity-able as-is; it is included so the digest dispatch
  path is exercised. A future fixture with a pinned, fixed-history git repo
  would be needed to byte-compare a *successful* digest (commit hashes, author
  names, and churn dates are otherwise non-deterministic) — that richer case is
  **deferred** until a deterministic git fixture exists.
  **TODO(agent):** add a committed bare-repo fixture with frozen author/date
  env (`GIT_AUTHOR_DATE`/`GIT_COMMITTER_DATE`) to cover a green digest body.

### 6. Audit / transcript side channels
- `--log-file`, `AuditLogPath`, and the stderr `mcp ts=… call=…` debug line are
  out of scope: they do not appear on the JSON-RPC stdout wire. The harness
  pipes child stderr to `/dev/null` and never asserts on it.

## Corpus re-scan verdict (all 22 cases)

Every case was audited for build-, time-, host-, commit-, or git-dependent
fields that would be byte-unsatisfiable without a Norm. Result: **all 22 cases
are satisfiable-in-principle.**

| Case | Volatile field? | Handling |
|------|-----------------|----------|
| `initialize` | serverInfo.version (commit hash) | `Norm::Version`; key-order RED but satisfiable |
| `tools_list` | none | Exact |
| `prompts_list` / `prompts_get_find_code` | none (static template render) | Exact |
| `resources_list` / `resources_templates_list` | none | Exact |
| `resources_read_file` | none (relative uri + fixed bytes) | Exact |
| `tool_ctx_where` | none (relative paths, fixed scoring) | Exact (PASS) |
| `tool_ctx_symbols` | none (extraction over fixed bytes) | Exact (PASS) |
| `tool_ctx_pack` | `**Generated**` wall-clock | `Norm::Timestamp` |
| `tool_ctx_budget` | git-status none (4b); tokens deterministic | Exact |
| `tool_ctx_skim` | abs path in header | `Norm::AbsPath` |
| `tool_ctx_tree` | git-status structurally empty (4b); tokens deterministic | Exact |
| `tool_ctx_focus` | none (origin is relative `main.go`) | Exact |
| `tool_ctx_digest` | deterministic non-repo error envelope | Exact |
| `tool_ctx_roots_list` | pinned via `CTX_ROOTS_FILE` (§4) | Exact |
| `err_unknown_method` | none | Exact (PASS) |
| `err_parse_error` | parser message text differs (see below) | Exact |
| `err_path_outside_root` | abs root in hint | `Norm::AbsPath` |
| `err_budget_too_large` | none | Exact |
| `err_focus_missing_anchor` | none | Exact |
| `err_unknown_tool` | none (envelope vs JSON-RPC error) | Exact |

**One satisfiability caveat — `err_parse_error`:** Go's `encoding/json` and
Rust's `serde_json` emit DIFFERENT parser-error message strings for the same
malformed input (Go: `invalid character 'n' looking for beginning of object key
string`; serde: `key must be a string at line 1 column 2`). Both are stable
(not time/host/commit dependent), so the case is NOT unsatisfiable in the
churn-forever sense — but achieving byte-parity requires the Rust port to
**hand-craft the JSON-RPC parse-error message to match Go's stdlib wording**
rather than forwarding serde's text. That is a real porting task (the `-32700`
code + null id are already parity-able). If the loop decides matching Go's
stdlib prose verbatim is not worth it, this single field should be moved to a
`Norm::ParseErrMsg` carve-out (normalize the `message` value to `<PARSE_ERR>`
on both sides while keeping code/id/shape byte-exact). Flagged here so the
decision is explicit rather than silently churning. **TODO(agent):** decide
match-Go-wording vs add `Norm::ParseErrMsg` when porting the parse-error path.
