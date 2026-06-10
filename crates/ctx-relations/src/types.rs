// crates/ctx-relations/src/types.rs
//
// Port of the subset of internal/relations and internal/model types the
// crate exposes across the FFI:
//
//   - relations.Index   (internal/relations/relations.go)
//   - Edges helper      (internal/relations/relations.go)
//
// JSON shape mirrors the Go-side golden exporter (cmd/relations-golden-export):
// snake_case keys, sorted map entries with stable ordering, no nulls.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// `Index` mirrors `relations.Index`. The Go type uses Go-style field
/// names with no JSON tags; the golden exporter normalises them to
/// snake_case so this struct's serialized form matches byte-exact.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Index {
    /// ModulePath from go.mod ("" when no go.mod at the root).
    #[serde(rename = "module_path")]
    pub module_path: String,

    /// Outgoing imports: file → sorted/deduped list of files it imports.
    /// Ordering is deterministic (BTreeMap keys) and matches the Go
    /// exporter which sorts keys before emitting.
    #[serde(rename = "imports")]
    pub imports: BTreeMap<String, Vec<String>>,

    /// Reverse index: file → sorted/deduped list of files that import it.
    #[serde(rename = "importers")]
    pub importers: BTreeMap<String, Vec<String>>,
}

impl Index {
    /// Mirrors `(*Index).Edges(file)`. Returns sorted/deduped
    /// (imports, importers) for `file`. Missing entries return empty
    /// (non-nil) vectors — matching the Go contract.
    pub fn edges(&self, file: &str) -> Edges {
        let mut imports = self.imports.get(file).cloned().unwrap_or_default();
        let mut importers = self.importers.get(file).cloned().unwrap_or_default();
        imports.sort();
        importers.sort();
        Edges { imports, importers }
    }
}

/// Sorted, deduped (imports, importers) pair returned by `Index::edges`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edges {
    pub imports: Vec<String>,
    pub importers: Vec<String>,
}
