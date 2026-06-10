// crates/ctx-symbols/src/testing/mod.rs
//
// Shared helpers exposed to parity / regression tests + benches.

use crate::types::{APIRange, APIRenderRequest, FileSymbols, Symbol};

pub fn make_request(lines: &[&str], ranges: Vec<(i32, i32, Option<&str>)>) -> APIRenderRequest {
    APIRenderRequest {
        lines: lines.iter().map(|s| s.to_string()).collect(),
        ranges: ranges
            .into_iter()
            .map(|(s, e, repl)| APIRange {
                start: s,
                end: e,
                end_replacement: repl.map(|x| x.to_string()),
            })
            .collect(),
    }
}

pub fn small_corpus() -> Vec<FileSymbols> {
    vec![
        FileSymbols {
            path: "a.go".to_string(),
            symbols: vec![Symbol {
                name: "Foo".to_string(),
                kind: "function".to_string(),
                line: 1,
            }],
        },
        FileSymbols {
            path: "internal/b.go".to_string(),
            symbols: vec![Symbol {
                name: "Foo".to_string(),
                kind: "method".to_string(),
                line: 5,
            }],
        },
    ]
}

/// Build a synthetic corpus of `files` files × `syms_per_file` symbols.
pub fn synthetic_corpus(files: usize, syms_per_file: usize) -> Vec<FileSymbols> {
    let mut out = Vec::with_capacity(files);
    for f in 0..files {
        let mut syms = Vec::with_capacity(syms_per_file);
        for s in 0..syms_per_file {
            let name = if s % 7 == 0 {
                "BuildIndex".to_string()
            } else {
                format!("Fn{}_{}", f, s)
            };
            syms.push(Symbol {
                name,
                kind: if s % 3 == 0 {
                    "function".to_string()
                } else {
                    "type".to_string()
                },
                line: (s as i32) + 1,
            });
        }
        out.push(FileSymbols {
            path: format!("internal/pkg{}/file{}.go", f / 10, f),
            symbols: syms,
        });
    }
    out
}
