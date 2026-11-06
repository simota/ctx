use std::path::{Path, PathBuf};

use rayon::prelude::*;

use crate::util::{classify_walk_entry, WalkEntry};

pub(crate) struct SymbolEntry {
    pub(crate) path: String,
    pub(crate) symbols: Vec<ctx_symbols::Symbol>,
}

/// `boundary` is the directory symlinked files must resolve into (see
/// [`classify_walk_entry`]); usually the server root.
///
/// [`collect_symbol_entries`] on the rayon pool, for `ctx_symbols`. Two
/// phases: a sequential walk lists the files, then they are parsed in
/// parallel and taken in walk order. The result and the error returned are
/// exactly the sequential walk's: the first failing file in walk order wins,
/// and a walk error (which stopped the listing) is reported only when every
/// file listed before it parsed.
pub(crate) fn collect_symbol_entries_parallel(
    root: &Path,
    boundary: &Path,
) -> Result<Vec<SymbolEntry>, String> {
    let mut paths = Vec::new();
    let walk_error = list_symbol_files(boundary, root, &mut paths).err();
    let parsed: Vec<std::io::Result<Vec<ctx_symbols::Symbol>>> = paths
        .par_iter()
        .map(ctx_symbols::extract_on_worker)
        .collect();
    let mut out = Vec::new();
    for (path, symbols) in paths.iter().zip(parsed) {
        let symbols = symbols.map_err(|err| err.to_string())?;
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
    if let Some(err) = walk_error {
        return Err(err);
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// The files [`collect_symbol_entries_parallel`] parses, in walk order. On a walk
/// error, `out` holds the files listed before it.
fn list_symbol_files(boundary: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
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
                list_symbol_files(boundary, &path, out)?;
                continue;
            }
            Some(WalkEntry::File(_)) => {}
            None => continue,
        }
        out.push(path);
    }
    Ok(())
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
