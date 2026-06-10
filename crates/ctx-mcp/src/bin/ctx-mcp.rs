//! `ctx-mcp` stdio server binary — the Rust counterpart to
//! `ctx mcp serve --root <dir> [--allow-outside-root]` (Go: internal/cli/mcp.go).
//!
//! This is a thin entrypoint: it parses the same flags the Go CLI accepts that
//! affect wire output (`--root`, `--allow-outside-root`), wires stdin/stdout to
//! [`ctx_mcp::serve`], and exits. All protocol logic lives in the library; the
//! MCP handler set is intentionally INCOMPLETE (the migration loop fills it in),
//! so this binary boots and answers `initialize` / `tools/list` / a subset of
//! `tools/call` while the differential parity oracle stays RED for everything
//! else.
//!
//! Flags that exist on the Go side but do NOT affect the JSON-RPC wire bytes
//! (`--log-file`, audit paths, config loading) are deliberately omitted here —
//! the parity harness never exercises them, and adding them would be MCP logic
//! the loop is responsible for, not oracle scaffolding.

use std::io::{self, BufReader};
use std::path::PathBuf;
use std::process::ExitCode;

use ctx_mcp::ServeOptions;

fn main() -> ExitCode {
    let mut root: Option<PathBuf> = None;
    let mut allow_outside_root = false;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--root" => match args.next() {
                Some(value) => root = Some(PathBuf::from(value)),
                None => {
                    eprintln!("ctx-mcp: --root requires a value");
                    return ExitCode::from(2);
                }
            },
            other if other.starts_with("--root=") => {
                root = Some(PathBuf::from(&other["--root=".len()..]));
            }
            "--allow-outside-root" => allow_outside_root = true,
            // Ignore stdio-transport subcommand words so invoking this binary as
            // a near drop-in for `ctx mcp serve` does not choke on a leading
            // `serve` token if a caller passes one.
            "serve" | "mcp" => {}
            unknown => {
                eprintln!("ctx-mcp: unrecognized argument {unknown:?}");
                return ExitCode::from(2);
            }
        }
    }

    let opts = match root {
        Some(root) => ServeOptions {
            root,
            allow_outside_root,
        },
        None => ServeOptions {
            allow_outside_root,
            ..ServeOptions::default()
        },
    };

    let stdin = io::stdin();
    let stdout = io::stdout();
    match ctx_mcp::serve(BufReader::new(stdin.lock()), stdout.lock(), opts) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("ctx-mcp: {err}");
            ExitCode::FAILURE
        }
    }
}
