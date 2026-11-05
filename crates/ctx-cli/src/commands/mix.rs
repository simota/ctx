//! `ctx mix`: create and manage saved file selections ("mixes").
//!
//! A mix is a named, ordered list of repository files plus the goal and token
//! budget they were chosen for. `ctx pack --from-mix <ID|NAME>` packs it,
//! `ctx browse` lists/creates them from the pinned-files board, and the MCP
//! `ctx_pack` tool accepts `mix`. This command is the CLI way to build one:
//!
//! ```text
//! ctx mix save "auth review" src/auth/ src/session.rs --goal "session expiry" --budget 30000
//! ctx where "session login" --json | ctx mix save login --from-where
//! ctx mix list
//! ctx mix show auth-review
//! ctx mix add auth-review tests/session_test.rs
//! ctx mix drop auth-review src/auth/legacy.rs
//! ctx mix set auth-review --goal "review token refresh"
//! ctx mix rm auth-review
//! ```

use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::commands::pack::{
    collect_pack_inputs, load_pack_ctx_toml, parse_pack_stdin_paths, PackIgnore,
};
use crate::common::*;

#[derive(Debug, Default)]
struct MixArgs {
    action: String,
    positionals: Vec<String>,
    root: Option<PathBuf>,
    goal: Option<String>,
    name: Option<String>,
    id: String,
    budget: Option<i64>,
    from_stdin: bool,
    from_where: bool,
    json: bool,
    paths_only: bool,
}

const USAGE: &str = "usage: ctx mix <save|list|show|add|drop|set|rm> ...\n\
  ctx mix save NAME [PATH...] [--goal G] [--budget N] [--id ID] [--from-where|--from-stdin]\n\
  ctx mix list [--json]\n\
  ctx mix show ID|NAME [--json|--paths]\n\
  ctx mix add ID|NAME PATH...\n\
  ctx mix drop ID|NAME PATH...\n\
  ctx mix set ID|NAME [--name N] [--goal G] [--budget N]\n\
  ctx mix rm ID|NAME...";

pub(crate) fn run_mix_command(args: &[OsString]) -> Option<ExitCode> {
    let parsed = match parse_mix_args(args) {
        Ok(parsed) => parsed,
        Err(err) => {
            eprintln!("{err}");
            return Some(ExitCode::from(2));
        }
    };
    match mix_command(parsed) {
        Ok(()) => Some(ExitCode::SUCCESS),
        Err(err) => {
            eprintln!("{err}");
            Some(ExitCode::from(1))
        }
    }
}

fn take_value(args: &[OsString], i: &mut usize, name: &str) -> Result<String, String> {
    *i += 1;
    args.get(*i)
        .map(|value| value.to_string_lossy().into_owned())
        .ok_or_else(|| format!("mix: {name} needs a value"))
}

fn parse_mix_args(args: &[OsString]) -> Result<MixArgs, String> {
    let mut out = MixArgs::default();
    let mut saw_mix = false;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        let text = arg.to_string_lossy();
        if !saw_mix && arg == OsStr::new("mix") {
            saw_mix = true;
        } else if arg == OsStr::new("--json") {
            out.json = true;
        } else if arg == OsStr::new("--paths") {
            out.paths_only = true;
        } else if arg == OsStr::new("--from-stdin") {
            out.from_stdin = true;
        } else if arg == OsStr::new("--from-where") {
            out.from_where = true;
        } else if let Some(value) = flag_value(arg, "--goal") {
            out.goal = Some(value.to_string_lossy().into_owned());
        } else if arg == OsStr::new("--goal") {
            out.goal = Some(take_value(args, &mut i, "--goal")?);
        } else if let Some(value) = flag_value(arg, "--name") {
            out.name = Some(value.to_string_lossy().into_owned());
        } else if arg == OsStr::new("--name") {
            out.name = Some(take_value(args, &mut i, "--name")?);
        } else if let Some(value) = flag_value(arg, "--id") {
            out.id = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--id") {
            out.id = take_value(args, &mut i, "--id")?;
        } else if let Some(value) = flag_value(arg, "--root") {
            out.root = Some(PathBuf::from(value));
        } else if arg == OsStr::new("--root") {
            out.root = Some(PathBuf::from(take_value(args, &mut i, "--root")?));
        } else if let Some(value) = flag_value(arg, "--budget") {
            out.budget = Some(parse_budget(&value.to_string_lossy())?);
        } else if arg == OsStr::new("--budget") {
            out.budget = Some(parse_budget(&take_value(args, &mut i, "--budget")?)?);
        } else if arg == OsStr::new("--") {
            out.positionals.extend(
                args[i + 1..]
                    .iter()
                    .map(|a| a.to_string_lossy().into_owned()),
            );
            break;
        } else if is_option(arg) {
            return Err(format!("mix: unknown option {text}\n{USAGE}"));
        } else if out.action.is_empty() {
            out.action = text.into_owned();
        } else {
            out.positionals.push(text.into_owned());
        }
        i += 1;
    }
    if out.action.is_empty() {
        return Err(USAGE.to_string());
    }
    Ok(out)
}

fn parse_budget(text: &str) -> Result<i64, String> {
    text.trim()
        .parse::<i64>()
        .ok()
        .filter(|n| (0..=ctx_mix::MAX_BUDGET).contains(n))
        .ok_or_else(|| {
            format!(
                "mix: --budget must be a number between 0 and {}",
                ctx_mix::MAX_BUDGET
            )
        })
}

fn mix_command(args: MixArgs) -> Result<(), String> {
    let store = ctx_mix::Store::open_default().map_err(|err| err.to_string())?;
    match args.action.as_str() {
        "save" | "new" | "create" => save(&store, args),
        "list" | "ls" => list(&store, &args),
        "show" => show(&store, &args),
        "add" => add(&store, args),
        "drop" => drop_files(&store, args),
        "set" => set(&store, args),
        "rm" | "remove" | "delete" => remove(&store, &args),
        other => Err(format!("mix: unknown action {other:?}\n{USAGE}")),
    }
}

fn root_of(args: &MixArgs) -> Result<PathBuf, String> {
    let root = args.root.clone().unwrap_or_else(|| PathBuf::from("."));
    std::fs::canonicalize(&root)
        .ok()
        .filter(|path| path.is_dir())
        .ok_or_else(|| format!("mix: --root {}: not a directory", root.display()))
}

/// Resolve command-line paths (relative to the current directory) to files
/// relative to `root`. A directory expands to the files a pack of it would
/// walk (ignore rules applied); a missing path or one outside `root` is an
/// error so a typo never lands in a recipe.
fn resolve_cli_paths(root: &Path, raw: &[String]) -> Result<Vec<String>, String> {
    let cwd = std::env::current_dir().map_err(|err| err.to_string())?;
    let cfg = load_pack_ctx_toml(root).unwrap_or_default();
    let ignore = PackIgnore::load(root, &cfg.ignore.patterns, true);
    let mut out = Vec::new();
    for path in raw {
        let joined = cwd.join(path);
        let abs = std::fs::canonicalize(&joined)
            .map_err(|_| format!("mix: {path}: no such file or directory"))?;
        if !abs.starts_with(root) {
            return Err(format!(
                "mix: {path}: outside the repository root {}",
                root.display()
            ));
        }
        if abs.is_dir() {
            let mut inputs = Vec::new();
            collect_pack_inputs(root, &abs, &ignore, &mut inputs)?;
            if inputs.is_empty() {
                eprintln!("warning: mix: {path}: no packable files");
            }
            out.extend(inputs.into_iter().map(|input| input.path));
        } else if abs.is_file() {
            let rel = abs
                .strip_prefix(root)
                .map_err(|err| err.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            out.push(rel);
        } else {
            return Err(format!("mix: {path}: not a regular file"));
        }
    }
    Ok(out)
}

/// Paths from `ctx where --json` / a newline list on stdin, kept only when
/// they name a file under `root`.
fn read_stdin_paths(root: &Path, from_where: bool) -> Result<Vec<String>, String> {
    let mut stdin = Vec::new();
    std::io::stdin()
        .read_to_end(&mut stdin)
        .map_err(|err| format!("mix: read stdin: {err}"))?;
    let paths = if from_where {
        ctx_pack::from_where::parse(&stdin).map_err(|err| format!("mix: --from-where: {err}"))?
    } else {
        parse_pack_stdin_paths(&String::from_utf8_lossy(&stdin))
    };
    let mut out = Vec::new();
    for path in paths {
        let clean = ctx_pack::from_where::clean_input_path(&path);
        if clean.is_empty() {
            continue;
        }
        let Ok(normalized) = ctx_mix::normalize_path(&clean) else {
            eprintln!("warning: mix: skipping {clean}: outside the repository");
            continue;
        };
        if resolves_to_file_in(root, &root.join(&normalized)) {
            out.push(normalized);
        } else {
            eprintln!(
                "warning: mix: skipping {clean}: not a file under {}",
                root.display()
            );
        }
    }
    Ok(out)
}

fn save(store: &ctx_mix::Store, args: MixArgs) -> Result<(), String> {
    let root = root_of(&args)?;
    let Some((name, paths)) = args.positionals.split_first() else {
        return Err("mix save: a NAME is required\nusage: ctx mix save NAME [PATH...]".to_string());
    };
    if args.from_where && args.from_stdin {
        return Err("mix save: --from-where and --from-stdin are mutually exclusive".to_string());
    }
    let mut files = resolve_cli_paths(&root, paths)?;
    if args.from_where || args.from_stdin {
        files.extend(read_stdin_paths(&root, args.from_where)?);
    }
    if files.is_empty() {
        return Err(
            "mix save: no files selected (pass PATH..., or pipe paths with --from-stdin / --from-where)"
                .to_string(),
        );
    }
    let mix = store
        .create(ctx_mix::Draft {
            id: args.id.clone(),
            name: name.clone(),
            goal: args.goal.clone().unwrap_or_default(),
            files,
            budget: args.budget.unwrap_or(0),
        })
        .map_err(|err| err.to_string())?;
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&mix).map_err(|e| e.to_string())?
        );
    } else {
        println!(
            "Saved mix {} ({} files) to {}",
            mix.id,
            mix.files.len(),
            store.dir().join(format!("{}.mix.json", mix.id)).display()
        );
        println!("Pack it with: ctx pack --from-mix {}", mix.id);
    }
    Ok(())
}

fn list(store: &ctx_mix::Store, args: &MixArgs) -> Result<(), String> {
    let mixes = store.list().map_err(|err| err.to_string())?;
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&mixes).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    if mixes.is_empty() {
        println!("No saved mixes. Create one with: ctx mix save NAME PATH...");
        return Ok(());
    }
    let id_width = mixes.iter().map(|m| m.id.len()).max().unwrap_or(2).max(2);
    println!(
        "{:<id_width$}  {:>5}  {:>7}  {:<10}  NAME / GOAL",
        "ID", "FILES", "BUDGET", "CREATED"
    );
    for mix in &mixes {
        let budget = if mix.budget.limit > 0 {
            mix.budget.limit.to_string()
        } else {
            "-".to_string()
        };
        let created = mix.created.get(..10).unwrap_or(&mix.created);
        let mut label = mix.name.clone();
        if !mix.goal.is_empty() {
            label.push_str(" — ");
            label.push_str(&mix.goal);
        }
        println!(
            "{:<id_width$}  {:>5}  {:>7}  {:<10}  {}",
            mix.id,
            mix.files.len(),
            budget,
            created,
            label
        );
    }
    Ok(())
}

fn find_one(store: &ctx_mix::Store, args: &MixArgs, action: &str) -> Result<ctx_mix::Mix, String> {
    let reference = args
        .positionals
        .first()
        .ok_or_else(|| format!("mix {action}: an ID or NAME is required"))?;
    store.find(reference).map_err(|err| err.to_string())
}

fn show(store: &ctx_mix::Store, args: &MixArgs) -> Result<(), String> {
    let mix = find_one(store, args, "show")?;
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&mix).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    if args.paths_only {
        for file in &mix.files {
            println!("{file}");
        }
        return Ok(());
    }
    let root = root_of(args).ok();
    println!("{} ({})", mix.name, mix.id);
    println!(
        "Goal:    {}",
        if mix.goal.is_empty() { "-" } else { &mix.goal }
    );
    if mix.budget.limit > 0 {
        println!("Budget:  {} tokens", mix.budget.limit);
    } else {
        println!("Budget:  - (ctx pack default)");
    }
    println!("Created: {}", mix.created);
    if !mix.updated.is_empty() {
        println!("Updated: {}", mix.updated);
    }
    println!("Files ({}):", mix.files.len());
    let mut missing = 0;
    for file in &mix.files {
        let exists = root.as_ref().is_none_or(|root| root.join(file).is_file());
        if exists {
            println!("  {file}");
        } else {
            missing += 1;
            println!("  {file}  (missing here)");
        }
    }
    if missing > 0 {
        println!(
            "\n{missing} file(s) are not present under the current root; `ctx pack --from-mix` skips them."
        );
    }
    println!("\nPack it with: ctx pack --from-mix {}", mix.id);
    Ok(())
}

fn add(store: &ctx_mix::Store, args: MixArgs) -> Result<(), String> {
    let mut mix = find_one(store, &args, "add")?;
    let root = root_of(&args)?;
    let mut added = resolve_cli_paths(&root, &args.positionals[1..])?;
    if args.from_where || args.from_stdin {
        added.extend(read_stdin_paths(&root, args.from_where)?);
    }
    if added.is_empty() {
        return Err("mix add: no files given".to_string());
    }
    let before = mix.files.len();
    mix.files.extend(added);
    let mix = store.update(mix).map_err(|err| err.to_string())?;
    println!(
        "Added {} file(s) to {} ({} files)",
        mix.files.len() - before,
        mix.id,
        mix.files.len()
    );
    Ok(())
}

fn drop_files(store: &ctx_mix::Store, args: MixArgs) -> Result<(), String> {
    let mut mix = find_one(store, &args, "drop")?;
    let patterns: Vec<String> = args.positionals[1..]
        .iter()
        .map(|p| ctx_mix::normalize_path(p).map_err(|err| err.to_string()))
        .collect::<Result<_, _>>()?;
    if patterns.is_empty() {
        return Err("mix drop: no files given".to_string());
    }
    let before = mix.files.len();
    // A pattern removes the file itself or everything under a directory.
    mix.files.retain(|file| {
        !patterns
            .iter()
            .any(|p| file == p || file.starts_with(&format!("{p}/")))
    });
    let removed = before - mix.files.len();
    if removed == 0 {
        return Err(format!(
            "mix drop: none of the given paths are in {}",
            mix.id
        ));
    }
    if mix.files.is_empty() {
        return Err(format!(
            "mix drop: that would leave {} empty; delete it with `ctx mix rm {}`",
            mix.id, mix.id
        ));
    }
    let mix = store.update(mix).map_err(|err| err.to_string())?;
    println!(
        "Dropped {removed} file(s) from {} ({} files)",
        mix.id,
        mix.files.len()
    );
    Ok(())
}

fn set(store: &ctx_mix::Store, args: MixArgs) -> Result<(), String> {
    let mut mix = find_one(store, &args, "set")?;
    if args.name.is_none() && args.goal.is_none() && args.budget.is_none() {
        return Err("mix set: pass --name, --goal and/or --budget".to_string());
    }
    if let Some(name) = args.name {
        mix.name = name;
    }
    if let Some(goal) = args.goal {
        mix.goal = goal;
    }
    if let Some(budget) = args.budget {
        mix.budget.limit = budget;
    }
    let mix = store.update(mix).map_err(|err| err.to_string())?;
    println!("Updated {}", mix.id);
    Ok(())
}

fn remove(store: &ctx_mix::Store, args: &MixArgs) -> Result<(), String> {
    if args.positionals.is_empty() {
        return Err("mix rm: an ID or NAME is required".to_string());
    }
    for reference in &args.positionals {
        let mix = store.find(reference).map_err(|err| err.to_string())?;
        store.delete(&mix.id).map_err(|err| err.to_string())?;
        println!("Removed {}", mix.id);
    }
    Ok(())
}
