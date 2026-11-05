// crates/ctx-pack/src/selection.rs
//
// Pack an explicit, ordered file selection (a pinned-files board, a saved
// mix, an agent's chosen `ctx_where` hits). Unlike the directory planner the
// goal does not filter or reorder: the caller already chose the files and
// their order, so the only cut is the token budget.
//
// Reads are confined to `root`: absolute paths, `..` components, symlinks
// resolving outside the root and non-regular files are reported as skipped,
// never read. Contents are secret-redacted with the default `[security]`
// settings, as every other non-CLI pack path does.

use std::path::Path;

use crate::assemble::{estimate_text_tokens, render, PackFile, RenderOptions};

#[derive(Debug, Clone)]
pub struct SelectionOptions {
    pub goal: String,
    /// Token budget; 0 means unlimited.
    pub budget: i64,
    /// `markdown` (default) or `plain` (file contents with `=== path ===`
    /// separators).
    pub format: String,
    pub contract: bool,
}

impl Default for SelectionOptions {
    fn default() -> Self {
        SelectionOptions {
            goal: String::new(),
            budget: 0,
            format: "markdown".to_string(),
            contract: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedFile {
    pub path: String,
    pub tokens: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedFile {
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct SelectionPack {
    pub content: String,
    pub used: i64,
    pub budget: i64,
    pub included: Vec<SelectedFile>,
    pub skipped: Vec<SkippedFile>,
}

/// Clean a repository-relative path; `None` for absolute paths, `..`
/// components or an empty path.
fn clean_relative(raw: &str) -> Option<String> {
    let unified = raw.trim().replace('\\', "/");
    let bytes = unified.as_bytes();
    if unified.starts_with('/') || (bytes.len() >= 2 && bytes[1] == b':') {
        return None;
    }
    let mut parts = Vec::new();
    for part in unified.split('/') {
        match part {
            "" | "." => {}
            ".." => return None,
            other => parts.push(other),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// Pack `paths` (relative to `root`) in the given order.
pub fn pack_selection(
    root: &Path,
    paths: &[String],
    opts: &SelectionOptions,
) -> Result<SelectionPack, String> {
    let format = match opts.format.as_str() {
        "" | "markdown" | "md" => "markdown",
        "plain" | "text" | "txt" => "plain",
        other => return Err(format!("unsupported format {other:?} (markdown or plain)")),
    };
    let canonical_root =
        std::fs::canonicalize(root).map_err(|err| format!("root {}: {err}", root.display()))?;
    let mut seen = std::collections::HashSet::new();
    let mut files = Vec::new();
    let mut skipped = Vec::new();
    let mut used = 0_i64;
    for raw in paths {
        let Some(rel) = clean_relative(raw) else {
            skipped.push(SkippedFile {
                path: raw.clone(),
                reason: "outside the repository".to_string(),
            });
            continue;
        };
        if !seen.insert(rel.clone()) {
            continue;
        }
        let abs = canonical_root.join(&rel);
        let target = match std::fs::canonicalize(&abs) {
            Ok(target) => target,
            Err(_) => {
                skipped.push(SkippedFile {
                    path: rel,
                    reason: "not found".to_string(),
                });
                continue;
            }
        };
        if !target.starts_with(&canonical_root) {
            skipped.push(SkippedFile {
                path: rel,
                reason: "links outside the repository".to_string(),
            });
            continue;
        }
        if !std::fs::metadata(&target).is_ok_and(|meta| meta.is_file()) {
            skipped.push(SkippedFile {
                path: rel,
                reason: "not a regular file".to_string(),
            });
            continue;
        }
        let bytes = match std::fs::read(&target) {
            Ok(bytes) => bytes,
            Err(err) => {
                skipped.push(SkippedFile {
                    path: rel,
                    reason: format!("unreadable: {err}"),
                });
                continue;
            }
        };
        if bytes.iter().take(8000).any(|b| *b == 0) {
            skipped.push(SkippedFile {
                path: rel,
                reason: "binary".to_string(),
            });
            continue;
        }
        let Ok(text) = String::from_utf8(bytes) else {
            skipped.push(SkippedFile {
                path: rel,
                reason: "not UTF-8 text".to_string(),
            });
            continue;
        };
        let abs_str = abs.to_string_lossy().into_owned();
        let content = crate::redact::redact_secrets(&abs_str, &text, &Default::default());
        let tokens = estimate_text_tokens(&content);
        if opts.budget > 0 && used + tokens > opts.budget {
            skipped.push(SkippedFile {
                path: rel,
                reason: format!("budget exceeded ({tokens} tokens)"),
            });
            continue;
        }
        used += tokens;
        files.push(PackFile {
            path: rel,
            abs_path: abs_str,
            content,
            tokens,
            score: 0,
            relevance: "selected".to_string(),
            reason: "selected".to_string(),
            symbols: Vec::new(),
        });
    }
    let render_opts = RenderOptions {
        format: format.to_string(),
        goal: opts.goal.clone(),
        budget: opts.budget,
        plain_file_contents: format == "plain",
        contract: opts.contract,
        ..RenderOptions::default()
    };
    let content = render(&render_opts, &files, None)?;
    Ok(SelectionPack {
        content,
        used,
        budget: opts.budget,
        included: files
            .iter()
            .map(|file| SelectedFile {
                path: file.path.clone(),
                tokens: file.tokens,
            })
            .collect(),
        skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Tmp(std::path::PathBuf);
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn tmp(name: &str) -> Tmp {
        let dir = std::env::temp_dir().join(format!("ctx-pack-sel-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Tmp(dir)
    }

    #[test]
    fn packs_in_the_given_order_and_reports_skips() {
        let dir = tmp("order");
        let root = &dir.0;
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/b.rs"), "fn b() {}\n").unwrap();
        std::fs::write(root.join("a.md"), "# a\n").unwrap();
        std::fs::write(root.join("bin.dat"), [0u8, 1, 2]).unwrap();
        let paths = vec![
            "src/b.rs".to_string(),
            "./a.md".to_string(),
            "src/b.rs".to_string(),
            "../etc/passwd".to_string(),
            "missing.rs".to_string(),
            "bin.dat".to_string(),
            "src".to_string(),
        ];
        let pack = pack_selection(root, &paths, &SelectionOptions::default()).unwrap();
        let included: Vec<&str> = pack.included.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(included, vec!["src/b.rs", "a.md"]);
        let b = pack.content.find("### src/b.rs").unwrap();
        let a = pack.content.find("### a.md").unwrap();
        assert!(b < a, "selection order is kept");
        let reasons: Vec<(&str, &str)> = pack
            .skipped
            .iter()
            .map(|s| (s.path.as_str(), s.reason.as_str()))
            .collect();
        assert_eq!(
            reasons,
            vec![
                ("../etc/passwd", "outside the repository"),
                ("missing.rs", "not found"),
                ("bin.dat", "binary"),
                ("src", "not a regular file"),
            ]
        );
        assert!(pack.content.contains("ctx:contract") || pack.content.contains("CTX-CONTRACT"));
    }

    #[test]
    fn budget_cuts_later_files_and_secrets_are_redacted() {
        let dir = tmp("budget");
        let root = &dir.0;
        std::fs::write(root.join("keys.env"), "AWS_KEY=AKIAIOSFODNN7EXAMPLE\n").unwrap();
        std::fs::write(root.join("big.txt"), "word ".repeat(2000)).unwrap();
        let opts = SelectionOptions {
            budget: 100,
            format: "plain".to_string(),
            contract: false,
            ..SelectionOptions::default()
        };
        let pack = pack_selection(
            root,
            &["keys.env".to_string(), "big.txt".to_string()],
            &opts,
        )
        .unwrap();
        assert_eq!(pack.included.len(), 1);
        assert!(pack.skipped[0].reason.starts_with("budget exceeded"));
        assert!(pack.content.contains("=== keys.env ==="));
        assert!(
            !pack.content.contains("AKIAIOSFODNN7EXAMPLE"),
            "{}",
            pack.content
        );
    }

    #[cfg(unix)]
    #[test]
    fn links_leaving_the_root_are_not_read() {
        let dir = tmp("links");
        let root = dir.0.join("repo");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(dir.0.join("secret.txt"), "top secret\n").unwrap();
        std::os::unix::fs::symlink(dir.0.join("secret.txt"), root.join("notes.txt")).unwrap();
        let pack = pack_selection(
            &root,
            &["notes.txt".to_string()],
            &SelectionOptions::default(),
        )
        .unwrap();
        assert!(pack.included.is_empty());
        assert_eq!(pack.skipped[0].reason, "links outside the repository");
        assert!(!pack.content.contains("top secret"));
    }

    #[test]
    fn rejects_unknown_formats() {
        let dir = tmp("fmt");
        let opts = SelectionOptions {
            format: "xml".to_string(),
            ..SelectionOptions::default()
        };
        assert!(pack_selection(&dir.0, &[], &opts).is_err());
    }
}
