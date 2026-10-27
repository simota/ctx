use std::path::Path;
use std::process::Command;

use super::*;
use crate::common::SecurityToml;

pub(crate) fn git_changed_paths(root: &Path) -> Result<std::collections::BTreeSet<String>, String> {
    let prefix = match git_root_prefix(root) {
        Ok(prefix) => prefix,
        Err(err) => {
            eprintln!("warning: git status unavailable: {err}");
            return Ok(std::collections::BTreeSet::new());
        }
    };
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["status", "--porcelain=v1", "-z", "--untracked-files=all"])
        .output();
    let output = match output {
        Ok(output) if output.status.success() => output,
        Ok(output) => {
            eprintln!(
                "warning: git status unavailable: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
            return Ok(std::collections::BTreeSet::new());
        }
        Err(err) => {
            eprintln!("warning: git status unavailable: {err}");
            return Ok(std::collections::BTreeSet::new());
        }
    };
    Ok(parse_git_changed_paths(&output.stdout, &prefix))
}

/// `root`'s directory inside its git work tree with a trailing `/`, or `""`
/// at the top level. Porcelain status and `git log` name paths from the top
/// of the work tree, while pack paths are relative to `root`.
pub(crate) fn git_root_prefix(root: &Path) -> Result<String, String> {
    git_output_in(root, &["rev-parse", "--show-prefix"])
}

/// Re-express a work-tree-relative git path relative to the pack root, or
/// `None` when it lies outside the root.
pub(crate) fn strip_git_root_prefix(path: &str, prefix: &str) -> Option<String> {
    path.strip_prefix(prefix)
        .filter(|rel| !rel.is_empty())
        .map(str::to_string)
}

fn parse_git_changed_paths(output: &[u8], prefix: &str) -> std::collections::BTreeSet<String> {
    let mut changed = std::collections::BTreeSet::new();
    let mut fields = output.split(|byte| *byte == 0);

    while let Some(record) = fields.next() {
        if record.is_empty() {
            continue;
        }
        if record.len() < 4 || record[2] != b' ' {
            continue;
        }

        let status = &record[..2];
        // The pack walker also uses to_string_lossy for filesystem paths,
        // so applying the same conversion keeps non-UTF-8 names comparable.
        let path = String::from_utf8_lossy(&record[3..]);
        if let Some(path) = strip_git_root_prefix(&path, prefix) {
            changed.insert(path);
        }

        // In porcelain v1 -z, rename/copy records put the destination path in
        // the main record and the source path in the following NUL field.
        if status.iter().any(|byte| matches!(*byte, b'R' | b'C')) {
            let _ = fields.next();
        }
    }

    changed
}

pub(crate) fn git_diff_entries(
    root: &Path,
    revspec: &str,
    api_only: bool,
    security: &SecurityToml,
) -> Result<Vec<ctx_pack::DiffEntry>, String> {
    let (base, head) = parse_diff_revspec(revspec)?;
    let before_commit = git_output_in(root, &["rev-parse", "--short=7", base])?;
    let after_commit = git_output_in(root, &["rev-parse", "--short=7", head])?;
    let name_status = git_output_bytes_in(
        root,
        &git_diff_args(&["--name-status", "-z", base, head, "--"]),
    )?;
    let mut entries = Vec::new();
    for (status, path, before_path) in parse_git_name_status_z(&name_status) {
        let added = status.starts_with('A');
        let deleted = status.starts_with('D');
        let binary = git_diff_is_binary(root, base, head, &path)?;
        let mut patch = git_output_allow_empty(root, &git_diff_args(&[base, head, "--", &path]))?;
        let mut before_content = if added || binary {
            String::new()
        } else {
            git_show_file(root, base, &before_path).unwrap_or_default()
        };
        let mut after_content = if deleted || binary {
            String::new()
        } else {
            git_show_file(root, head, &path).unwrap_or_default()
        };
        if api_only {
            before_content =
                extract_public_api_light(&path, &before_content).unwrap_or(before_content);
            after_content =
                extract_public_api_light(&path, &after_content).unwrap_or(after_content);
        }
        // Committed secrets show up in both blobs and in the patch. The
        // path stands in for allowlist_files matching (there is no file).
        let label = root.join(&path).to_string_lossy().into_owned();
        redact_pack_body(&label, &mut before_content, security);
        redact_pack_body(&label, &mut after_content, security);
        redact_pack_body(&label, &mut patch, security);
        entries.push(ctx_pack::DiffEntry {
            path,
            before_content,
            after_content,
            before_commit: before_commit.clone(),
            after_commit: after_commit.clone(),
            patch,
            added,
            deleted,
            binary,
        });
    }
    Ok(entries)
}

fn parse_git_name_status_z(output: &[u8]) -> Vec<(String, String, String)> {
    let mut fields = output
        .split(|byte| *byte == 0)
        .filter(|field| !field.is_empty());
    let mut entries = Vec::new();

    while let Some(status_bytes) = fields.next() {
        let status = String::from_utf8_lossy(status_bytes).into_owned();
        let Some(before_bytes) = fields.next() else {
            break;
        };
        let before = String::from_utf8_lossy(before_bytes).into_owned();

        if matches!(status.as_bytes().first(), Some(b'R' | b'C')) {
            let Some(after_bytes) = fields.next() else {
                break;
            };
            let after = String::from_utf8_lossy(after_bytes).into_owned();
            entries.push((status, after, before));
        } else {
            entries.push((status, before.clone(), before));
        }
    }

    entries
}

/// `git diff` arguments for `--diff` packs. `--relative` limits the diff to
/// the pack root and names paths from it, so the per-path pathspecs (which
/// git resolves against `-C root`) match. The other flags keep user config
/// (`diff.external`, textconv drivers, `color.ui=always`) out of the bundle.
fn git_diff_args<'a>(args: &[&'a str]) -> Vec<&'a str> {
    let mut out = vec![
        "diff",
        "--relative",
        "--no-ext-diff",
        "--no-textconv",
        "--no-color",
    ];
    out.extend_from_slice(args);
    out
}

pub(crate) fn git_output_bytes_in(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|err| format!("git {}: {err}", args.join(" ")))?;
    if !output.status.success() {
        return Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output.stdout)
}

pub(crate) fn parse_diff_revspec(revspec: &str) -> Result<(&str, &str), String> {
    let Some((base, head)) = revspec.split_once("..") else {
        return Err("diff revspec must be BASE..HEAD".to_string());
    };
    if base.is_empty() || head.is_empty() || head.contains("..") {
        return Err("diff revspec must be BASE..HEAD".to_string());
    }
    Ok((base, head))
}

pub(crate) fn git_diff_is_binary(
    root: &Path,
    base: &str,
    head: &str,
    path: &str,
) -> Result<bool, String> {
    let out = git_output_allow_empty(root, &git_diff_args(&["--numstat", base, head, "--", path]))?;
    Ok(out
        .lines()
        .next()
        .map(|line| line.starts_with("-\t-"))
        .unwrap_or(false))
}

pub(crate) fn git_show_file(root: &Path, rev: &str, path: &str) -> Result<String, String> {
    // `REV:./path` resolves against `-C root`, like the `--relative` diff paths.
    git_output_in(root, &["show", &format!("{rev}:./{path}")])
}

pub(crate) fn git_output_in(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|err| format!("git {}: {err}", args.join(" ")))?;
    if !output.status.success() {
        return Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .trim_end_matches('\n')
        .to_string())
}

pub(crate) fn git_output_allow_empty(root: &Path, args: &[&str]) -> Result<String, String> {
    match git_output_in(root, args) {
        Ok(out) => Ok(out),
        Err(err) if err.contains("exists on disk, but not in") => Ok(String::new()),
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_status_z_parser_preserves_special_paths_and_copy_rename_pairs() {
        let raw = b"M\0line\nbreak.rs\0R100\0old name.rs\0new -> name.rs\0C075\0source.rs\0copy\\name.rs\0";
        let entries = parse_git_name_status_z(raw);

        assert_eq!(
            entries,
            vec![
                (
                    "M".to_string(),
                    "line\nbreak.rs".to_string(),
                    "line\nbreak.rs".to_string(),
                ),
                (
                    "R100".to_string(),
                    "new -> name.rs".to_string(),
                    "old name.rs".to_string(),
                ),
                (
                    "C075".to_string(),
                    r"copy\name.rs".to_string(),
                    "source.rs".to_string(),
                ),
            ]
        );
    }

    #[test]
    fn porcelain_z_parser_preserves_special_paths_and_rename_target() {
        let raw = b"?? dir/a b.rs\0 M literal\\name.rs\0R  dst -> literal.rs\0src old.rs\0?? comma,name.rs\0";
        let paths = parse_git_changed_paths(raw, "");

        assert!(paths.contains("dir/a b.rs"));
        assert!(paths.contains(r"literal\name.rs"));
        assert!(paths.contains("dst -> literal.rs"));
        assert!(paths.contains("comma,name.rs"));
        assert!(!paths.contains("src old.rs"));
    }

    #[test]
    fn porcelain_z_parser_names_paths_from_a_subdirectory_root() {
        let raw = b" M sub/a.rs\0 M top.rs\0?? sub/new dir/b.rs\0?? subway.rs\0";
        let paths = parse_git_changed_paths(raw, "sub/");

        assert_eq!(
            paths.into_iter().collect::<Vec<_>>(),
            vec!["a.rs".to_string(), "new dir/b.rs".to_string()]
        );
    }
}
