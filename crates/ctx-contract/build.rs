// crates/ctx-contract/build.rs
//
// T-26 FFI shim: emit a C99 header (`include/ctx_contract.h`) describing
// the `pub extern "C"` surface in `src/ffi.rs` so downstream cgo
// integrations get a stable, version-controlled handshake.
//
// The header is also written to OUT_DIR for sandbox-friendly builds
// (e.g. read-only source trees in CI). The repo-local copy under
// `crates/ctx-contract/include/` is the canonical artifact consumed by
// the Go side and gets refreshed on every successful build.

use std::env;
use std::path::PathBuf;

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let crate_path = PathBuf::from(&crate_dir);

    // Rebuild when the FFI surface or the cbindgen config moves.
    println!("cargo:rerun-if-changed=src/ffi.rs");
    println!("cargo:rerun-if-changed=cbindgen.toml");
    println!("cargo:rerun-if-changed=build.rs");

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");
    let out_path = PathBuf::from(out_dir).join("ctx_contract.h");

    let config =
        cbindgen::Config::from_file(crate_path.join("cbindgen.toml")).unwrap_or_else(|e| {
            println!("cargo:warning=cbindgen.toml parse failed: {e}");
            cbindgen::Config::default()
        });

    let builder = cbindgen::Builder::new()
        .with_crate(&crate_dir)
        .with_config(config);

    match builder.generate() {
        Ok(bindings) => {
            // Cargo-internal copy (OUT_DIR).
            bindings.write_to_file(&out_path);

            // Repo-stable copy for downstream cgo. Best-effort: missing
            // directory is created; permission errors only warn so the
            // crate still builds in read-only sandboxes.
            let stable_dir = crate_path.join("include");
            if let Err(e) = std::fs::create_dir_all(&stable_dir) {
                println!("cargo:warning=could not create include dir: {e}");
                return;
            }
            let stable_path = stable_dir.join("ctx_contract.h");
            bindings.write_to_file(&stable_path);
        }
        Err(e) => {
            // Don't fail the build on bindgen errors — emit a warning and
            // let the rlib/staticlib/cdylib still get produced. Header
            // freshness is verified in CI separately.
            println!("cargo:warning=cbindgen generation failed: {e}");
        }
    }
}
