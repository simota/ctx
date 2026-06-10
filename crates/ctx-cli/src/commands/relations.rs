use std::env;
use std::ffi::{OsStr, OsString};
use std::io::{self};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::common::*;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub(crate) struct RelationList {
    path: String,
    kind: &'static str,
    items: Vec<String>,
}

#[derive(Debug)]
pub(crate) struct RelationArgs {
    kind: &'static str,
    path: OsString,
    format: String,
}

pub(crate) fn run_relations_command(args: &[OsString]) -> Option<ExitCode> {
    let parsed = parse_relation_args(args)?;
    match relation_command_root_and_path(&parsed.path)
        .and_then(|(root, rel)| relation_items(&root, &rel, parsed.kind).map(|items| (rel, items)))
        .and_then(|(rel, items)| {
            render_relation_list(
                &RelationList {
                    path: rel,
                    kind: parsed.kind,
                    items,
                },
                &parsed.format,
            )
        }) {
        Ok(()) => Some(ExitCode::SUCCESS),
        Err(err) => {
            eprintln!("Error: {err}");
            Some(ExitCode::from(1))
        }
    }
}

pub(crate) fn parse_relation_args(args: &[OsString]) -> Option<RelationArgs> {
    let mut json = false;
    let mut format = "text".to_string();
    let mut command: Option<&'static str> = None;
    let mut positionals = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == OsStr::new("--json") {
            json = true;
        } else if let Some(value) = flag_value(arg, "--format") {
            format = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--format") {
            i += 1;
            format = args.get(i)?.to_string_lossy().into_owned();
        } else if flag_value(arg, "--relations-engine").is_some() {
            // Accepted for Go CLI compatibility. The Rust entrypoint always
            // uses the Rust relations crate once this native path is selected.
        } else if arg == OsStr::new("--relations-engine") {
            i += 1;
            args.get(i)?;
        } else if arg == OsStr::new("deps") {
            if command.replace("deps").is_some() {
                return None;
            }
        } else if arg == OsStr::new("impact") {
            if command.replace("impact").is_some() {
                return None;
            }
        } else if is_option(arg) {
            return None;
        } else if command.is_some() {
            positionals.push(arg.clone());
        } else {
            return None;
        }
        i += 1;
    }
    if json && format == "text" {
        format = "json".to_string();
    }
    Some(RelationArgs {
        kind: command?,
        path: match positionals.as_slice() {
            [path] => path.clone(),
            _ => return None,
        },
        format,
    })
}

pub(crate) fn relation_command_root_and_path(raw: &OsStr) -> Result<(PathBuf, String), String> {
    let root = env::current_dir()
        .map_err(|err| format!("relations: cannot determine working directory: {err}"))?;
    let raw_path = PathBuf::from(raw);
    let path = if raw_path.is_absolute() {
        raw_path
            .strip_prefix(&root)
            .map_err(|err| format!("relations: cannot relativize {:?}: {err}", raw))?
            .to_path_buf()
    } else {
        raw_path
    };
    let rel = clean_relative_path(&path)
        .ok_or_else(|| format!("relations: {:?} is outside the repository root", raw))?;
    let full = root.join(Path::new(&rel));
    let meta = std::fs::metadata(&full).map_err(|err| format!("relations: {rel}: {err}"))?;
    if meta.is_dir() {
        return Err(format!("relations: {rel} is a directory; pass a file path"));
    }
    Ok((root, rel))
}

pub(crate) fn clean_relative_path(path: &Path) -> Option<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            std::path::Component::ParentDir => {
                parts.pop()?;
            }
            _ => return None,
        }
    }
    if parts.is_empty() {
        return None;
    }
    Some(parts.join("/"))
}

pub(crate) fn relation_items(root: &Path, rel: &str, kind: &str) -> Result<Vec<String>, String> {
    let index = ctx_relations::build(&root.to_string_lossy()).map_err(|err| err.to_string())?;
    let edges = index.edges(rel);
    Ok(if kind == "impact" {
        edges.importers
    } else {
        edges.imports
    })
}

pub(crate) fn render_relation_list(res: &RelationList, format: &str) -> Result<(), String> {
    match format {
        "json" => {
            serde_json::to_writer_pretty(io::stdout(), res).map_err(|err| err.to_string())?;
            println!();
            Ok(())
        }
        "text" => {
            render_relation_text(res);
            Ok(())
        }
        other => Err(format!(
            "unknown --format value {other:?} (allowed: text, json)"
        )),
    }
}

pub(crate) fn render_relation_text(res: &RelationList) {
    let (label, empty) = if res.kind == "impact" {
        ("Dependents", "No dependents found")
    } else {
        ("Dependencies", "No dependencies found")
    };
    if res.items.is_empty() {
        println!("{empty} for {}.", res.path);
        return;
    }
    println!("{label} for {}:", res.path);
    for item in &res.items {
        println!("  - {item}");
    }
}
