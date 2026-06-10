# ctx-tui — deferred / known divergences

## `p` pack action writes a simplified context.md — CLOSED

**Status: CLOSED.** The pack markdown assembly was extracted out of
ctx-cli's `render_native_pack` into a reusable library API
(`crates/ctx-pack/src/assemble.rs`: `render(...)` + `pack_markdown(inputs,
goal, budget)`), and `write_pack` in `crates/ctx-tui/src/lib.rs` now calls
`ctx_pack::assemble::pack_markdown(inputs, "", model.budget)` — the Rust
equivalent of Go's `pack.Pack(f, m.includedFiles(), pack.Options{Budget,
Format: Markdown})`. `context.md` is a FULL rendered markdown pack
(metadata header + `## File contents` sections), no longer a path list.

Historical record of the original divergence:

> Go's tui `writePack()` (`internal/tui/app.go`) writes a FULL rendered
> context pack via `pack.Pack(f, m.includedFiles(), pack.Options{Budget,
> Format: Markdown})`. The native pack markdown assembly lived **inline**
> in ctx-cli's pack command — there was no reusable
> `ctx_pack::pack(writer, files, opts)` library API — so the native tui's
> `p` action wrote the included file PATHS as an interim. The divergence
> was deferred because the extraction touches the parity-critical pack
> path and the `p` action is interactive-only with no parity oracle.

Closure verification:
* `ctx pack` CLI parity suite stays green (the extraction is a verbatim
  move; ctx-cli's `render_native_pack` is now a thin wrapper over
  `ctx_pack::assemble::render`).
* `ctx-pack` unit tests cover `render` (markdown shape, contract on/off,
  unknown format) and `pack_markdown` (full pack shape, budget cut).
* `ctx-tui` unit test `write_pack_writes_full_rendered_pack_not_path_list`
  drives `write_pack_to` over a real tempdir tree.
