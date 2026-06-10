use std::path::Path;

pub(crate) struct SymbolEntry {
    pub(crate) path: String,
    pub(crate) symbols: Vec<ctx_symbols::Symbol>,
}

pub(crate) fn collect_symbol_entries(root: &Path) -> Result<Vec<SymbolEntry>, String> {
    let mut out = Vec::new();
    collect_symbol_entries_inner(root, root, &mut out)?;
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

pub(crate) fn collect_symbol_entries_inner(
    root: &Path,
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
        if path.is_dir() {
            collect_symbol_entries_inner(root, &path, out)?;
            continue;
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
