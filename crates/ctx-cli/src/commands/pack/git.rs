use std::path::Path;
use std::process::Command;

use super::*;

pub(crate) fn git_changed_paths(root: &Path) -> Result<std::collections::BTreeSet<String>, String> {
    let output = Command::new("git")
        .args(["-C", &root.to_string_lossy(), "status", "--porcelain"])
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
    let mut changed = std::collections::BTreeSet::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if line.len() < 4 {
            continue;
        }
        let path = line[3..].trim();
        let path = path.split(" -> ").last().unwrap_or(path);
        if !path.is_empty() {
            changed.insert(path.replace('\\', "/"));
        }
    }
    Ok(changed)
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
        let fields: Vec<&str> = line.split_whitespace().collect();
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
