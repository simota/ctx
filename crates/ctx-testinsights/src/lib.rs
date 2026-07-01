//! Static Go test and coverage insight, mirroring `internal/testinsights`.
//!
//! `analyze(root, rel_path, profile)` is a 1-to-1 port of Go's
//! `testinsights.Analyze`. Test-function detection uses tree-sitter-go to
//! replicate Go's `go/ast` rules exactly:
//!   - Function name starts with "Test", "Benchmark", or "Fuzz"
//!   - Only top-level `function_declaration` nodes (not method_declaration)
//!
//! Walk logic mirrors `walk.DefaultOptions()`:
//! skips `.git`, `node_modules`, `dist`, `coverage`; follows no symlinks.

use std::collections::HashSet;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tree_sitter::{Node, Parser};

// ── Public types (mirror Go's Insight, TestFile, SourceFile, Coverage, LineRange) ──

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Insight {
    pub path: String,
    pub kind: String,
    pub tests: Vec<TestFile>,
    pub sources: Vec<SourceFile>,
    pub total_tests: i32,
    pub total_sources: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage: Option<Coverage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TestFile {
    pub path: String,
    pub score: i32,
    pub reasons: Vec<String>,
    pub test_count: i32,
    pub matched_symbols: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceFile {
    pub path: String,
    pub score: i32,
    pub reasons: Vec<String>,
    pub matched_symbols: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Coverage {
    pub profile: String,
    pub total_stmts: i32,
    pub covered_stmts: i32,
    pub percent: f64,
    pub uncovered_lines: Vec<LineRange>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LineRange {
    pub start: i32,
    pub end: i32,
}

// ── Internal types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
struct GoFileInfo {
    package: String,
    symbols: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct GoTestInfo {
    package: String,
    test_count: i32,
    matched_symbols: Vec<String>,
}

// ── Public API ──────────────────────────────────────────────────────────────

/// Port of Go's `testinsights.Analyze(root, relPath, profile)`.
pub fn analyze(root: impl AsRef<Path>, rel_path: &str, profile: &str) -> std::io::Result<Insight> {
    let root = root.as_ref();
    let rel_path = clean_slash(rel_path);
    let kind = kind_for_path(&rel_path).to_string();
    let mut out = Insight {
        path: rel_path.clone(),
        kind,
        tests: Vec::new(),
        sources: Vec::new(),
        total_tests: 0,
        total_sources: 0,
        coverage: None,
    };
    if out.kind != "go" {
        return Ok(out);
    }

    let target_info = parse_go_file(&root.join(from_slash(&rel_path)))?;
    if rel_path.ends_with("_test.go") {
        out.sources = related_go_sources(root, &rel_path, &target_info)?;
        out.total_sources = out.sources.len() as i32;
    } else {
        out.tests = related_go_tests(root, &rel_path, &target_info)?;
        out.total_tests = out.tests.len() as i32;
    }
    out.coverage = read_coverage(root, &rel_path, profile)?;
    Ok(out)
}

// ── Core logic ──────────────────────────────────────────────────────────────

fn related_go_sources(
    root: &Path,
    rel_path: &str,
    test_info: &GoFileInfo,
) -> std::io::Result<Vec<SourceFile>> {
    let test_text = fs::read_to_string(root.join(from_slash(rel_path)))?;
    let dir = clean_dir(rel_path);
    let base = Path::new(rel_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .trim_end_matches("_test.go")
        .to_string();

    // Mirrors Go's `conventional` map
    let mut conventional: Vec<(String, &'static str)> = Vec::new();
    conventional.push((
        join_slash(&dir, &format!("{base}.go")),
        "conventional source",
    ));
    if let Some(idx) = base.find('_') {
        if idx > 0 {
            conventional.push((
                join_slash(&dir, &format!("{}.go", &base[..idx])),
                "prefix source",
            ));
        }
    }
    let test_package = test_info
        .package
        .strip_suffix("_test")
        .unwrap_or(&test_info.package);

    let mut out = Vec::new();
    for fi in walk(root)? {
        let p = fi.path;
        if !p.ends_with(".go") || p.ends_with("_test.go") {
            continue;
        }
        let source_info = match parse_go_file(&fi.abs_path) {
            Ok(info) => info,
            Err(_) => continue,
        };
        let mut score = 0i32;
        let mut reasons: Vec<String> = Vec::new();
        if let Some((_, reason)) = conventional.iter().find(|(path, _)| path == &p) {
            score += 50;
            reasons.push((*reason).to_string());
        }
        let source_dir = clean_dir(&p);
        if source_dir == dir {
            score += 20;
            reasons.push("same directory".to_string());
        }
        if source_info.package == test_package {
            score += 15;
            reasons.push("same package".to_string());
        }
        let matched = symbols_mentioned(&test_text, &source_info.symbols);
        if !matched.is_empty() {
            score += std::cmp::min(30, matched.len() as i32 * 5);
            reasons.push("referenced by test".to_string());
        }
        if score == 0 {
            continue;
        }
        out.push(SourceFile {
            path: p,
            score,
            reasons,
            matched_symbols: matched,
        });
    }
    out.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.path.cmp(&b.path)));
    Ok(out)
}

fn related_go_tests(
    root: &Path,
    rel_path: &str,
    target: &GoFileInfo,
) -> std::io::Result<Vec<TestFile>> {
    let dir = clean_dir(rel_path);
    let base = Path::new(rel_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .trim_end_matches(".go");
    let conventional = join_slash(&dir, &format!("{base}_test.go"));

    let mut out = Vec::new();
    for fi in walk(root)? {
        let p = fi.path;
        if !p.ends_with("_test.go") {
            continue;
        }
        let ti = match parse_go_test_file(&fi.abs_path, &target.symbols) {
            Ok(info) => info,
            Err(_) => continue,
        };
        let mut score = 0i32;
        let mut reasons: Vec<String> = Vec::new();
        if p == conventional {
            score += 50;
            reasons.push("conventional filename".to_string());
        }
        let test_dir = clean_dir(&p);
        if test_dir == dir {
            score += 20;
            reasons.push("same directory".to_string());
        }
        if ti.package == target.package || ti.package == format!("{}_test", target.package) {
            score += 15;
            reasons.push("same package".to_string());
        }
        if !ti.matched_symbols.is_empty() {
            score += std::cmp::min(30, ti.matched_symbols.len() as i32 * 5);
            reasons.push("references target symbols".to_string());
        }
        if score == 0 {
            continue;
        }
        out.push(TestFile {
            path: p,
            score,
            reasons,
            test_count: ti.test_count,
            matched_symbols: ti.matched_symbols,
        });
    }
    out.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.path.cmp(&b.path)));
    Ok(out)
}

// ── kindForPath ──────────────────────────────────────────────────────────────

fn kind_for_path(path: &str) -> &'static str {
    if path.to_ascii_lowercase().ends_with(".go") {
        "go"
    } else {
        ""
    }
}

// ── Tree-sitter Go parsing ───────────────────────────────────────────────────

fn parse_go_tree(src: &[u8]) -> std::io::Result<tree_sitter::Tree> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_go::language())
        .map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "go parser unavailable")
        })?;
    parser
        .parse(src, None)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "parse failed"))
}

fn package_name(root: Node, src: &[u8]) -> Option<String> {
    for i in 0..root.named_child_count() {
        let child = root.named_child(i)?;
        if child.kind() == "package_clause" {
            for j in 0..child.named_child_count() {
                let n = child.named_child(j)?;
                if n.kind() == "package_identifier" {
                    return text(n, src);
                }
            }
        }
    }
    None
}

/// Mirrors Go's `parseGoFile`: collects symbols from TOP-LEVEL declarations
/// ONLY, matching `go/ast`'s `for _, decl := range f.Decls` loop:
///   - `ast.FuncDecl`   → function/method name
///   - `ast.GenDecl` of `ast.TypeSpec`  → type name
///   - `ast.GenDecl` of `ast.ValueSpec` → const/var name(s)
///
/// It MUST NOT descend into function bodies, parameter lists, struct fields,
/// or short-var-declarations — those are not in `f.Decls` and Go never
/// surfaces them (e.g. a local/parameter `out *[]Symbol` must NOT appear).
fn parse_go_file(path: &Path) -> std::io::Result<GoFileInfo> {
    let src = fs::read(path)?;
    let tree = parse_go_tree(&src)?;
    let root = tree.root_node();
    let mut info = GoFileInfo {
        package: package_name(root, &src).unwrap_or_default(),
        symbols: Vec::new(),
    };
    let mut seen = HashSet::new();
    // Walk ONLY the source_file's direct children (the top-level declarations),
    // mirroring `f.Decls`. We do NOT recurse into arbitrary subtrees.
    collect_top_level_symbols(root, &src, &mut info.symbols, &mut seen);
    info.symbols.sort();
    Ok(info)
}

/// Iterate the `source_file`'s direct children — the analogue of Go's
/// `f.Decls`. Each child is one of:
///   - function_declaration / method_declaration → take `name` (FuncDecl)
///   - type_declaration   → for each type_spec child, take `name` (TypeSpec)
///   - const_declaration / var_declaration → for each const_spec/var_spec
///     child, take name(s) (ValueSpec, possibly multi-name)
///
/// Anything else at top level (imports, comments, package clause) is ignored.
/// We deliberately do NOT recurse into nested scopes (function bodies, blocks)
/// so that parameters, locals, and struct fields never leak into the symbol
/// set — exactly as `go/ast` over `f.Decls` behaves.
fn collect_top_level_symbols(
    root: Node,
    src: &[u8],
    out: &mut Vec<String>,
    seen: &mut HashSet<String>,
) {
    for i in 0..root.named_child_count() {
        let Some(decl) = root.named_child(i) else {
            continue;
        };
        match decl.kind() {
            // ast.FuncDecl — both free functions and methods carry a `name`.
            "function_declaration" | "method_declaration" => {
                if let Some(name) = decl.child_by_field_name("name").and_then(|n| text(n, src)) {
                    add_symbol(out, seen, &name);
                }
            }
            // ast.GenDecl(TypeSpec) — `type_declaration` wraps one or more
            // `type_spec` nodes. In tree-sitter-go these are DIRECT children
            // for both single (`type Foo …`) and grouped (`type ( … )`) forms.
            // For safety we also descend through a `type_spec_list` wrapper if
            // a grammar revision introduces one.
            "type_declaration" => {
                for_each_spec(decl, "type_spec", "type_spec_list", &mut |spec| {
                    if let Some(name) = spec.child_by_field_name("name").and_then(|n| text(n, src))
                    {
                        add_symbol(out, seen, &name);
                    }
                });
            }
            // ast.GenDecl(ValueSpec) — `const_declaration`/`var_declaration`.
            // CRITICAL grammar asymmetry in tree-sitter-go:
            //   - single  `var X …` / `const K …`  → spec is a DIRECT child.
            //   - grouped  `const ( … )`            → const_spec are DIRECT children.
            //   - grouped  `var ( … )`              → var_spec are wrapped in an
            //     intermediate `var_spec_list` node.
            // So we must descend through `{const,var}_spec_list` wrappers to
            // reach every spec — otherwise grouped `var ( … )` names (e.g.
            // `sharedEncoder`) are silently dropped. Each spec may itself carry
            // multiple names (`a, b = 1, 2`), handled by collect_value_spec_names.
            "const_declaration" => {
                for_each_spec(decl, "const_spec", "const_spec_list", &mut |spec| {
                    collect_value_spec_names(spec, src, out, seen);
                });
            }
            "var_declaration" => {
                for_each_spec(decl, "var_spec", "var_spec_list", &mut |spec| {
                    collect_value_spec_names(spec, src, out, seen);
                });
            }
            _ => {}
        }
    }
}

/// Visit every `spec_kind` node inside a top-level declaration, handling both
/// the DIRECT-child form (single decl, and grouped `const`/`type`) and the
/// LIST-WRAPPED form (grouped `var` → `var_spec_list`). Mirrors `go/ast`'s
/// `GenDecl.Specs` which flattens grouped blocks into a single spec slice.
fn for_each_spec<'a>(
    decl: Node<'a>,
    spec_kind: &str,
    list_kind: &str,
    visit: &mut dyn FnMut(Node<'a>),
) {
    for i in 0..decl.named_child_count() {
        let Some(child) = decl.named_child(i) else {
            continue;
        };
        if child.kind() == spec_kind {
            visit(child);
        } else if child.kind() == list_kind {
            for j in 0..child.named_child_count() {
                if let Some(spec) = child.named_child(j) {
                    if spec.kind() == spec_kind {
                        visit(spec);
                    }
                }
            }
        }
    }
}

/// Extract the declared name(s) from a `const_spec`/`var_spec`, mirroring
/// `ast.ValueSpec.Names`.
///
/// In tree-sitter-go, a spec's declared names are its DIRECT `identifier`
/// children (`var A, B int` → `identifier "A"`, `identifier "B"`). The
/// optional type is a `type_identifier` (NOT an `identifier`) and the
/// initialiser values live inside an `expression_list` (a deeper subtree),
/// so iterating only the spec's direct `identifier` children captures
/// exactly the declared names and nothing else — never a value reference
/// like `var Y = someGlobal`'s `someGlobal`. (The `name` field only points
/// at the FIRST identifier, so a field lookup would drop `B` in `A, B`.)
fn collect_value_spec_names(
    spec: Node,
    src: &[u8],
    out: &mut Vec<String>,
    seen: &mut HashSet<String>,
) {
    for i in 0..spec.named_child_count() {
        if let Some(n) = spec.named_child(i) {
            if n.kind() == "identifier" {
                if let Some(name) = text(n, src) {
                    add_symbol(out, seen, &name);
                }
            }
        }
    }
}

/// Mirrors Go's `parseGoTestFile`: counts test functions + finds matched symbols.
/// Go's `isGoTestFunc` checks: HasPrefix("Test") | HasPrefix("Benchmark") | HasPrefix("Fuzz").
/// Only top-level `function_declaration` nodes are test functions (not methods).
fn parse_go_test_file(path: &Path, target_symbols: &[String]) -> std::io::Result<GoTestInfo> {
    let src = fs::read(path)?;
    let tree = parse_go_tree(&src)?;
    let root = tree.root_node();
    let mut info = GoTestInfo {
        package: package_name(root, &src).unwrap_or_default(),
        test_count: 0,
        matched_symbols: symbols_mentioned(&String::from_utf8_lossy(&src), target_symbols),
    };
    count_test_funcs(root, &src, &mut info.test_count);
    Ok(info)
}

/// Count top-level Go test functions matching `isGoTestFunc`.
/// Only `function_declaration` at source_file level (not method_declaration).
fn count_test_funcs(node: Node, src: &[u8], count: &mut i32) {
    // Only walk top-level: the source_file's direct children
    if node.kind() == "source_file" {
        for i in 0..node.named_child_count() {
            if let Some(child) = node.named_child(i) {
                if child.kind() == "function_declaration" {
                    if let Some(name) = child.child_by_field_name("name").and_then(|n| text(n, src))
                    {
                        if is_go_test_func(&name) {
                            *count += 1;
                        }
                    }
                }
            }
        }
    } else {
        // Recurse in case source_file is nested (shouldn't be, but be safe)
        for i in 0..node.named_child_count() {
            if let Some(child) = node.named_child(i) {
                count_test_funcs(child, src, count);
            }
        }
    }
}

/// Port of Go's `isGoTestFunc`: Test | Benchmark | Fuzz prefix.
fn is_go_test_func(name: &str) -> bool {
    name.starts_with("Test") || name.starts_with("Benchmark") || name.starts_with("Fuzz")
}

fn add_symbol(dst: &mut Vec<String>, seen: &mut HashSet<String>, name: &str) {
    if name.is_empty() || name == "_" || !seen.insert(name.to_string()) {
        return;
    }
    dst.push(name.to_string());
}

fn text(node: Node, src: &[u8]) -> Option<String> {
    node.utf8_text(src).ok().map(|s| s.to_string())
}

// ── symbolsMentioned / containsIdentifier ────────────────────────────────────

fn symbols_mentioned(text: &str, symbols: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for sym in symbols {
        if !sym.is_empty() && contains_identifier(text, sym) {
            out.push(sym.clone());
        }
    }
    out.sort();
    out
}

fn contains_identifier(text: &str, ident: &str) -> bool {
    if ident.is_empty() {
        return false;
    }
    let mut start = 0usize;
    while let Some(idx) = text[start..].find(ident) {
        let pos = start + idx;
        let after = pos + ident.len();
        let before_ok = pos == 0 || !is_ident_byte(text.as_bytes()[pos - 1]);
        let after_ok = after >= text.len() || !is_ident_byte(text.as_bytes()[after]);
        if before_ok && after_ok {
            return true;
        }
        start = after;
    }
    false
}

fn is_ident_byte(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphanumeric()
}

// ── Coverage parsing ─────────────────────────────────────────────────────────

fn read_coverage(root: &Path, rel_path: &str, profile: &str) -> std::io::Result<Option<Coverage>> {
    let profile = if profile.is_empty() {
        "coverage.out"
    } else {
        profile
    };
    let profile = clean_slash(profile);
    if Path::new(&profile).is_absolute() || profile.starts_with("../") || profile == ".." {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "coverage profile must be repo-relative",
        ));
    }
    let file = match fs::File::open(root.join(from_slash(&profile))) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };

    let mut cov = Coverage {
        profile,
        total_stmts: 0,
        covered_stmts: 0,
        percent: 0.0,
        uncovered_lines: Vec::new(),
    };
    for line in BufReader::new(file).lines() {
        let line = line?;
        let line = line.trim().to_string();
        if line.is_empty() || line.starts_with("mode:") {
            continue;
        }
        let Some(entry) = parse_cover_line(&line) else {
            continue;
        };
        if !cover_path_matches(&entry.path, rel_path) {
            continue;
        }
        cov.total_stmts += entry.stmts;
        if entry.count > 0 {
            cov.covered_stmts += entry.stmts;
        } else {
            cov.uncovered_lines.push(LineRange {
                start: entry.start_line,
                end: entry.end_line,
            });
        }
    }
    if cov.total_stmts == 0 {
        return Ok(None);
    }
    cov.percent = cov.covered_stmts as f64 * 100.0 / cov.total_stmts as f64;
    cov.uncovered_lines = merge_ranges(cov.uncovered_lines);
    Ok(Some(cov))
}

struct CoverEntry {
    path: String,
    start_line: i32,
    end_line: i32,
    stmts: i32,
    count: i32,
}

fn parse_cover_line(line: &str) -> Option<CoverEntry> {
    let colon = line.rfind(':')?;
    if colon == 0 {
        return None;
    }
    let path = clean_slash(&line[..colon]);
    let fields: Vec<&str> = line[colon + 1..].split_whitespace().collect();
    if fields.len() != 3 {
        return None;
    }
    let rng: Vec<&str> = fields[0].split(',').collect();
    if rng.len() != 2 {
        return None;
    }
    Some(CoverEntry {
        path,
        start_line: parse_cover_line_num(rng[0])?,
        end_line: parse_cover_line_num(rng[1])?,
        stmts: fields[1].parse().ok()?,
        count: fields[2].parse().ok()?,
    })
}

fn parse_cover_line_num(s: &str) -> Option<i32> {
    let dot = s.find('.')?;
    if dot == 0 {
        return None;
    }
    s[..dot].parse().ok()
}

fn cover_path_matches(profile_path: &str, rel_path: &str) -> bool {
    let profile_path = clean_slash(profile_path);
    let rel_path = clean_slash(rel_path);
    profile_path == rel_path || profile_path.ends_with(&format!("/{rel_path}"))
}

fn merge_ranges(mut ranges: Vec<LineRange>) -> Vec<LineRange> {
    if ranges.len() < 2 {
        return ranges;
    }
    ranges.sort_by(|a, b| a.start.cmp(&b.start).then_with(|| a.end.cmp(&b.end)));
    let mut out = vec![ranges[0].clone()];
    for r in ranges.into_iter().skip(1) {
        let last = out.last_mut().expect("at least one");
        if r.start <= last.end + 1 {
            if r.end > last.end {
                last.end = r.end;
            }
        } else {
            out.push(r);
        }
    }
    out
}

// ── File walker (mirrors walk.DefaultOptions) ────────────────────────────────

struct WalkedFile {
    path: String,
    abs_path: PathBuf,
}

/// Walk `root` yielding all non-directory non-symlink files, sorted by name at
/// each level. Skips `.git`, `node_modules`, `dist`, `coverage` (mirrors Go's
/// `walk.DefaultOptions()` ExtraIgnore list).
fn walk(root: &Path) -> std::io::Result<Vec<WalkedFile>> {
    let abs_root = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let mut out = Vec::new();
    visit_dir(&abs_root, &abs_root, &mut out)?;
    Ok(out)
}

fn visit_dir(root: &Path, dir: &Path, out: &mut Vec<WalkedFile>) -> std::io::Result<()> {
    let mut children: Vec<_> = match fs::read_dir(dir) {
        Ok(entries) => entries.filter_map(Result::ok).collect(),
        Err(_) => return Ok(()),
    };
    children.sort_by_key(|e| e.file_name());
    for entry in children {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        // Mirror DefaultOptions ExtraIgnore + always skip .git
        if matches!(
            name_str.as_ref(),
            ".git" | "node_modules" | "dist" | "coverage"
        ) {
            continue;
        }
        let path = entry.path();
        let ty = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if ty.is_symlink() {
            continue;
        }
        if ty.is_dir() {
            visit_dir(root, &path, out)?;
        } else {
            out.push(WalkedFile {
                path: relativise(root, &path),
                abs_path: path,
            });
        }
    }
    Ok(())
}

fn relativise(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    if rel.as_os_str().is_empty() {
        return ".".to_string();
    }
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

// ── Path utilities ───────────────────────────────────────────────────────────

fn clean_dir(path: &str) -> String {
    let p = Path::new(path);
    match p.parent().and_then(|p| p.to_str()) {
        Some("") | Some(".") | None => String::new(),
        Some(s) => s.replace('\\', "/"),
    }
}

fn join_slash(dir: &str, base: &str) -> String {
    if dir.is_empty() {
        base.to_string()
    } else {
        format!("{dir}/{base}")
    }
}

/// Port of Go's `filepath.ToSlash(filepath.Clean(path))`.
fn clean_slash(path: &str) -> String {
    let path = path.replace('\\', "/");
    let mut out: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if out.is_empty() || out.last() == Some(&"..") {
                    out.push("..");
                } else {
                    out.pop();
                }
            }
            s => out.push(s),
        }
    }
    if out.is_empty() {
        ".".to_string()
    } else {
        out.join("/")
    }
}

fn from_slash(path: &str) -> PathBuf {
    path.split('/').collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Top-level-only symbol extraction: function parameters, locals, and
    /// short-var-decls must NOT leak into the symbol set. Mirrors go/ast's
    /// `f.Decls` iteration. Regression guard for the `out` over-extraction
    /// bug (a `walkNode(..., out *[]Symbol, ...)` parameter was leaking).
    #[test]
    fn extracts_only_top_level_declarations() {
        let src = br#"
package demo

import "fmt"

const maxParseBytes = 500 * 1024

type Extractor interface{}

type langSpec struct {
	kinds map[string]int
}

var langSpecs = map[string]int{}

var GroupedA, GroupedB = 1, 2

func New() Extractor { return nil }

func walkNode(n int, out *[]int, seen map[string]bool) {
	var local int
	shadowed := 7
	for i := 0; i < n; i++ {
		out = append(out, i)
	}
	_ = local
	_ = shadowed
}

func (Extractor) Extract(path string) error {
	var out []int
	_ = out
	return nil
}
"#;
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_go::language()).unwrap();
        let tree = parser.parse(&src[..], None).unwrap();
        let mut symbols = Vec::new();
        let mut seen = HashSet::new();
        collect_top_level_symbols(tree.root_node(), &src[..], &mut symbols, &mut seen);
        symbols.sort();

        let expected = vec![
            "Extract",
            "Extractor",
            "GroupedA",
            "GroupedB",
            "New",
            "langSpec",
            "langSpecs",
            "maxParseBytes",
            "walkNode",
        ];
        assert_eq!(symbols, expected);
        // The bug being locked: a parameter/local named `out`, `local`,
        // `shadowed`, `path`, `n`, `seen`, `i` must NEVER appear.
        for leaked in ["out", "local", "shadowed", "path", "n", "seen", "i"] {
            assert!(
                !symbols.contains(&leaked.to_string()),
                "leaked non-top-level identifier: {leaked}"
            );
        }
    }

    /// Grouped declaration blocks: `var ( … )` (wrapped in `var_spec_list`),
    /// `const ( … )`, and `type ( … )` must surface EVERY name. Regression
    /// guard for the under-extraction where grouped `var (` names (e.g.
    /// `sharedEncoder` in internal/tokens/counter.go) were silently dropped
    /// because `var_spec` nodes are nested under `var_spec_list`, not direct
    /// children of `var_declaration`.
    #[test]
    fn extracts_grouped_declaration_blocks() {
        let src = br#"
package demo

import "sync"

var (
	sharedEncoderOnce sync.Once
	sharedEncoder     *int
	sharedEncoderErr  error
)

const (
	GA = 1
	GB, GC = 2, 3
)

type (
	GroupedType1 int
	GroupedType2 struct{}
)

func getShared() (*int, error) {
	sharedEncoderOnce.Do(func() {})
	return sharedEncoder, sharedEncoderErr
}
"#;
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_go::language()).unwrap();
        let tree = parser.parse(&src[..], None).unwrap();
        let mut symbols = Vec::new();
        let mut seen = HashSet::new();
        collect_top_level_symbols(tree.root_node(), &src[..], &mut symbols, &mut seen);
        symbols.sort();

        let expected = vec![
            "GA",
            "GB",
            "GC",
            "GroupedType1",
            "GroupedType2",
            "getShared",
            "sharedEncoder",
            "sharedEncoderErr",
            "sharedEncoderOnce",
        ];
        assert_eq!(symbols, expected);
    }

    /// Test-function detection mirrors `isGoTestFunc`: Test/Benchmark/Fuzz
    /// prefix on TOP-LEVEL function_declaration only (not methods).
    #[test]
    fn counts_only_top_level_test_funcs() {
        let src = br#"
package demo

import "testing"

func TestOne(t *testing.T)        {}
func BenchmarkTwo(b *testing.B)   {}
func FuzzThree(f *testing.F)      {}
func ExampleFour()                {}
func helper()                     {}
func (s S) TestMethod()           {}
"#;
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_go::language()).unwrap();
        let tree = parser.parse(&src[..], None).unwrap();
        let mut count = 0;
        count_test_funcs(tree.root_node(), &src[..], &mut count);
        // TestOne, BenchmarkTwo, FuzzThree = 3. Example/helper/method excluded.
        assert_eq!(count, 3);
    }
}
