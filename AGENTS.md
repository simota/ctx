# Repository Guidelines

## Project Structure & Module Organization

`ctx` is a Rust CLI workspace organized under `crates/`.
The main binary lives in `crates/ctx-cli`; supporting libraries include
`ctx-web`, `ctx-mcp`, `ctx-symbols`, `ctx-tui`, and `ctx-pack`. Each crate keeps
implementation in `src/`, integration tests in `tests/`, and benchmarks in
`benches/` when present.
The Svelte/Vite frontend is in `web/`; generated web assets embedded by Rust
are under `crates/ctx-web/dist/`. Documentation and static site files are in
`docs/`, CI helpers in `ci/`, and loop automation scripts in `loops/`.

## Build, Test, and Development Commands

- `make build`: build the debug `ctx` binary and copy it to `./bin/ctx`.
- `make release`: build an optimized release binary.
- `make run ARGS="pack ."`: build and run the CLI with forwarded arguments.
- `make browse`: start the native Axum web UI via `ctx browse`.
- `make check`: run a fast Rust type check.
- `make test`: run the Rust test suites, including the Go parity oracle gate.
- `make lint`: run `cargo clippy` on all CLI targets.
- `make fmt`: format all Rust crates with `rustfmt`.
- `make web`: rebuild the frontend into Rust-embedded assets; requires `pnpm`.
- `make dev`: run the Vite frontend dev server only.

## Coding Style & Naming Conventions

Use standard Rust formatting (`cargo fmt`) and keep Clippy warnings actionable.
Prefer small modules, explicit error handling, and non-sensitive diagnostics.
Rust crate names use `ctx-*`; Rust modules, functions, and variables use
`snake_case`; types and traits use `PascalCase`. Frontend code should follow the
existing Svelte + TypeScript style in `web/` and use `pnpm` scripts.

## Testing Guidelines

Place integration tests in each crate's `tests/` directory and unit
tests beside the code they cover. Name tests for behavior, for example
`packs_diff_with_budget_limit`. Run `make test` before broad changes; for a
narrow crate, use `cargo test --manifest-path crates/<crate>/Cargo.toml`.
Update or add focused tests when changing CLI behavior, parsing, packing,
symbol extraction, MCP behavior, or web embedding.

## Commit & Pull Request Guidelines

History uses Conventional Commit-style subjects such as `fix(web): ...`,
`chore: ...`, and `docs(adr-0005): ...`. Keep PRs focused,
describe the change, list verification commands, link issues or ADRs when
relevant, and include screenshots for web UI changes.

## Security & Configuration Tips

Start from `ctx.toml.example` for local configuration. Do not commit secrets,
tokens, private keys, local state files, or generated artifacts from `target/`
or crate-local `target/`. Avoid logging repository contents or user data unless
required for explicit diagnostics.

