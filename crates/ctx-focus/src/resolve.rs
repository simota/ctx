// crates/ctx-focus/src/resolve.rs
//
// Port of internal/focus.ResolveAnchor. Three-pass match: symbol exact →
// basename → repo-relative path. Multiple symbol matches yield an
// ErrAmbiguous with all candidates.

use crate::types::{Anchor, AnchorKind, Candidate, ErrAmbiguous, FileInput};

/// supportedExt mirrors the Go side: only ports + extensions the focus
/// pipeline scans for symbols.
pub(crate) fn supported_ext(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let ext = match lower.rfind('.') {
        Some(i) => &lower[i..],
        None => return false,
    };
    matches!(
        ext,
        ".go" | ".ts" | ".tsx" | ".js" | ".jsx" | ".mjs" | ".py"
    )
}

/// basename returns the path component after the last forward slash.
pub(crate) fn basename(path: &str) -> &str {
    match path.rfind('/') {
        Some(i) => &path[i + 1..],
        None => path,
    }
}

/// stem returns the basename with its final extension stripped.
pub(crate) fn stem(path: &str) -> &str {
    let bn = basename(path);
    match bn.rfind('.') {
        Some(i) if i > 0 => &bn[..i],
        _ => bn,
    }
}

/// dirname returns the path with its basename stripped (ToSlash form).
pub(crate) fn dirname(path: &str) -> &str {
    match path.rfind('/') {
        Some(i) => &path[..i],
        None => "",
    }
}

/// resolve_anchor runs the Go-side three-pass resolution against the
/// already-walked corpus. Returns:
///
///   * `Ok(Anchor)` — unique resolution.
///   * `Err(ErrAmbiguous)` — multiple symbol matches (carrying candidates).
///   * `Ok(Anchor)` with `kind = Basename` / `kind = Path` for fallback hits.
///   * `Err(...)` with empty candidates list — not found.
pub fn resolve_anchor(files: &[FileInput], raw: &str) -> Result<Anchor, ErrAmbiguous> {
    // ── Pass 1: symbol exact match ──────────────────────────────────────
    let mut candidates: Vec<Candidate> = Vec::new();
    for fi in files {
        if fi.is_dir || !supported_ext(&fi.path) {
            continue;
        }
        for sym in &fi.symbols {
            if sym.name == raw {
                candidates.push(Candidate {
                    path: to_slash(&fi.path),
                    line: sym.line,
                    kind: sym.kind.clone(),
                });
            }
        }
    }

    match candidates.len() {
        1 => {
            return Ok(Anchor {
                kind: AnchorKind::Symbol,
                raw: raw.to_string(),
                name: raw.to_string(),
                origin_path: candidates.into_iter().next().unwrap().path,
            });
        }
        n if n > 1 => {
            return Err(ErrAmbiguous {
                anchor: raw.to_string(),
                candidates,
            });
        }
        _ => {}
    }

    // ── Pass 2: basename match ──────────────────────────────────────────
    for fi in files {
        if fi.is_dir {
            continue;
        }
        if basename(&fi.path) == raw {
            return Ok(Anchor {
                kind: AnchorKind::Basename,
                raw: raw.to_string(),
                name: raw.to_string(),
                origin_path: to_slash(&fi.path),
            });
        }
    }

    // ── Pass 3: path match ──────────────────────────────────────────────
    let normalised = clean_path(raw);
    for fi in files {
        if fi.is_dir {
            continue;
        }
        if to_slash(&fi.path) == normalised {
            return Ok(Anchor {
                kind: AnchorKind::Path,
                raw: raw.to_string(),
                name: raw.to_string(),
                origin_path: to_slash(&fi.path),
            });
        }
    }

    Err(ErrAmbiguous {
        anchor: raw.to_string(),
        candidates: Vec::new(),
    })
}

fn to_slash(s: &str) -> String {
    s.replace('\\', "/")
}

/// clean_path is a minimal filepath.Clean+ToSlash replacement covering
/// the cases focus actually encounters (single-segment paths, repo-rel).
fn clean_path(raw: &str) -> String {
    let slashy = raw.replace('\\', "/");
    // Collapse runs of '/' and drop trailing slash (matches filepath.Clean
    // behaviour for the test inputs we accept).
    let mut out = String::with_capacity(slashy.len());
    let mut prev = '\0';
    for c in slashy.chars() {
        if c == '/' && prev == '/' {
            continue;
        }
        out.push(c);
        prev = c;
    }
    if out.len() > 1 && out.ends_with('/') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SymbolInfo;

    fn mkfile(path: &str, syms: &[(&str, &str, i64)]) -> FileInput {
        FileInput {
            path: path.into(),
            is_dir: false,
            symbols: syms
                .iter()
                .map(|(n, k, l)| SymbolInfo {
                    name: (*n).into(),
                    kind: (*k).into(),
                    line: *l,
                })
                .collect(),
            lines: vec![],
        }
    }

    #[test]
    fn resolve_unique_symbol_returns_symbol_anchor() {
        let files = vec![mkfile("internal/pack/pack.go", &[("Pack", "function", 3)])];
        let a = resolve_anchor(&files, "Pack").unwrap();
        assert_eq!(a.kind, AnchorKind::Symbol);
        assert_eq!(a.origin_path, "internal/pack/pack.go");
    }

    #[test]
    fn resolve_ambiguous_returns_candidates() {
        let files = vec![
            mkfile("internal/pack/pack.go", &[("Pack", "function", 3)]),
            mkfile("internal/other/other.go", &[("Pack", "function", 2)]),
        ];
        let err = resolve_anchor(&files, "Pack").unwrap_err();
        assert_eq!(err.candidates.len(), 2);
        assert_eq!(err.anchor, "Pack");
    }

    #[test]
    fn resolve_basename_fallback() {
        let files = vec![mkfile("cmd/main.go", &[])];
        let a = resolve_anchor(&files, "main.go").unwrap();
        assert_eq!(a.kind, AnchorKind::Basename);
    }

    #[test]
    fn resolve_path_fallback() {
        let files = vec![mkfile("internal/render/render.go", &[])];
        let a = resolve_anchor(&files, "internal/render/render.go").unwrap();
        assert_eq!(a.kind, AnchorKind::Path);
    }

    #[test]
    fn resolve_not_found_returns_empty_candidates() {
        let files = vec![mkfile("a.go", &[])];
        let err = resolve_anchor(&files, "NonexistentSymbol").unwrap_err();
        assert!(err.candidates.is_empty());
    }

    #[test]
    fn supported_ext_recognises_all_targets() {
        assert!(supported_ext("a.go"));
        assert!(supported_ext("b.ts"));
        assert!(supported_ext("c.tsx"));
        assert!(supported_ext("d.JS"));
        assert!(supported_ext("e.py"));
        assert!(!supported_ext("f.rs"));
        assert!(!supported_ext("g"));
    }
}
