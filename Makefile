# ctx — pure-Rust project (Go eliminated, ADR-0005 Wave 4).
# This Makefile is a thin convenience layer over cargo. The `ctx` binary is the
# native Rust CLI (crates/ctx-cli); the web SPA is embedded at compile time by
# ctx-web from the vendored crates/ctx-web/dist, so a normal build needs no
# Node/pnpm toolchain — rebuild the frontend explicitly with `make web`.

.PHONY: all help build release install install-prefix uninstall \
        test lint fmt check web run dev browse oracle clean distclean

BINARY      := ctx
CLI_MANIFEST := crates/ctx-cli/Cargo.toml
CLI_DIR     := crates/ctx-cli
DEBUG_BIN   := $(CLI_DIR)/target/debug/$(BINARY)
RELEASE_BIN := $(CLI_DIR)/target/release/$(BINARY)

# BIN_DIR: local copy of the freshly built binary.
# PREFIX:  install root for `install-prefix` (override on the command line).
# DESTDIR: staged-install prefix for packaging.
# ARGS:    extra args forwarded to `make run`.
BIN_DIR     := bin
PREFIX      ?= $(HOME)/.local
DESTDIR     ?=
ARGS        ?=

# ---- default --------------------------------------------------------------

help:  ## Show this help
	@grep -hE '^[a-zA-Z_-]+:.*## .*$$' $(MAKEFILE_LIST) \
		| sort \
		| awk 'BEGIN {FS = ":.*## "} {printf "  \033[36m%-14s\033[0m %s\n", $$1, $$2}'

all: build  ## Alias for `build`

# ---- build ----------------------------------------------------------------

# `install` (not cp) writes a fresh inode, so it does not invalidate the
# binary's code signature — a plain `cp` over an existing Mach-O on macOS
# triggers SIGKILL on the next exec.
build:  ## Build the debug ctx binary and copy it to ./bin
	cargo build --manifest-path $(CLI_MANIFEST)
	@mkdir -p $(BIN_DIR)
	install -m 0755 $(DEBUG_BIN) $(BIN_DIR)/$(BINARY)
	@echo "built $(BIN_DIR)/$(BINARY)"

release:  ## Build the optimized release ctx binary and copy it to ./bin
	cargo build --release --manifest-path $(CLI_MANIFEST)
	@mkdir -p $(BIN_DIR)
	install -m 0755 $(RELEASE_BIN) $(BIN_DIR)/$(BINARY)
	@echo "built $(BIN_DIR)/$(BINARY) (release)"

web:  ## Rebuild the web frontend into crates/ctx-web/dist (needs pnpm)
	cd web && pnpm install --frozen-lockfile && pnpm build

# ---- install --------------------------------------------------------------

install:  ## Install ctx into ~/.cargo/bin via `cargo install` (release, locked)
	cargo install --path $(CLI_DIR) --locked --force
	@echo "installed $(BINARY) to $$(cargo --version >/dev/null 2>&1 && echo $${CARGO_HOME:-$$HOME/.cargo}/bin)/$(BINARY)"

install-prefix: release  ## Install the release binary to $(PREFIX)/bin (override PREFIX=)
	install -d $(DESTDIR)$(PREFIX)/bin
	install -m 0755 $(RELEASE_BIN) $(DESTDIR)$(PREFIX)/bin/$(BINARY)
	@echo "installed $(BINARY) to $(DESTDIR)$(PREFIX)/bin/$(BINARY)"

uninstall:  ## Remove ctx (both `cargo install` and $(PREFIX)/bin copies)
	-cargo uninstall ctx-cli
	-rm -f $(DESTDIR)$(PREFIX)/bin/$(BINARY)

# ---- quality --------------------------------------------------------------

test:  ## Run the Rust test suites incl. the parity gate vs the go-oracle/v1 tag
	CTX_GO_BIN="$$(bash ci/build-go-oracle.sh)" cargo test --manifest-path $(CLI_MANIFEST)
	cargo test --manifest-path crates/ctx-web/Cargo.toml
	cargo test --manifest-path crates/ctx-mcp/Cargo.toml
	cargo test --manifest-path crates/ctx-symbols/Cargo.toml --features testing
	cargo test --manifest-path crates/ctx-tui/Cargo.toml

lint:  ## Run clippy across the CLI and its workspace crates
	cargo clippy --manifest-path $(CLI_MANIFEST) --all-targets

fmt:  ## Format all crates with rustfmt
	cargo fmt --manifest-path $(CLI_MANIFEST) --all

check:  ## Fast type-check without producing a binary
	cargo check --manifest-path $(CLI_MANIFEST)

oracle:  ## Build the frozen Go parity oracle from the go-oracle/v1 tag (prints its path)
	@bash ci/build-go-oracle.sh

# ---- run ------------------------------------------------------------------

run: build  ## Build + run ctx (forward args via ARGS="map .")
	./$(BIN_DIR)/$(BINARY) $(ARGS)

browse: build  ## Build + start the native axum web UI (ctx browse)
	./$(BIN_DIR)/$(BINARY) browse

dev:  ## Vite dev server only (frontend hot reload; needs pnpm)
	cd web && pnpm dev

# ---- clean ----------------------------------------------------------------

clean:  ## Remove ./bin and cargo build artifacts
	rm -rf $(BIN_DIR)
	cargo clean --manifest-path $(CLI_MANIFEST) 2>/dev/null || true

distclean: clean  ## clean + drop the cached go-oracle worktree/build
	-git worktree remove --force target/go-oracle-v1/src 2>/dev/null
	rm -rf target/go-oracle-v1
