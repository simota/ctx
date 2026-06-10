// crates/ctx-pack/src/testing.rs
//
// Helpers shared between parity tests and benches. Only compiled when
// the `testing` feature is on (or under cfg(test)).

use crate::types::{FileInput, MetadataInput, SymbolInput};

pub fn make_file(path: &str, role: &str, syms: &[(&str, &str)]) -> FileInput {
    FileInput {
        path: path.into(),
        abs_path: String::new(),
        is_dir: false,
        tokens: 100,
        role: role.into(),
        metadata: MetadataInput {
            size: 100,
            tokens_est: 100,
            role: role.into(),
            symbols: syms
                .iter()
                .map(|(n, k)| SymbolInput {
                    name: (*n).into(),
                    kind: (*k).into(),
                    line: 1,
                })
                .collect(),
        },
        content_head: Vec::new(),
    }
}

/// Generate a synthetic corpus shaped like the internal/pack package
/// itself — useful for benches and parity tests. Returns N files.
pub fn synth_corpus(n: usize) -> Vec<FileInput> {
    let mut out = Vec::with_capacity(n);
    let dirs = ["src/auth", "internal/pack", "internal/scan", "cmd/ctx", "docs", "test"];
    let bases = ["login", "session", "config", "render", "diff", "preset", "main", "readme"];
    let roles = ["core", "entry", "route", "test", "doc", "config", "unknown"];
    for i in 0..n {
        let dir = dirs[i % dirs.len()];
        let base = bases[(i / dirs.len()) % bases.len()];
        let role = roles[i % roles.len()];
        let path = format!("{dir}/{base}_{i}.go");
        let syms: Vec<(&str, &str)> = vec![("HandleLogin", "function"), ("ValidateSession", "function")];
        out.push(make_file(&path, role, &syms));
    }
    out
}
