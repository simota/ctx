// crates/ctx-scan/src/types.rs
//
// Port of the subset of internal/model + internal/scan options that the
// scan code actually consumes:
//
//   - model.Warning  (internal/model/file.go line 82)
//   - scan.Options   (internal/scan/secret.go line 17)
//
// We deliberately do NOT port internal/model.FileInfo etc — Phase 1
// scope replicates the minimal surface scan needs, per the
// Phase 1 mission charter.
//
// JSON shape:
//   Warning fields use snake_case so the Go-side golden exporter
//   (cmd/scan-golden-export) and the rustbridge dispatcher agree on
//   wire format. The Go Warning struct itself has no json tags so the
//   exporter normalises Go field names to lower_snake_case before
//   emitting goldens.

use serde::{Deserialize, Serialize};

/// `Warning` mirrors `internal/model.Warning`. Field names use
/// snake_case for the JSON wire so the parity goldens and the cgo
/// dispatcher can decode both sides identically.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Warning {
    pub path: String,
    pub line: i64,
    pub kind: String,
    pub severity: String,
    pub message: String,
    pub preview: String,
}

/// `Options` mirrors `scan.Options`. The defaults mirror Go zero values:
/// empty allowlists, entropy disabled.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Options {
    #[serde(default)]
    pub allowlist: Vec<String>,
    #[serde(default)]
    pub allowlist_files: Vec<String>,
    #[serde(default)]
    pub enable_entropy: bool,
}
