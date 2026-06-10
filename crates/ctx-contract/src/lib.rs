// crates/ctx-contract/src/lib.rs
//
// Rust port of internal/contract/. The module layout mirrors the Go
// package's file split so byte-parity verification stays trivially
// auditable side-by-side.

pub mod types;
pub mod hash;
pub mod builder;
pub mod parse_refs;
pub mod embed;
pub mod verify;
pub mod format;
pub mod ffi;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

/// SchemaVersion is the current contract manifest schema.
///
/// Mirrors `internal/contract/contract.go`'s `SchemaVersion = 1`.
pub const SCHEMA_VERSION: i32 = 1;

// Re-export the most-commonly-used types so callers can `use
// ctx_contract::Contract` without spelling out the submodule path.
pub use crate::types::{
    Contract, File, FileInput, ReferenceKind, Reference, Result as VerifyResult, StaleFile,
    Violation, ViolationKind, VerifyOptions, OK,
};
