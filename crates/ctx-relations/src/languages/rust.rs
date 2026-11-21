// crates/ctx-relations/src/languages/rust.rs
//
// Rust extractor. Unlike its siblings this is not a port: the Go
// implementation never resolved Rust.
//
// Edges are file-level, like every other language here. A Rust file gets an
// edge to:
//
//   - the file of the deepest module named by a path that starts at `crate`,
//     `self`, `super` or the name of another crate found in the tree
//     (`use ctx_pack::excerpt::Spec`, or inline `crate::store::open()`),
//     falling back to the module / crate root when the path names an item
//     defined directly in it;
//   - a child module it declares (`mod foo;`, `#[path = "x.rs"] mod foo;`)
//     and then names in a path (`pub use foo::*;`, `foo::run()`).
//
// A bare `mod foo;` adds no edge by itself: a parent that only declares a
// module does not depend on it, and counting it would make every module
// root a dependent of every file below it.
//
// Resolution follows the file-system convention (`a.rs` or `a/mod.rs`); it
// does not expand macros, follow re-exports or evaluate `cfg`. Paths inside
// an inline `mod name { .. }` block are interpreted relative to that block,
// so the ubiquitous `mod tests { use super::*; }` adds no edge.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use once_cell::sync::Lazy;
use regex::Regex;

use super::common::{parent_slash, FileEntry};

/// One Cargo package found in the walk.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CrateInfo {
    /// Repo-relative directory holding Cargo.toml ("" for the root).
    dir: String,
    /// Library root file (`src/lib.rs`, `[lib] path`), when the package has one.
    lib_root: Option<String>,
    /// Root file `crate::` falls back to: the library root, else `src/main.rs`.
    root_file: Option<String>,
    /// Directory the crate's top-level modules live in (usually `<dir>/src`).
    src_dir: String,
}

/// Cargo packages of the tree, by directory and by crate name.
#[derive(Debug, Default)]
pub struct RustIndex {
    /// Sorted by directory length, longest first (nearest ancestor wins).
    crates: Vec<CrateInfo>,
    /// Crate name as written in Rust source (`ctx_pack`) → index in `crates`.
    by_name: HashMap<String, usize>,
    /// For packages with both a library and a `src/main.rs`: each file of
    /// the binary's module tree → `src/main.rs`, so `crate::` in it means
    /// the binary rather than the library.
    bin_root_of: HashMap<String, String>,
    /// `#[path = "..."] mod name;` declarations: the module's conventional
    /// location (`<children dir of the declaring file>/name`) → its file.
    path_mods: HashMap<String, String>,
}

/// Directory a module file's children live in.
fn child_dir(rel: &str, is_root: bool) -> String {
    let dir = parent_slash(rel);
    let base = basename(rel);
    if is_root || base == "mod.rs" {
        dir
    } else {
        join(&dir, base.strip_suffix(".rs").unwrap_or(base))
    }
}

/// Every file in the module tree under `root`, found by following its
/// out-of-line `mod` declarations (incl. those inside inline modules).
fn module_tree(root: &str, abs_of: &HashMap<&str, &Path>, all: &HashSet<String>) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::from([root.to_string()]);
    let mut queue = vec![root.to_string()];
    let mut out = Vec::new();
    while let Some(rel) = queue.pop() {
        let Some(src) = abs_of
            .get(rel.as_str())
            .and_then(|abs| fs::read_to_string(abs).ok())
        else {
            continue;
        };
        let (no_comments, code) = sanitize(&src);
        let mods = inline_mods(&code);
        let base_dir = child_dir(&rel, rel == root);
        for caps in MOD_DECL_RE.captures_iter(&no_comments) {
            let at = caps.get(2).unwrap().start();
            if code.as_bytes().get(at) != no_comments.as_bytes().get(at) {
                continue;
            }
            let target = match caps.get(1) {
                Some(path) => normalize(&join(&parent_slash(&rel), path.as_str())),
                None => {
                    let mut dir = base_dir.clone();
                    for name in enclosing(&mods, at) {
                        dir = join(&dir, name);
                    }
                    let name = &caps[2];
                    [
                        join(&dir, &format!("{name}.rs")),
                        join(&dir, &format!("{name}/mod.rs")),
                    ]
                    .into_iter()
                    .find(|path| all.contains(path))
                }
            };
            if let Some(target) = target.filter(|path| all.contains(path)) {
                if seen.insert(target.clone()) {
                    queue.push(target);
                }
            }
        }
        out.push(rel);
    }
    out
}

fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

fn basename(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

/// `dir/../x` → `x`. `None` when the path climbs out of the tree.
fn normalize(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    Some(parts.join("/"))
}

/// `[package] name`, `[lib] name` and `[lib] path` of a manifest. A line
/// parser is enough for these three string keys and keeps this crate free of
/// a TOML dependency.
fn parse_manifest(body: &str) -> (Option<String>, Option<String>, Option<String>) {
    let (mut package, mut lib_name, mut lib_path) = (None, None, None);
    let mut section = String::new();
    for line in body.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            section = line
                .trim_matches(|c| c == '[' || c == ']')
                .trim()
                .to_string();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        let Some(value) = value
            .strip_prefix('"')
            .and_then(|rest| rest.split('"').next())
        else {
            continue;
        };
        match (section.as_str(), key.trim()) {
            ("package", "name") => package = Some(value.to_string()),
            ("lib", "name") => lib_name = Some(value.to_string()),
            ("lib", "path") => lib_path = Some(value.to_string()),
            _ => {}
        }
    }
    (package, lib_name, lib_path)
}

/// Finds every Cargo package among `files`.
pub fn build_rust_index(files: &[FileEntry], all: &HashSet<String>) -> RustIndex {
    let mut crates = Vec::new();
    let mut names = Vec::new();
    for fi in files {
        if fi.is_dir || basename(&fi.rel) != "Cargo.toml" {
            continue;
        }
        let Ok(body) = fs::read_to_string(&fi.abs) else {
            continue;
        };
        let (Some(package), lib_name, lib_path) = parse_manifest(&body) else {
            continue; // a virtual workspace manifest
        };
        let dir = parent_slash(&fi.rel);
        let lib_root = lib_path
            .and_then(|path| normalize(&join(&dir, &path)))
            .or_else(|| Some(join(&dir, "src/lib.rs")))
            .filter(|path| all.contains(path));
        let main = Some(join(&dir, "src/main.rs")).filter(|path| all.contains(path));
        let src_dir = match &lib_root {
            Some(root) => parent_slash(root),
            None => join(&dir, "src"),
        };
        names.push(lib_name.unwrap_or(package).replace('-', "_"));
        crates.push(CrateInfo {
            dir,
            root_file: lib_root.clone().or(main),
            lib_root,
            src_dir,
        });
    }
    let mut order: Vec<usize> = (0..crates.len()).collect();
    order.sort_by(|&a, &b| {
        crates[b]
            .dir
            .len()
            .cmp(&crates[a].dir.len())
            .then_with(|| crates[a].dir.cmp(&crates[b].dir))
    });
    let mut index = RustIndex::default();
    let abs_of: HashMap<&str, &Path> = files
        .iter()
        .filter(|fi| !fi.is_dir)
        .map(|fi| (fi.rel.as_str(), fi.abs.as_path()))
        .collect();
    for krate in &crates {
        let (Some(lib_root), main) = (&krate.lib_root, join(&krate.dir, "src/main.rs")) else {
            continue;
        };
        if !all.contains(&main) {
            continue;
        }
        // A module both targets declare stays with the library.
        let lib: HashSet<String> = module_tree(lib_root, &abs_of, all).into_iter().collect();
        for rel in module_tree(&main, &abs_of, all) {
            if !lib.contains(&rel) {
                index.bin_root_of.insert(rel, main.clone());
            }
        }
    }
    for old in order {
        // The first package to claim a name keeps it: nearest to the root
        // last, so a fixture crate deep in the tree cannot shadow… a name is
        // normally unique, and when it is not there is no right answer.
        index
            .by_name
            .entry(names[old].clone())
            .or_insert(index.crates.len());
        index.crates.push(crates[old].clone());
    }
    // `#[path]` modules, keyed by where the convention would look for them,
    // so qualified paths from any file (`crate::sys::Handle`) find them.
    let mut path_mods = Vec::new();
    for fi in files {
        if fi.is_dir || !fi.rel.ends_with(".rs") {
            continue;
        }
        let Ok(src) = fs::read_to_string(&fi.abs) else {
            continue;
        };
        if !src.contains("#[path") {
            continue;
        }
        let (no_comments, code) = sanitize(&src);
        let mods = inline_mods(&code);
        let ctx = FileCtx::new(&fi.rel, &index, all);
        for caps in MOD_DECL_RE.captures_iter(&no_comments) {
            let (Some(path), name) = (caps.get(1), caps.get(2).unwrap()) else {
                continue;
            };
            if code.as_bytes().get(name.start()) != no_comments.as_bytes().get(name.start()) {
                continue;
            }
            let Some(target) = normalize(&join(&parent_slash(&fi.rel), path.as_str()))
                .filter(|path| all.contains(path))
            else {
                continue;
            };
            let inline = enclosing(&mods, name.start());
            path_mods.push((join(&ctx.inline_dir(&inline), name.as_str()), target));
        }
    }
    index.path_mods.extend(path_mods);
    index
}

impl RustIndex {
    fn crate_of(&self, rel: &str) -> Option<&CrateInfo> {
        self.crates.iter().find(|krate| {
            krate.dir.is_empty()
                || rel
                    .strip_prefix(krate.dir.as_str())
                    .is_some_and(|rest| rest.starts_with('/'))
        })
    }
}

/// Blanks comments (in both results) and string / char literals (in the
/// second), byte for byte, so offsets and line breaks are preserved.
fn sanitize(src: &str) -> (String, String) {
    let bytes = src.as_bytes();
    let mut no_comments = bytes.to_vec();
    let mut code = bytes.to_vec();
    let blank = |buf: &mut Vec<u8>, from: usize, to: usize| {
        for b in &mut buf[from..to] {
            if *b != b'\n' {
                *b = b' ';
            }
        }
    };
    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'/' && bytes.get(i + 1) == Some(&b'/') {
            let end = bytes[i..]
                .iter()
                .position(|&c| c == b'\n')
                .map_or(bytes.len(), |n| i + n);
            blank(&mut no_comments, i, end);
            blank(&mut code, i, end);
            i = end;
        } else if b == b'/' && bytes.get(i + 1) == Some(&b'*') {
            let mut depth = 1;
            let mut j = i + 2;
            while j < bytes.len() && depth > 0 {
                if bytes[j] == b'/' && bytes.get(j + 1) == Some(&b'*') {
                    depth += 1;
                    j += 2;
                } else if bytes[j] == b'*' && bytes.get(j + 1) == Some(&b'/') {
                    depth -= 1;
                    j += 2;
                } else {
                    j += 1;
                }
            }
            blank(&mut no_comments, i, j);
            blank(&mut code, i, j);
            i = j;
        } else if b == b'"' {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] != b'"' {
                j += if bytes[j] == b'\\' { 2 } else { 1 };
            }
            let end = j.min(bytes.len());
            blank(&mut code, i + 1, end);
            i = end + 1;
        } else if b == b'r'
            && matches!(bytes.get(i + 1), Some(b'"') | Some(b'#'))
            && (i == 0
                || !is_ident(bytes[i - 1])
                || (bytes[i - 1] == b'b' && (i < 2 || !is_ident(bytes[i - 2]))))
        {
            let mut hashes = 0;
            let mut j = i + 1;
            while bytes.get(j) == Some(&b'#') {
                hashes += 1;
                j += 1;
            }
            if bytes.get(j) != Some(&b'"') {
                i += 1; // `r#ident`
                continue;
            }
            let start = j + 1;
            let mut end = bytes.len();
            let mut k = start;
            while k < bytes.len() {
                if bytes[k] == b'"'
                    && bytes[k + 1..].len() >= hashes
                    && bytes[k + 1..k + 1 + hashes].iter().all(|&c| c == b'#')
                {
                    end = k;
                    break;
                }
                k += 1;
            }
            blank(&mut code, start, end);
            i = (end + 1 + hashes).min(bytes.len());
        } else if b == b'\'' {
            // A char literal ('x', '\n', '\u{1F600}') — anything else is a
            // lifetime and stays.
            let close = if bytes.get(i + 1) == Some(&b'\\') {
                bytes[i + 2..]
                    .iter()
                    .take(12)
                    .position(|&c| c == b'\'')
                    .map(|n| i + 2 + n)
            } else {
                src[i + 1..]
                    .chars()
                    .next()
                    .map(|c| i + 1 + c.len_utf8())
                    .filter(|&end| bytes.get(end) == Some(&b'\''))
            };
            match close {
                Some(end) => {
                    blank(&mut code, i + 1, end);
                    i = end + 1;
                }
                None => i += 1,
            }
        } else {
            i += 1;
        }
    }
    (
        String::from_utf8(no_comments).unwrap_or_default(),
        String::from_utf8(code).unwrap_or_default(),
    )
}

const IDENT: &str = r"[A-Za-z_][A-Za-z0-9_]*";

static MOD_DECL_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(
        r#"(?m)^[ \t]*(?:#\[path\s*=\s*"([^"]+)"\]\s*)?(?:pub(?:\s*\([^)]*\))?\s+)?mod\s+({IDENT})\s*;"#
    ))
    .unwrap()
});
static INLINE_MOD_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(&format!(r"\bmod\s+({IDENT})\s*\{{")).unwrap());
static PATH_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(&format!(r"\b{IDENT}(?:\s*::\s*{IDENT})+")).unwrap());
static USE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\buse\s+([^;{}]*(?:\{[^;]*\})?[^;{}]*);").unwrap());
static EXTERN_CRATE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(&format!(r"\bextern\s+crate\s+({IDENT})")).unwrap());

/// An inline `mod name { .. }` block: byte range of its body and its name.
struct InlineMod {
    start: usize,
    end: usize,
    name: String,
}

fn inline_mods(code: &str) -> Vec<InlineMod> {
    let bytes = code.as_bytes();
    INLINE_MOD_RE
        .captures_iter(code)
        .map(|caps| {
            let start = caps.get(0).unwrap().end();
            let mut depth = 1;
            let mut end = start;
            while end < bytes.len() && depth > 0 {
                match bytes[end] {
                    b'{' => depth += 1,
                    b'}' => depth -= 1,
                    _ => {}
                }
                end += 1;
            }
            InlineMod {
                start,
                end,
                name: caps[1].to_string(),
            }
        })
        .collect()
}

/// Names of the inline modules enclosing byte offset `at`, outermost first.
fn enclosing(mods: &[InlineMod], at: usize) -> Vec<&str> {
    mods.iter()
        .filter(|m| m.start <= at && at < m.end)
        .map(|m| m.name.as_str())
        .collect()
}

/// Expands a `use` tree (`a::{b, c::{d, self}, e as f}`) into flat paths.
/// `prefix` carries the segments above the tree being expanded.
fn expand_use(tree: &str, prefix: &[String], out: &mut Vec<Vec<String>>) {
    let tree = tree.trim();
    if tree.is_empty() {
        return;
    }
    let (head, group) = match tree.find('{') {
        Some(open) => {
            let close = tree.rfind('}').unwrap_or(tree.len());
            (&tree[..open], Some(&tree[open + 1..close.max(open + 1)]))
        }
        None => (tree, None),
    };
    let head = head.split(" as ").next().unwrap_or(head);
    let mut path = prefix.to_vec();
    for seg in head.split("::") {
        let seg = seg.trim();
        if seg.is_empty() || seg == "*" {
            continue;
        }
        if seg == "self" && !path.is_empty() {
            continue; // `a::{self}` names `a`
        }
        path.push(seg.to_string());
    }
    match group {
        None => out.push(path),
        Some(group) => {
            // Split on the commas of this nesting level only.
            let mut depth = 0;
            let mut start = 0;
            for (i, c) in group.char_indices() {
                match c {
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    ',' if depth == 0 => {
                        expand_use(&group[start..i], &path, out);
                        start = i + 1;
                    }
                    _ => {}
                }
            }
            expand_use(&group[start..], &path, out);
        }
    }
}

/// Where a file sits in its crate's module tree.
struct FileCtx<'a> {
    rel: &'a str,
    all: &'a HashSet<String>,
    index: &'a RustIndex,
    /// Directory holding this module's children.
    mod_dir: String,
    /// Directory holding this module's siblings (`None` for a crate root).
    super_dir: Option<String>,
    /// Directory of the top-level modules `crate::` starts from.
    crate_dir: Option<String>,
    /// File `crate::Item` falls back to.
    crate_root: Option<String>,
}

impl<'a> FileCtx<'a> {
    fn new(rel: &'a str, index: &'a RustIndex, all: &'a HashSet<String>) -> Self {
        let dir = parent_slash(rel);
        let base = basename(rel);
        let stem = base.strip_suffix(".rs").unwrap_or(base);
        let krate = index.crate_of(rel);
        let in_crate = match krate {
            Some(krate) if !krate.dir.is_empty() => &rel[krate.dir.len() + 1..],
            _ => rel,
        };
        let in_crate_dir = parent_slash(in_crate);
        // Every file directly in one of these is the root of its own target.
        let target_root = matches!(
            in_crate_dir.as_str(),
            "tests" | "benches" | "examples" | "src/bin"
        ) || in_crate == "build.rs";
        let is_lib_root = krate.is_some_and(|k| k.lib_root.as_deref() == Some(rel));
        let crate_root_file = target_root || is_lib_root || matches!(base, "lib.rs" | "main.rs");
        let mod_dir = if crate_root_file || base == "mod.rs" {
            dir.clone()
        } else {
            join(&dir, stem)
        };
        let super_dir = if crate_root_file {
            None
        } else if base == "mod.rs" {
            (!dir.is_empty()).then(|| parent_slash(&dir))
        } else {
            Some(dir.clone())
        };
        let top = in_crate.split('/').next().unwrap_or("");
        let (crate_dir, crate_root) = if target_root {
            (Some(dir.clone()), None)
        } else if matches!(top, "tests" | "benches" | "examples") {
            let base_dir = krate.map_or("", |k| k.dir.as_str());
            (Some(join(base_dir, top)), None)
        } else if let Some(main) = index.bin_root_of.get(rel) {
            (Some(parent_slash(main)), Some(main.clone()))
        } else if let Some(krate) = krate {
            (Some(krate.src_dir.clone()), krate.root_file.clone())
        } else {
            // No manifest in the tree: the nearest `src` directory is the
            // best available guess for the crate root.
            let parts: Vec<&str> = dir.split('/').collect();
            match parts.iter().rposition(|part| *part == "src") {
                Some(at) => {
                    let src = parts[..=at].join("/");
                    let root = ["lib.rs", "main.rs"]
                        .iter()
                        .map(|name| join(&src, name))
                        .find(|path| all.contains(path));
                    (Some(src), root)
                }
                None => (None, None),
            }
        };
        Self {
            rel,
            all,
            index,
            mod_dir,
            super_dir,
            crate_dir,
            crate_root,
        }
    }

    /// The file of the deepest module `segments` names below `dir`.
    fn descend(&self, dir: &str, segments: &[String]) -> Option<String> {
        let mut dir = dir.to_string();
        let mut best = None;
        for seg in segments {
            // Modules are snake_case; a capitalised segment is a type, and
            // nothing after it is a module.
            if seg.starts_with(|c: char| c.is_ascii_uppercase()) {
                break;
            }
            let file = join(&dir, &format!("{seg}.rs"));
            let nested = join(&dir, &format!("{seg}/mod.rs"));
            let module = join(&dir, seg);
            if let Some(target) = self.index.path_mods.get(&module) {
                best = Some(target.clone());
                dir = child_dir(target, false);
                continue;
            }
            if self.all.contains(&file) {
                best = Some(file);
            } else if self.all.contains(&nested) {
                best = Some(nested);
            }
            dir = join(&dir, seg);
        }
        best
    }

    /// Directory the out-of-line children of the inline module path
    /// `inline` (outermost first) live in.
    fn inline_dir(&self, inline: &[&str]) -> String {
        inline
            .iter()
            .fold(self.mod_dir.clone(), |dir, name| join(&dir, name))
    }

    /// The file that defines the module whose children live in `dir`.
    fn module_file(&self, dir: &str) -> Option<String> {
        let mut candidates = vec![
            join(dir, "mod.rs"),
            join(dir, "lib.rs"),
            join(dir, "main.rs"),
        ];
        if !dir.is_empty() {
            candidates.insert(1, format!("{dir}.rs"));
        }
        candidates.into_iter().find(|path| self.all.contains(path))
    }

    /// Resolves one path, written inside the inline modules `inline`.
    /// `declared` maps the child modules this file declares to their files;
    /// `from_use` lets any bare first segment name a child module.
    fn resolve(
        &self,
        segments: &[String],
        inline: &[&str],
        declared: &HashMap<String, String>,
        from_use: bool,
    ) -> Option<String> {
        let (first, rest) = segments.split_first()?;
        match first.as_str() {
            "crate" => {
                let dir = self.crate_dir.as_deref()?;
                self.descend(dir, rest).or_else(|| self.crate_root.clone())
            }
            "self" => self.descend(&self.inline_dir(inline), rest),
            "super" => {
                let supers = 1 + rest.iter().take_while(|seg| *seg == "super").count();
                let rest = &rest[supers - 1..];
                // Each enclosing inline module absorbs one `super`; the path
                // then continues from a module of this same file, and only
                // its out-of-line descendants are other files.
                if supers <= inline.len() {
                    let dir = self.inline_dir(&inline[..inline.len() - supers]);
                    return self.descend(&dir, rest);
                }
                let above = supers - inline.len();
                let mut dir = self.super_dir.clone()?;
                for _ in 1..above {
                    if dir.is_empty() {
                        return None;
                    }
                    dir = parent_slash(&dir);
                }
                self.descend(&dir, rest).or_else(|| self.module_file(&dir))
            }
            name => {
                if let Some(&at) = self.index.by_name.get(name) {
                    let krate = &self.index.crates[at];
                    if let Some(lib_root) = &krate.lib_root {
                        return self
                            .descend(&krate.src_dir, rest)
                            .or_else(|| Some(lib_root.clone()));
                    }
                }
                if !inline.is_empty() {
                    return None;
                }
                if let Some(file) = declared.get(name) {
                    return self
                        .descend(&self.mod_dir, segments)
                        .or_else(|| Some(file.clone()));
                }
                if from_use {
                    return self.descend(&self.mod_dir, segments);
                }
                None
            }
        }
    }
}

/// Files `from_rel` depends on. Never contains `from_rel` itself.
pub fn resolve_rust_imports(
    abs_path: &Path,
    from_rel: &str,
    index: &RustIndex,
    all: &HashSet<String>,
) -> Vec<String> {
    let Ok(src) = fs::read_to_string(abs_path) else {
        return Vec::new();
    };
    let (no_comments, code) = sanitize(&src);
    let mods = inline_mods(&code);
    let ctx = FileCtx::new(from_rel, index, all);
    let mut out = Vec::new();

    let mut declared = HashMap::new();
    for caps in MOD_DECL_RE.captures_iter(&no_comments) {
        let at = caps.get(2).unwrap().start();
        // The declaration must survive string blanking, or it is text
        // inside a literal; one nested in an inline module is out of scope.
        if code.as_bytes().get(at) != no_comments.as_bytes().get(at)
            || !enclosing(&mods, at).is_empty()
        {
            continue;
        }
        let name = caps[2].to_string();
        let target = match caps.get(1) {
            Some(path) => normalize(&join(&parent_slash(from_rel), path.as_str()))
                .filter(|path| all.contains(path)),
            None => ctx.descend(&ctx.mod_dir, std::slice::from_ref(&name)),
        };
        if let Some(target) = target {
            declared.insert(name, target);
        }
    }

    for caps in USE_RE.captures_iter(&code) {
        let whole = caps.get(0).unwrap();
        let tree = caps[1].trim();
        if tree.starts_with("::") {
            continue;
        }
        let inline = enclosing(&mods, whole.start());
        let mut paths = Vec::new();
        expand_use(tree, &[], &mut paths);
        for path in paths {
            out.extend(ctx.resolve(&path, &inline, &declared, true));
        }
    }

    for found in PATH_RE.find_iter(&code) {
        // `<T as X>::a::b` and `Vec::<T>::a::b` continue an earlier path.
        if code[..found.start()].trim_end().ends_with("::") {
            continue;
        }
        let segments: Vec<String> = found
            .as_str()
            .split("::")
            .map(|seg| seg.trim().to_string())
            .collect();
        let inline = enclosing(&mods, found.start());
        out.extend(ctx.resolve(&segments, &inline, &declared, false));
    }

    for caps in EXTERN_CRATE_RE.captures_iter(&code) {
        out.extend(ctx.resolve(&[caps[1].to_string()], &[], &declared, false));
    }

    out.retain(|path| path != ctx.rel);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tree(files: &[(&str, &str)]) -> (tempdir::Dir, Vec<FileEntry>, HashSet<String>) {
        let dir = tempdir::Dir::new();
        let mut entries = Vec::new();
        for (rel, content) in files {
            let abs = dir.path.join(rel);
            fs::create_dir_all(abs.parent().unwrap()).unwrap();
            fs::write(&abs, content).unwrap();
            entries.push(FileEntry {
                rel: rel.to_string(),
                abs,
                is_dir: false,
            });
        }
        let all = entries.iter().map(|e| e.rel.clone()).collect();
        (dir, entries, all)
    }

    /// Minimal self-cleaning temp dir (the crate has no tempfile dev-dep).
    mod tempdir {
        use super::PathBuf;
        use std::sync::atomic::{AtomicUsize, Ordering};

        static NEXT: AtomicUsize = AtomicUsize::new(0);

        pub struct Dir {
            pub path: PathBuf,
        }

        impl Dir {
            pub fn new() -> Self {
                let path = std::env::temp_dir().join(format!(
                    "ctx-relations-rust-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::SeqCst)
                ));
                std::fs::create_dir_all(&path).unwrap();
                Self { path }
            }
        }

        impl Drop for Dir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.path);
            }
        }
    }

    fn imports(files: &[(&str, &str)], of: &str) -> Vec<String> {
        let (_dir, entries, all) = tree(files);
        let index = build_rust_index(&entries, &all);
        let entry = entries.iter().find(|e| e.rel == of).unwrap();
        let mut found = resolve_rust_imports(&entry.abs, of, &index, &all);
        found.sort();
        found.dedup();
        found
    }

    const MANIFEST: &str = "[package]\nname = \"app\"\n";

    #[test]
    fn declared_modules_become_edges_only_when_used() {
        let files = [
            ("Cargo.toml", MANIFEST),
            (
                "src/lib.rs",
                "mod store;\npub mod net;\npub(crate) mod util;\nmod unused;\n// mod gone;\npub use store::Store;\nfn f() { net::connect(); util::helper::go(); gone::x(); }\n",
            ),
            ("src/store.rs", ""),
            ("src/net/mod.rs", "mod http;\npub use http::*;\n"),
            ("src/net/http.rs", ""),
            ("src/util.rs", ""),
            ("src/unused.rs", ""),
            ("src/gone.rs", ""),
        ];
        assert_eq!(
            imports(&files, "src/lib.rs"),
            ["src/net/mod.rs", "src/store.rs", "src/util.rs"]
        );
        assert_eq!(imports(&files, "src/net/mod.rs"), ["src/net/http.rs"]);
    }

    #[test]
    fn non_mod_rs_file_declares_children_in_its_own_directory() {
        let files = [
            ("Cargo.toml", MANIFEST),
            ("src/lib.rs", "mod net;\n"),
            ("src/net.rs", "mod http;\nfn f() { http::get(); }\n"),
            ("src/net/http.rs", "use super::Client;\n"),
        ];
        assert!(imports(&files, "src/lib.rs").is_empty());
        assert_eq!(imports(&files, "src/net.rs"), ["src/net/http.rs"]);
        // `super::Client` is an item of the parent module file.
        assert_eq!(imports(&files, "src/net/http.rs"), ["src/net.rs"]);
    }

    #[test]
    fn path_attribute_overrides_the_convention() {
        let files = [
            ("Cargo.toml", MANIFEST),
            (
                "src/lib.rs",
                "#[path = \"imp/unix.rs\"]\nmod sys;\npub use sys::Handle;\n",
            ),
            ("src/imp/unix.rs", ""),
        ];
        assert_eq!(imports(&files, "src/lib.rs"), ["src/imp/unix.rs"]);
    }

    #[test]
    fn path_attribute_modules_resolve_from_qualified_paths() {
        let files = [
            ("Cargo.toml", MANIFEST),
            (
                "src/lib.rs",
                "#[path = \"platform/unix.rs\"]\npub mod sys;\npub mod api;\n",
            ),
            ("src/platform/unix.rs", "pub struct Handle;\n"),
            ("src/api.rs", "use crate::sys::Handle;\n"),
            ("src/sys.rs", ""),
        ];
        assert_eq!(imports(&files, "src/api.rs"), ["src/platform/unix.rs"]);
    }

    #[test]
    fn crate_paths_resolve_to_the_deepest_module_file() {
        let files = [
            ("Cargo.toml", MANIFEST),
            ("src/lib.rs", "pub struct Config;\n"),
            ("src/store/mod.rs", ""),
            ("src/store/disk.rs", ""),
            ("src/api.rs", "use crate::store::disk::Disk;\nuse crate::Config;\nfn f() { crate::store::open(); }\n"),
        ];
        assert_eq!(
            imports(&files, "src/api.rs"),
            ["src/lib.rs", "src/store/disk.rs", "src/store/mod.rs"]
        );
    }

    #[test]
    fn use_groups_are_expanded() {
        let files = [
            ("Cargo.toml", MANIFEST),
            ("src/lib.rs", ""),
            ("src/a.rs", ""),
            ("src/b/mod.rs", ""),
            ("src/b/c.rs", ""),
            ("src/d.rs", ""),
            (
                "src/api.rs",
                "use crate::{\n    a::A,\n    b::{self, c::{C, D as E}},\n    d::*,\n};\n",
            ),
        ];
        assert_eq!(
            imports(&files, "src/api.rs"),
            ["src/a.rs", "src/b/c.rs", "src/b/mod.rs", "src/d.rs"]
        );
    }

    #[test]
    fn super_and_self_follow_the_module_tree() {
        let files = [
            ("Cargo.toml", MANIFEST),
            ("src/lib.rs", ""),
            ("src/net/mod.rs", "use self::http::Client;\n"),
            (
                "src/net/http.rs",
                "use super::tls::Tls;\nuse super::super::store::Store;\n",
            ),
            ("src/net/tls.rs", ""),
            ("src/store.rs", ""),
        ];
        assert_eq!(imports(&files, "src/net/mod.rs"), ["src/net/http.rs"]);
        assert_eq!(
            imports(&files, "src/net/http.rs"),
            ["src/net/tls.rs", "src/store.rs"]
        );
    }

    #[test]
    fn inline_test_module_super_adds_no_edge() {
        let files = [
            ("Cargo.toml", MANIFEST),
            ("src/lib.rs", "mod a;\n"),
            ("src/a.rs", "pub fn f() {}\n#[cfg(test)]\nmod tests {\n    use super::*;\n    use super::f;\n    fn t() { super::f(); self::t(); }\n}\n"),
        ];
        assert!(imports(&files, "src/a.rs").is_empty());
    }

    #[test]
    fn inline_module_paths_reach_out_of_line_children() {
        let files = [
            ("Cargo.toml", MANIFEST),
            (
                "src/lib.rs",
                "mod child;\nmod outer {\n    mod deep;\n    fn f() { self::deep::g(); }\n}\n#[cfg(test)]\nmod tests {\n    use super::child::run;\n    use super::helper;\n}\n",
            ),
            ("src/child.rs", ""),
            ("src/outer/deep.rs", ""),
        ];
        assert_eq!(
            imports(&files, "src/lib.rs"),
            ["src/child.rs", "src/outer/deep.rs"]
        );
    }

    #[test]
    fn binary_modules_resolve_crate_to_main_not_the_library() {
        let files = [
            ("Cargo.toml", MANIFEST),
            ("src/lib.rs", "pub mod store;\npub struct Config;\n"),
            ("src/store.rs", "use crate::Config;\n"),
            (
                "src/main.rs",
                "mod cmd;\nconst VERSION: &str = \"1\";\nfn main() { cmd::run(); }\n",
            ),
            ("src/cmd.rs", "use crate::VERSION;\nuse app::store::open;\n"),
        ];
        assert_eq!(
            imports(&files, "src/cmd.rs"),
            ["src/main.rs", "src/store.rs"]
        );
        assert_eq!(imports(&files, "src/store.rs"), ["src/lib.rs"]);
    }

    #[test]
    fn super_beyond_an_inline_module_reaches_the_parent() {
        let files = [
            ("Cargo.toml", MANIFEST),
            ("src/lib.rs", "mod a;\nmod b;\n"),
            ("src/a.rs", "mod tests {\n    use super::super::b::B;\n}\n"),
            ("src/b.rs", ""),
        ];
        assert_eq!(imports(&files, "src/a.rs"), ["src/b.rs"]);
    }

    #[test]
    fn workspace_crates_resolve_by_name() {
        let files = [
            ("Cargo.toml", "[workspace]\nmembers = [\"crates/*\"]\n"),
            ("crates/my-core/Cargo.toml", "[package]\nname = \"my-core\"\n"),
            ("crates/my-core/src/lib.rs", "pub mod store;\n"),
            ("crates/my-core/src/store.rs", ""),
            ("crates/cli/Cargo.toml", "[package]\nname = \"cli\"\n\n[lib]\nname = \"cli_lib\"\npath = \"src/library.rs\"\n"),
            ("crates/cli/src/library.rs", "mod run;\npub use run::go;\n"),
            ("crates/cli/src/run.rs", "use my_core::store::Store;\nfn f() { my_core::init(); serde_json::to_string(&1); }\n"),
            ("crates/cli/tests/it.rs", "mod common;\nuse cli_lib::run::go;\nfn t() { common::setup(); }\n"),
            ("crates/cli/tests/common/mod.rs", ""),
        ];
        assert_eq!(
            imports(&files, "crates/cli/src/run.rs"),
            ["crates/my-core/src/lib.rs", "crates/my-core/src/store.rs"]
        );
        assert_eq!(
            imports(&files, "crates/cli/src/library.rs"),
            ["crates/cli/src/run.rs"]
        );
        assert_eq!(
            imports(&files, "crates/cli/tests/it.rs"),
            ["crates/cli/src/run.rs", "crates/cli/tests/common/mod.rs"]
        );
    }

    #[test]
    fn comments_strings_and_foreign_paths_are_ignored() {
        let files = [
            ("Cargo.toml", MANIFEST),
            ("src/lib.rs", ""),
            ("src/a.rs", ""),
            (
                "src/b.rs",
                "// use crate::a::A;\n/* crate::a::f() */\nconst S: &str = \"use crate::a::A; mod a;\";\nconst R: &str = r#\"crate::a\"#;\nuse std::collections::HashMap;\nfn f<'a>(x: &'a str) -> char { 'c' }\n",
            ),
        ];
        assert!(imports(&files, "src/b.rs").is_empty());
    }

    #[test]
    fn works_without_a_manifest() {
        let files = [
            ("src/main.rs", "mod a;\nfn main() { a::run(); }\n"),
            ("src/a.rs", "use crate::b::B;\n"),
            ("src/b.rs", "use crate::VERSION;\n"),
        ];
        assert_eq!(imports(&files, "src/main.rs"), ["src/a.rs"]);
        assert_eq!(imports(&files, "src/a.rs"), ["src/b.rs"]);
        assert_eq!(imports(&files, "src/b.rs"), ["src/main.rs"]);
    }

    #[test]
    fn expand_use_handles_nested_groups() {
        let mut out = Vec::new();
        expand_use("a::{b, c::{d, self}, e as f}", &[], &mut out);
        let flat: Vec<String> = out.iter().map(|p| p.join("::")).collect();
        assert_eq!(flat, ["a::b", "a::c::d", "a::c", "a::e"]);
    }
}
