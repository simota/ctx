# ctx-web

Rust port of `internal/web/` — the axum HTTP server behind `ctx browse`
(ADR-0005 Wave 2 foundation). Mirrors the Go mux: `/api/*` + `/raw/*`
handlers, then an embedded SPA catch-all.

## Module layout (mirrors the Go package)

| File | Go analogue |
|------|-------------|
| `src/safepath.rs` | `safepath.go` `Resolve` (root-jail path resolution) |
| `src/response.rs` | `handlers.go` `writeJSON` / `writeError` / `writeBadPath` |
| `src/embed.rs` | `embed.go` `DistHandler` (rust-embed of `internal/web/dist` + SPA fallback) |
| `src/router.rs` | `routes.go` `NewMuxWithBind` (route table) |
| `src/handlers/*` | one module per `/api/*` (and `/raw/*`) route |
| `src/lib.rs` | `server.go` `Server` (`New`/`Listen`/`Addr`/`Start`) |

## Running headlessly

Via the library:

```rust
let mut s = ctx_web::Server::new("/path/to/project", "127.0.0.1:0", false);
s.listen().await?;            // binds; addr() now readable
let addr = s.addr().unwrap(); // ephemeral port
s.serve(async { /* shutdown signal */ }).await?;
```

Via the CLI (strangler selector, matching the per-command `--*-engine`
convention):

```
ctx browse <path> --no-open --port <N> --bind 127.0.0.1 --web-engine rust
```

`--web-engine rust` (or env `CTX_WEB_ENGINE=rust`) runs this native server in
the `ctx` binary; any other value delegates to the Go server (current
default). The native server prints the same
`ctx browse: serving <path> at http://<addr>/` line the Go server emits, so
the browser-launcher parent parses the URL unchanged.

## Ported routes

- Core browse APIs: `/api/file`, `/api/tree`, `/api/dir`, `/api/where`,
  `/api/relations`, `/api/symbols`, `/api/definition`, `/api/budget`,
  `/api/tests`, `/api/roots`, `/api/evidence`, and `/api/evidence/verify`.
- Git and replay APIs: `/api/git/diff`, `/api/git/log`,
  `/api/git/co-change`, `/api/git/branches`, `/api/git/tags`,
  `/api/git/worktrees`, `/api/git/file-log`, `/api/git/commit-files`,
  `/api/git/commit-diff`, `/api/replay/list`, `/api/replay/show`,
  `/api/replay/diff`, and `/api/replay/verify`.
- Mix read APIs: `GET /api/mix` and `GET /api/mix/<id>`.
- Static serving: `GET|HEAD /raw/<path>` and SPA static fallback.

## Adding a route

1. Add a module in `src/handlers/` exposing an axum handler `fn`.
2. Add one `.route("/api/<name>", get(handlers::<name>::handle))` line in
   `src/router.rs::build`.
3. Add one `Case { … }` to `tests/parity.rs::cases()`.

## Parity harness (`tests/parity.rs`)

Starts BOTH the Go oracle (`browse … --port 0`) and this Rust server on
ephemeral loopback ports against the same fixture dir, polls each port for
readiness (10 s timeout, 50 ms interval; the Go server's stdout URL line is
parsed for its port), then issues identical HTTP requests and asserts
byte-equality of **status + body + a fixed allow-list of headers**
(`Content-Type`, the `/raw/` security headers, `Cache-Control`, `Allow`).

The Go binary is located via `$CTX_GO_BIN`, else `/tmp/ctx-go`, else built
on demand with `go build`. If Go is unavailable the test SKIPs with a notice.

### Parity carve-outs (per ADR-0005)

- **`not_found` messages** embed the resolved absolute path + an OS errno
  string (machine-specific, `/private` symlink on macOS). Those cases use
  `Norm::AbsPath`, which rewrites the volatile path to `<ROOT>` on both sides;
  status/code/shape still match byte-for-byte.
- **`Date`** and transport framing (`Content-Length`/`Connection`) are
  excluded — both stacks set them correctly but format independently.
- **Raw floats**: neither ported route emits unrounded floats, so the
  ADR-0005 numeric tolerance is not needed yet. When a float-bearing route is
  added, give its case a `Norm::FloatTol(eps)` variant (≤ 1e-12, per the
  `echo` BM25 lesson — Go `math.Log` vs Rust `f64::ln` can differ by ≤ 1 ULP).

## Deferred / known compatibility gaps

- `/api/mix` mutations (`POST /api/mix`, `DELETE /api/mix/<id>`) still return
  a deliberate Rust 405 sentinel; Go performs create/delete. This blocks full
  cutover until deterministic write fixtures, id injection, and clock injection
  exist.
- `/api/file` is ported but not currently byte-identical: Rust emits optional
  fs metadata fields used by the Svelte file detail view, while the Go oracle
  omits them.
- `/api/tests?profile=...` coverage-profile byte parity lacks a deterministic
  coverprofile fixture.
- The audit sink (`Server.WithAuditSink` / `auditMiddleware`).
- Full `.gitignore` / `.ctxignore` fidelity is not yet shared by every Rust
  walker. See `DEFERRED_ROUTES.md` for the route-by-route details.
- Full gitignore-glob fidelity in the `/raw/` secret-deny matcher (current
  matcher is basename/suffix/dir-anchored, faithful for the static
  `SecretDenyPatterns` list).
