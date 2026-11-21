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
    /// Hop limit, reported only for a transitive query (`--depth` > 1).
    #[serde(skip_serializing_if = "Option::is_none")]
    depth: Option<usize>,
    /// Distance and route of each item, for a transitive query.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    details: Vec<ctx_relations::Reached>,
}

#[derive(Debug)]
pub(crate) struct RelationArgs {
    kind: &'static str,
    paths: Vec<OsString>,
    format: String,
    depth: usize,
}

pub(crate) fn run_relations_command(args: &[OsString]) -> Option<ExitCode> {
    let parsed = parse_relation_args(args)?;
    match relation_list(&parsed).and_then(|list| render_relation_list(&list, &parsed.format)) {
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
    let mut depth = 1usize;
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
        } else if let Some(value) = flag_value(arg, "--depth") {
            depth = value.to_string_lossy().parse().ok()?;
        } else if arg == OsStr::new("--depth") {
            i += 1;
            depth = args.get(i)?.to_string_lossy().parse().ok()?;
        } else if flag_value(arg, "--relations-engine").is_some() {
            // Accepted for Go CLI compatibility. The Rust entrypoint always
            // uses the Rust relations crate once this native path is selected.
        } else if arg == OsStr::new("--relations-engine") {
            i += 1;
            args.get(i)?;
        } else if command.is_none() && arg == OsStr::new("deps") {
            command = Some("deps");
        } else if command.is_none() && arg == OsStr::new("impact") {
            command = Some("impact");
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
    if positionals.is_empty() {
        return None;
    }
    Some(RelationArgs {
        kind: command?,
        paths: positionals,
        format,
        depth,
    })
}

/// `raw` as a clean path relative to the working directory, and whether it
/// names a directory.
pub(crate) fn relation_anchor(root: &Path, raw: &OsStr) -> Result<(String, bool), String> {
    let raw_path = PathBuf::from(raw);
    let path = if raw_path.is_absolute() {
        raw_path
            .strip_prefix(root)
            .map_err(|err| format!("relations: cannot relativize {:?}: {err}", raw))?
            .to_path_buf()
    } else {
        raw_path
    };
    let rel = if path.as_os_str() == "." || path.as_os_str().is_empty() {
        String::new()
    } else {
        clean_relative_path(&path)
            .ok_or_else(|| format!("relations: {:?} is outside the repository root", raw))?
    };
    let full = root.join(Path::new(&rel));
    let meta = std::fs::metadata(&full)
        .map_err(|err| format!("relations: {}: {err}", raw.to_string_lossy()))?;
    Ok((rel, meta.is_dir()))
}

fn relation_list(args: &RelationArgs) -> Result<RelationList, String> {
    if args.depth == 0 {
        return Err("relations: --depth must be at least 1".to_string());
    }
    if !matches!(args.format.as_str(), "text" | "json" | "paths") {
        return Err(format!(
            "unknown --format value {:?} (allowed: text, json, paths)",
            args.format
        ));
    }
    let root = env::current_dir()
        .map_err(|err| format!("relations: cannot determine working directory: {err}"))?;
    let mut anchors = Vec::new();
    for raw in &args.paths {
        anchors.push(relation_anchor(&root, raw)?);
    }
    let index = ctx_relations::build(&root.to_string_lossy()).map_err(|err| err.to_string())?;
    // A directory anchor stands for every file under it that has edges; the
    // answer is then what the directory as a whole depends on / is used by.
    let mut seeds = std::collections::BTreeSet::new();
    for (rel, is_dir) in &anchors {
        if *is_dir {
            let prefix = if rel.is_empty() {
                String::new()
            } else {
                format!("{rel}/")
            };
            for file in index.imports.keys().chain(index.importers.keys()) {
                if file.starts_with(&prefix) {
                    seeds.insert(file.clone());
                }
            }
        } else {
            seeds.insert(rel.clone());
        }
    }
    let seeds: Vec<String> = seeds.into_iter().collect();
    let direction = if args.kind == "impact" {
        ctx_relations::Direction::Importers
    } else {
        ctx_relations::Direction::Imports
    };
    let mut details = index.reach(&seeds, direction, args.depth);
    if args.depth == 1 {
        // Direct edges keep the historical plain path order.
        details.sort_by(|a, b| a.path.cmp(&b.path));
    }
    let label: Vec<String> = anchors
        .iter()
        .map(|(rel, is_dir)| match (rel.is_empty(), is_dir) {
            (true, _) => ".".to_string(),
            (false, true) => format!("{rel}/"),
            (false, false) => rel.clone(),
        })
        .collect();
    Ok(RelationList {
        path: label.join(", "),
        kind: args.kind,
        items: details.iter().map(|item| item.path.clone()).collect(),
        depth: (args.depth > 1).then_some(args.depth),
        details: if args.depth > 1 { details } else { Vec::new() },
    })
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

pub(crate) fn render_relation_list(res: &RelationList, format: &str) -> Result<(), String> {
    match format {
        "json" => {
            serde_json::to_writer_pretty(io::stdout(), res).map_err(|err| err.to_string())?;
            println!();
            Ok(())
        }
        "paths" => {
            // One path per line and nothing else, for
            // `ctx impact FILE --format paths | ctx pack --from-stdin`.
            for item in &res.items {
                println!("{item}");
            }
            Ok(())
        }
        "text" => {
            render_relation_text(res);
            Ok(())
        }
        other => Err(format!(
            "unknown --format value {other:?} (allowed: text, json, paths)"
        )),
    }
}

pub(crate) fn render_relation_text(res: &RelationList) {
    let (label, empty) = if res.kind == "impact" {
        ("Dependents", "No dependents found")
    } else {
        ("Dependencies", "No dependencies found")
    };
    let within = match res.depth {
        Some(depth) => format!(" (within {depth} hops)"),
        None => String::new(),
    };
    if res.items.is_empty() {
        println!("{empty} for {}{within}.", res.path);
        return;
    }
    println!("{label} for {}{within}:", res.path);
    if res.details.is_empty() {
        for item in &res.items {
            println!("  - {item}");
        }
        return;
    }
    for item in &res.details {
        if item.depth == 1 {
            println!("  - {}", item.path);
        } else {
            println!("  - {} ({} hops, via {})", item.path, item.depth, item.via);
        }
    }
}
