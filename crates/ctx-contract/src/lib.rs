// crates/ctx-contract/src/lib.rs
//
// Rust port of internal/contract/. The module layout mirrors the Go
// package's file split so byte-parity verification stays trivially
// auditable side-by-side.

pub mod builder;
pub mod embed;
pub mod ffi;
pub mod format;
pub mod hash;
pub mod parse_refs;
pub mod types;
pub mod verify;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

/// SchemaVersion is the current contract manifest schema.
///
/// Mirrors `internal/contract/contract.go`'s `SchemaVersion = 1`.
pub const SCHEMA_VERSION: i32 = 1;

// Re-export the most-commonly-used types so callers can `use
// ctx_contract::Contract` without spelling out the submodule path.
pub use crate::types::{
    Contract, File, FileInput, Reference, ReferenceKind, Result as VerifyResult, StaleFile,
    VerifyOptions, Violation, ViolationKind, OK,
};
