// crates/ctx-replay/build.rs
//
// Phase 3 FFI shim: emit a C99 header (`include/ctx_replay.h`) describing
// the `pub extern "C"` surface in `src/ffi.rs` so downstream cgo
// integrations get a stable, version-controlled handshake.
//
// Mirrors the pioneer's pattern (crates/ctx-contract/build.rs,
// ctx-scan/build.rs, ctx-relations/build.rs).

use std::env;
use std::path::PathBuf;

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let crate_path = PathBuf::from(&crate_dir);

    println!("cargo:rerun-if-changed=src/ffi.rs");
    println!("cargo:rerun-if-changed=cbindgen.toml");
    println!("cargo:rerun-if-changed=build.rs");

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");
    let out_path = PathBuf::from(out_dir).join("ctx_replay.h");

    let config = cbindgen::Config::from_file(crate_path.join("cbindgen.toml"))
        .unwrap_or_else(|e| {
            println!("cargo:warning=cbindgen.toml parse failed: {e}");
            cbindgen::Config::default()
        });

    let builder = cbindgen::Builder::new()
        .with_crate(&crate_dir)
        .with_config(config);

    match builder.generate() {
        Ok(bindings) => {
            bindings.write_to_file(&out_path);

            let stable_dir = crate_path.join("include");
            if let Err(e) = std::fs::create_dir_all(&stable_dir) {
                println!("cargo:warning=could not create include dir: {e}");
                return;
            }
            let stable_path = stable_dir.join("ctx_replay.h");
            bindings.write_to_file(&stable_path);
        }
        Err(e) => {
            println!("cargo:warning=cbindgen generation failed: {e}");
        }
    }
}
