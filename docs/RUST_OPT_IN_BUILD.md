# Rust build guide

`ctx` is a pure-Rust project. The former mixed-language bridge and
`rust_contract` opt-in build mode have been removed from the published
setup path.

This document describes the current build path. Historical FFI details
are not setup instructions.

## Requirements

- Rust toolchain with stable `rustc` and `cargo`.
- Node.js / pnpm only when rebuilding the Svelte frontend in `web/`.

The normal CLI build uses the vendored web assets in
`crates/ctx-web/dist/`, so Node.js is not required for everyday Rust
builds.

## Build

```bash
make build
./bin/ctx --help
```

For an optimized binary:

```bash
make release
./bin/ctx --help
```

To run without installing:

```bash
make run ARGS="pack ."
```

## Install

```bash
make install
ctx --help
```

`make install` runs:

```bash
cargo install --path crates/ctx-cli --locked --force
```

For a prefix install:

```bash
make install-prefix PREFIX="$HOME/.local"
```

## Web Assets

Rebuild the Svelte/Vite frontend only when files under `web/` change:

```bash
make web
```

This updates the Rust-embedded assets under `crates/ctx-web/dist/`.

For frontend-only development:

```bash
make dev
```

## Verification

Fast type check:

```bash
make check
```

Focused Rust test suites:

```bash
cargo test --manifest-path crates/ctx-cli/Cargo.toml
cargo test --manifest-path crates/ctx-web/Cargo.toml
cargo test --manifest-path crates/ctx-mcp/Cargo.toml
cargo test --manifest-path crates/ctx-symbols/Cargo.toml --features testing
cargo test --manifest-path crates/ctx-tui/Cargo.toml
```

Full project test command:

```bash
make test
```

## Removed Legacy Path

The removed mixed-language build path is intentionally not documented
here. Current docs and release instructions should use the Rust commands
above.
