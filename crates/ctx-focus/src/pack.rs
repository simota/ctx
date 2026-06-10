// crates/ctx-focus/src/pack.rs
//
// Pack orchestrator: ResolveAnchor + Expand → PackResult. This is the
// one-shot stateless entry point. For repeated lookups against the same
// corpus, prefer the sticky-handle session API (see ffi.rs).

use crate::expand::expand;
use crate::resolve::resolve_anchor;
use crate::types::{ErrAmbiguous, ExpandOptions, FileInput, PackResult};

pub fn pack(
    files: &[FileInput],
    raw: &str,
    opts: &ExpandOptions,
) -> Result<PackResult, ErrAmbiguous> {
    let anchor = resolve_anchor(files, raw)?;
    let files_out = expand(files, &anchor, opts);
    Ok(PackResult {
        anchor,
        files: files_out,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SymbolInfo;

    fn mkfile(path: &str, lines: Vec<&str>, syms: Vec<(&str, i64)>) -> FileInput {
        FileInput {
            path: path.into(),
            is_dir: false,
            symbols: syms
                .into_iter()
                .map(|(n, l)| SymbolInfo {
                    name: n.into(),
                    kind: "function".into(),
                    line: l,
                })
                .collect(),
            lines: lines.into_iter().map(String::from).collect(),
        }
    }

    #[test]
    fn pack_resolves_and_expands() {
        let files = vec![
            mkfile(
                "internal/pack/pack.go",
                vec!["package pack", "func Pack() {}"],
                vec![("Pack", 2)],
            ),
            mkfile(
                "internal/pack/helper.go",
                vec!["package pack"],
                vec![("helper", 2)],
            ),
        ];
        let r = pack(&files, "Pack", &ExpandOptions { hops: 1 }).unwrap();
        assert_eq!(r.anchor.origin_path, "internal/pack/pack.go");
        assert!(r.files.iter().any(|f| f.path == "internal/pack/helper.go"));
    }
}
