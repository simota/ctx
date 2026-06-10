use std::env;
use std::ffi::OsString;
use std::io::{self, Read};
use std::path::Path;
use std::process::ExitCode;

use super::*;
use crate::common::*;

pub(crate) fn run_pack_command(args: &[OsString]) -> Option<ExitCode> {
    let parsed = parse_pack_args(args)?;
    match pack_command(parsed) {
        Ok(()) => Some(ExitCode::SUCCESS),
        Err(err) => {
            eprintln!("{err}");
            Some(ExitCode::from(1))
        }
    }
}

pub(crate) fn pack_command(mut args: PackArgs) -> Result<(), String> {
    if args.budget <= 0 {
        return Err("pack: budget must be positive".to_string());
    }
    if args.from_where && args.from_stdin {
        return Err("pack: --from-where and --from-stdin are mutually exclusive".to_string());
    }
    if !args.from_mix.is_empty() && args.from_where {
        return Err("pack: --from-mix and --from-where are mutually exclusive".to_string());
    }
    if !args.from_mix.is_empty() && args.from_stdin {
        return Err("pack: --from-mix and --from-stdin are mutually exclusive".to_string());
    }
    if !args.why_paths.is_empty() && args.explain {
        return Err("pack: --why is mutually exclusive with --explain".to_string());
    }
    if !args.snapshot_id.is_empty() && !args.since_snapshot.is_empty() {
        return Err("--snapshot and --since-snapshot are mutually exclusive".to_string());
    }
    if !matches!(
        args.layout.as_str(),
        "" | "sequential" | "side-by-side" | "unified"
    ) {
        return Err(format!("unknown --layout value {:?}", args.layout));
    }
    if args.from_where && !args.goal.is_empty() && !args.no_warnings {
        eprintln!("warning: --from-where is set; --goal relevance scoring is skipped (using where ranking instead)");
    }
    let root = if args.root.is_absolute() {
        args.root.clone()
    } else {
        env::current_dir()
            .map_err(|err| err.to_string())?
            .join(&args.root)
    };
    let cfg = load_pack_ctx_toml(&root)?;
    if args.preset.is_empty() && !args.preset_changed {
        args.preset = cfg.pack.preset.clone();
    }
    apply_pack_preset(&mut args)?;
    let mix_paths = if args.from_mix.is_empty() {
        None
    } else {
        let mix = load_pack_mix(&args.from_mix)?;
        if !args.goal_changed && !mix.goal.is_empty() {
            args.goal = mix.goal.clone();
        }
        if !args.budget_changed && mix.budget.limit > 0 {
            args.budget = mix.budget.limit;
        }
        Some(mix.files)
    };
    if !args.why_paths.is_empty() {
        let diagnostics = diagnose_pack_why(&root, &args, &cfg)?;
        let rendered = render_why_diagnostics(&diagnostics, &args.format)?;
        write_pack_output(&args, rendered)?;
        if let Some(missing) = diagnostics.iter().find(|diag| !diag.exists) {
            return Err(format!(
                "pack --why: path not found in repo: {}",
                missing.path
            ));
        }
        return Ok(());
    }
    if !args.diff_spec.is_empty() {
        let diffs = git_diff_entries(&root, &args.diff_spec, args.api_only)?;
        let rendered = ctx_pack::diff::render(
            &diffs,
            &ctx_pack::DiffOptions {
                layout: args.layout.clone(),
                preset: String::new(),
            },
        );
        write_pack_output(&args, rendered)?;
        return Ok(());
    }
    let mut files = if args.from_where {
        let mut stdin = Vec::new();
        io::stdin()
            .read_to_end(&mut stdin)
            .map_err(|err| format!("pack: read stdin: {err}"))?;
        let paths = ctx_pack::from_where::parse(&stdin).map_err(|err| err.to_string())?;
        read_pack_paths_ordered(&root, &paths, args.budget, args.no_warnings)?
    } else if args.from_stdin {
        let mut stdin = String::new();
        io::stdin()
            .read_to_string(&mut stdin)
            .map_err(|err| format!("pack: read stdin: {err}"))?;
        let paths = parse_pack_stdin_paths(&stdin);
        read_pack_root(&root, &args, &cfg, Some(&paths), false)?
    } else if let Some(paths) = mix_paths.as_ref() {
        read_pack_root(&root, &args, &cfg, Some(paths), false)?
    } else {
        read_pack_root(&root, &args, &cfg, None, true)?
    };
    let replay_header = if args.since_snapshot.is_empty() {
        None
    } else {
        Some(apply_since_snapshot_narrowing(&root, &args, &mut files)?)
    };
    let rendered = render_native_pack(&args, &files, replay_header.as_ref())?;
    let out_sha256 = write_pack_output(&args, rendered)?;
    if !args.snapshot_id.is_empty() {
        if let Err(err) = save_pack_snapshot(&root, &args, &files, &out_sha256) {
            eprintln!("warning: snapshot save failed: {err}");
        }
    }
    Ok(())
}

pub(crate) fn write_pack_output(args: &PackArgs, rendered: String) -> Result<String, String> {
    if args.out.is_empty() || args.out == "-" {
        print!("{rendered}");
        Ok(String::new())
    } else {
        std::fs::write(&args.out, rendered)
            .map_err(|err| format!("pack: write {}: {err}", args.out))?;
        eprintln!("Writing context to {}", args.out);
        sha256_file_hex(Path::new(&args.out))
    }
}

pub(crate) fn save_pack_snapshot(
    root: &Path,
    args: &PackArgs,
    files: &[NativePackFile],
    out_sha256: &str,
) -> Result<(), String> {
    let dir = ctx_replay::resolve(ctx_replay::ResolveOptions {
        shared: args.replay_shared,
        root: root.to_string_lossy().into_owned(),
    })
    .map_err(|err| err.to_string())?;
    let store = ctx_replay::open_store(&dir).map_err(|err| err.to_string())?;
    let manifest = ctx_replay::build_manifest(ctx_replay::BuildInput {
        id: args.snapshot_id.clone(),
        created_at: current_rfc3339_utc(),
        ctx_version: "dev".to_string(),
        goal: args.goal.clone(),
        budget: args.budget,
        used: files.iter().map(|file| file.tokens).sum(),
        root: root.to_string_lossy().into_owned(),
        preset: String::new(),
        format: args.format.clone(),
        out_sha256: out_sha256.to_string(),
        included: files
            .iter()
            .map(|file| ctx_replay::EntryInput {
                path: file.path.clone(),
                abs_path: file.abs_path.clone(),
                tokens: file.tokens,
                relevance: file.relevance.clone(),
                score: file.score,
                reason: file.reason.clone(),
            })
            .collect(),
        skipped: Vec::new(),
    })
    .map_err(|err| err.to_string())?;
    store.save(&manifest).map_err(|err| err.to_string())?;
    eprintln!(
        "Saved snapshot {} -> {}",
        args.snapshot_id,
        store
            .path(&args.snapshot_id)
            .map_err(|err| err.to_string())?
            .display()
    );
    Ok(())
}

pub(crate) fn apply_since_snapshot_narrowing(
    root: &Path,
    args: &PackArgs,
    files: &mut Vec<NativePackFile>,
) -> Result<NativeReplayHeader, String> {
    let base = load_replay_manifest(root, args)?;
    let current = build_pack_manifest(root, args, files, "current", "")?;
    let summary = ctx_replay::compute(
        &base,
        &current,
        ctx_replay::DiffOptions {
            strict: args.replay_strict,
        },
    );
    let changed: std::collections::BTreeSet<String> = summary
        .changes
        .iter()
        .filter(|change| {
            matches!(
                change.kind,
                ctx_replay::ChangeKind::Added | ctx_replay::ChangeKind::Modified
            )
        })
        .map(|change| change.path.clone())
        .collect();
    files.retain(|file| changed.contains(&file.path));
    Ok(NativeReplayHeader {
        base_id: base.id,
        added: summary.added,
        modified: summary.modified,
        removed: summary.removed,
        token_delta: summary.token_delta,
    })
}

pub(crate) fn load_replay_manifest(
    root: &Path,
    args: &PackArgs,
) -> Result<ctx_replay::Manifest, String> {
    let dir = ctx_replay::resolve(ctx_replay::ResolveOptions {
        shared: args.replay_shared,
        root: root.to_string_lossy().into_owned(),
    })
    .map_err(|err| err.to_string())?;
    let store = ctx_replay::open_store(&dir).map_err(|err| err.to_string())?;
    store
        .load(&args.since_snapshot)
        .map_err(|err| err.to_string())
}

pub(crate) fn build_pack_manifest(
    root: &Path,
    args: &PackArgs,
    files: &[NativePackFile],
    id: &str,
    out_sha256: &str,
) -> Result<ctx_replay::Manifest, String> {
    ctx_replay::build_manifest(ctx_replay::BuildInput {
        id: id.to_string(),
        created_at: current_rfc3339_utc(),
        ctx_version: "dev".to_string(),
        goal: args.goal.clone(),
        budget: args.budget,
        used: files.iter().map(|file| file.tokens).sum(),
        root: root.to_string_lossy().into_owned(),
        preset: String::new(),
        format: args.format.clone(),
        out_sha256: out_sha256.to_string(),
        included: files
            .iter()
            .map(|file| ctx_replay::EntryInput {
                path: file.path.clone(),
                abs_path: file.abs_path.clone(),
                tokens: file.tokens,
                relevance: file.relevance.clone(),
                score: file.score,
                reason: file.reason.clone(),
            })
            .collect(),
        skipped: Vec::new(),
    })
    .map_err(|err| err.to_string())
}
