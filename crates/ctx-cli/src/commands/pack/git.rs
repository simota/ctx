use std::path::Path;
use std::process::Command;

use super::*;

pub(crate) fn git_changed_paths(root: &Path) -> Result<std::collections::BTreeSet<String>, String> {
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
    Ok(parse_git_changed_paths(&output.stdout))
}

fn parse_git_changed_paths(output: &[u8]) -> std::collections::BTreeSet<String> {
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
        let path = &record[3..];
        if !path.is_empty() {
            // The pack walker also uses to_string_lossy for filesystem paths,
            // so applying the same conversion keeps non-UTF-8 names comparable.
            changed.insert(String::from_utf8_lossy(path).into_owned());
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
) -> Result<Vec<ctx_pack::DiffEntry>, String> {
    let (base, head) = parse_diff_revspec(revspec)?;
    let before_commit = git_output_in(root, &["rev-parse", "--short=7", base])?;
    let after_commit = git_output_in(root, &["rev-parse", "--short=7", head])?;
    let name_status = git_output_in(root, &["diff", "--name-status", base, head])?;
    let mut entries = Vec::new();
    for line in name_status.lines() {
        // --name-status output is tab-separated; paths may contain spaces.
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 2 {
            continue;
        }
        let status = fields[0];
        let (path, before_path) = if status.starts_with('R') && fields.len() >= 3 {
            (fields[2], fields[1])
        } else {
            (fields[1], fields[1])
        };
        let added = status.starts_with('A');
        let deleted = status.starts_with('D');
        let binary = git_diff_is_binary(root, base, head, path)?;
        let patch = git_output_allow_empty(root, &["diff", base, head, "--", path])?;
        let mut before_content = if added || binary {
            String::new()
        } else {
            git_show_file(root, base, before_path).unwrap_or_default()
        };
        let mut after_content = if deleted || binary {
            String::new()
        } else {
            git_show_file(root, head, path).unwrap_or_default()
        };
        if api_only {
            before_content =
                extract_public_api_light(path, &before_content).unwrap_or(before_content);
            after_content = extract_public_api_light(path, &after_content).unwrap_or(after_content);
        }
        entries.push(ctx_pack::DiffEntry {
            path: path.to_string(),
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
    let out = git_output_allow_empty(root, &["diff", "--numstat", base, head, "--", path])?;
    Ok(out
        .lines()
        .next()
        .map(|line| line.starts_with("-\t-"))
        .unwrap_or(false))
}

pub(crate) fn git_show_file(root: &Path, rev: &str, path: &str) -> Result<String, String> {
    git_output_in(root, &["show", &format!("{rev}:{path}")])
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
    fn porcelain_z_parser_preserves_special_paths_and_rename_target() {
        let raw = b"?? dir/a b.rs\0 M literal\\name.rs\0R  dst -> literal.rs\0src old.rs\0?? comma,name.rs\0";
        let paths = parse_git_changed_paths(raw);

        assert!(paths.contains("dir/a b.rs"));
        assert!(paths.contains(r"literal\name.rs"));
        assert!(paths.contains("dst -> literal.rs"));
        assert!(paths.contains("comma,name.rs"));
        assert!(!paths.contains("src old.rs"));
    }
}
