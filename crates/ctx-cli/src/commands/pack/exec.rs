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
    if args.from_refs {
        for (set, flag) in [
            (args.from_where, "--from-where"),
            (args.from_stdin, "--from-stdin"),
            (!args.from_mix.is_empty(), "--from-mix"),
            (!args.diff_spec.is_empty(), "--diff"),
            (!args.base.is_empty(), "--base"),
            (args.changed, "--changed"),
        ] {
            if set {
                return Err(format!("pack: --from-refs does not combine with {flag}"));
            }
        }
    }
    if !args.why_paths.is_empty() && args.explain {
        return Err("pack: --why is mutually exclusive with --explain".to_string());
    }
    if !args.snapshot_id.is_empty() && !args.since_snapshot.is_empty() {
        return Err("--snapshot and --since-snapshot are mutually exclusive".to_string());
    }
    if !args.base.is_empty() {
        if !args.diff_spec.is_empty() {
            return Err(
                "pack: --base does not combine with --diff (use --diff BASE..HEAD for patches)"
                    .to_string(),
            );
        }
        if args.from_where {
            return Err("pack: --base does not combine with --from-where".to_string());
        }
    }
    if args.companions > 0 {
        if !args.diff_spec.is_empty() {
            return Err("pack: --companions does not apply to --diff".to_string());
        }
        if !(args.changed
            || args.from_where
            || args.from_stdin
            || args.from_refs
            || !args.from_mix.is_empty())
        {
            return Err(
                "pack: --companions needs a file selection to start from (--changed, --from-where, --from-stdin, --from-refs or --from-mix)"
                    .to_string(),
            );
        }
    }
    if !matches!(
        args.layout.as_str(),
        "" | "sequential" | "side-by-side" | "unified"
    ) {
        return Err(format!("unknown --layout value {:?}", args.layout));
    }
    if !matches!(args.overflow.as_str(), "drop" | "signatures") {
        return Err(format!(
            "pack: unknown --overflow value {:?} (use drop or signatures)",
            args.overflow
        ));
    }
    if args.around > 0 && !args.from_where && !args.from_refs {
        return Err("pack: --around needs --from-where or --from-refs (ctx where --format json | ctx pack --from-where --around 20)".to_string());
    }
    if args.from_refs && !args.around_set {
        args.around = DEFAULT_REFS_AROUND;
    }
    if args.from_refs && args.api_only {
        return Err("pack: --from-refs does not combine with --api-only".to_string());
    }
    if args.around > 0 && args.api_only {
        return Err("pack: --around does not combine with --api-only".to_string());
    }
    if args.overflow == "signatures" && args.api_only {
        return Err("pack: --overflow signatures does not combine with --api-only".to_string());
    }
    let path_filter = ctx_pack::pathfilter::PathFilter::new(&args.include, &args.exclude)
        .map_err(|err| format!("pack: --include/--exclude: {err}"))?;
    if !path_filter.is_empty() && !args.diff_spec.is_empty() {
        return Err("pack: --include/--exclude do not apply to --diff".to_string());
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
        if !args.no_warnings {
            let base = if root.is_file() {
                root.parent().unwrap_or(&root).to_path_buf()
            } else {
                root.clone()
            };
            for entry in &mix.files {
                let file = ctx_pack::excerpt::split_spec_in(&base, entry).0;
                if !base.join(file).is_file() {
                    eprintln!(
                        "warning: --from-mix: {} lists {file}, which is not a file under {}",
                        mix.id,
                        base.display()
                    );
                }
            }
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
        let diffs = git_diff_entries(&root, &args.diff_spec, args.api_only, &cfg.security)?;
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
    let (mut files, skipped) = if args.from_where {
        let mut stdin = Vec::new();
        io::stdin()
            .read_to_end(&mut stdin)
            .map_err(|err| format!("pack: read stdin: {err}"))?;
        let mut paths = ctx_pack::from_where::parse(&stdin).map_err(|err| err.to_string())?;
        let mut reasons = std::collections::HashMap::new();
        let mut excerpts = PackExcerpts::new();
        if args.around > 0 {
            for (path, lines) in ctx_pack::from_where::match_lines(&stdin) {
                excerpts.insert(
                    path,
                    ctx_pack::from_where::ranges_around(&lines, args.around),
                );
            }
        }
        if args.companions > 0 {
            let seeds: Vec<String> = paths
                .iter()
                .map(|path| ctx_pack::from_where::clean_input_path(path))
                .filter(|path| !path.is_empty())
                .collect();
            for (path, reason) in pack_companions(&root, &seeds, args.no_warnings)
                .into_iter()
                .take(args.companions)
            {
                paths.push(path.clone());
                reasons.insert(path, reason);
            }
        }
        read_pack_paths_ordered(
            &root,
            &paths,
            &reasons,
            &excerpts,
            &args,
            &path_filter,
            &cfg.security,
        )?
    } else if args.from_refs {
        pack_from_refs(&root, &args, &cfg, &path_filter)?
    } else if args.from_stdin {
        let mut stdin = String::new();
        io::stdin()
            .read_to_string(&mut stdin)
            .map_err(|err| format!("pack: read stdin: {err}"))?;
        let (paths, excerpts) = split_pack_selection(&root, &parse_pack_stdin_paths(&stdin));
        check_excerpt_args(&args, &excerpts)?;
        read_pack_root(
            &root,
            &args,
            &cfg,
            &path_filter,
            Some(&paths),
            &excerpts,
            false,
        )?
    } else if let Some(entries) = mix_paths.as_ref() {
        let (paths, excerpts) = split_pack_selection(&root, entries);
        check_excerpt_args(&args, &excerpts)?;
        read_pack_root(
            &root,
            &args,
            &cfg,
            &path_filter,
            Some(&paths),
            &excerpts,
            false,
        )?
    } else {
        read_pack_root(
            &root,
            &args,
            &cfg,
            &path_filter,
            None,
            &Default::default(),
            true,
        )?
    };
    let replay_header = if args.since_snapshot.is_empty() {
        None
    } else {
        Some(apply_since_snapshot_narrowing(&root, &args, &mut files)?)
    };
    let rendered =
        render_native_pack_with_skipped(&args, &files, &skipped, replay_header.as_ref())?;
    let out_sha256 = write_pack_output(&args, rendered)?;
    if !args.snapshot_id.is_empty() {
        if let Err(err) = save_pack_snapshot(&root, &args, &files, &skipped, &out_sha256) {
            eprintln!("warning: snapshot save failed: {err}");
        }
    }
    Ok(())
}

/// `--from-refs`: pack the repository files (and lines) the text on stdin
/// refers to. Mentions that name no file, or several, are warned about and
/// listed as omitted; finding no file at all is an error.
fn pack_from_refs(
    root: &Path,
    args: &PackArgs,
    cfg: &PackCtxToml,
    filter: &ctx_pack::pathfilter::PathFilter,
) -> Result<PackSelection, String> {
    let mut stdin = Vec::new();
    io::stdin()
        .read_to_end(&mut stdin)
        .map_err(|err| format!("pack: read stdin: {err}"))?;
    let text = String::from_utf8_lossy(&stdin);
    let files = list_pack_paths(root, cfg)?;
    let scan = ctx_pack::refs::scan(&text, &files, args.around);
    if !args.no_warnings {
        for miss in &scan.unresolved {
            eprintln!(
                "warning: --from-refs: {} (input line {}): {}",
                miss.mention, miss.input_line, miss.reason
            );
        }
    }
    if scan.targets.is_empty() {
        let hint = if scan.unresolved.is_empty() {
            String::new()
        } else {
            format!(" ({} unresolved mention(s) above)", scan.unresolved.len())
        };
        return Err(format!(
            "pack: --from-refs: the input names no file in {}{hint}",
            root.display()
        ));
    }
    let reasons: std::collections::HashMap<String, String> = scan
        .targets
        .iter()
        .map(|target| (target.path.clone(), target.reason()))
        .collect();
    let (paths, excerpts) = split_pack_selection(root, &scan.specs());
    let (mut files, mut skipped) =
        read_pack_root(root, args, cfg, filter, Some(&paths), &excerpts, false)?;
    for file in &mut files {
        if let Some(reason) = reasons.get(&file.path) {
            file.reason = if args.goal.is_empty() {
                reason.clone()
            } else {
                format!("{reason}; {}", file.reason)
            };
        }
    }
    skipped.extend(
        scan.unresolved
            .into_iter()
            .map(|miss| PackSkipped::new(miss.mention, miss.reason, 0)),
    );
    Ok((files, skipped))
}

/// Line-range excerpts cut the file's own text: they do not combine with
/// `--api-only`, which packs a rewritten public-API view.
fn check_excerpt_args(args: &PackArgs, excerpts: &PackExcerpts) -> Result<(), String> {
    if args.api_only && !excerpts.is_empty() {
        return Err(
            "pack: line ranges (path:START-END) do not combine with --api-only".to_string(),
        );
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
    skipped: &[PackSkipped],
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
        skipped: skipped
            .iter()
            .map(|file| ctx_replay::SkippedInput {
                path: file.path.clone(),
                reason: file.reason.clone(),
            })
            .collect(),
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
