use std::path::Path;

use crate::util::{classify_walk_entry, WalkEntry};

pub(crate) struct SymbolEntry {
    pub(crate) path: String,
    pub(crate) symbols: Vec<ctx_symbols::Symbol>,
}

/// `boundary` is the directory symlinked files must resolve into (see
/// [`classify_walk_entry`]); usually the server root.
pub(crate) fn collect_symbol_entries(
    root: &Path,
    boundary: &Path,
) -> Result<Vec<SymbolEntry>, String> {
    let mut out = Vec::new();
    collect_symbol_entries_inner(root, boundary, root, &mut out)?;
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

pub(crate) fn collect_symbol_entries_inner(
    root: &Path,
    boundary: &Path,
    dir: &Path,
    out: &mut Vec<SymbolEntry>,
) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|err| format!("walk {}: {err}", dir.display()))? {
        let entry = entry.map_err(|err| err.to_string())?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || name == "node_modules" || name == "dist" || name == "coverage" {
            continue;
        }
        match classify_walk_entry(boundary, &entry)? {
            Some(WalkEntry::Dir) => {
                collect_symbol_entries_inner(root, boundary, &path, out)?;
                continue;
            }
            Some(WalkEntry::File(_)) => {}
            None => continue,
        }
        let symbols = ctx_symbols::extract(&path).map_err(|err| err.to_string())?;
        if symbols.is_empty() {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .map_err(|err| err.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        out.push(SymbolEntry { path: rel, symbols });
    }
    Ok(())
}
