use std::env;
use std::ffi::{OsStr, OsString};
use std::io::{self};
use std::path::Path;
use std::process::ExitCode;

use crate::commands::digest::generate_digest;
use crate::commands::focus::focus_files;
use crate::commands::pack::*;
use crate::commands::where_cmd::where_files;
use crate::common::*;

#[derive(Debug)]
pub(crate) struct BraidArgs {
    file: String,
    budget: i64,
    format: String,
    explain: bool,
    dry_run: bool,
    out: String,
    contract: bool,
}

#[derive(Debug)]
pub(crate) struct BraidExecResult {
    paths: Vec<String>,
}

pub(crate) fn run_braid_command(args: &[OsString]) -> Option<ExitCode> {
    let parsed = parse_braid_args(args)?;
    match braid_command(parsed) {
        Ok(()) => Some(ExitCode::SUCCESS),
        Err(err) => {
            eprintln!("{err}");
            let code = if is_braid_validation_error(&err) {
                2
            } else {
                1
            };
            Some(ExitCode::from(code))
        }
    }
}

pub(crate) fn parse_braid_args(args: &[OsString]) -> Option<BraidArgs> {
    let mut saw_braid = false;
    let mut json = false;
    let mut plain = false;
    let mut file = "braid.toml".to_string();
    let mut budget = 32000_i64;
    let mut format = "markdown".to_string();
    let mut explain = false;
    let mut dry_run = false;
    let mut out = String::new();
    let mut contract = true; // braid enables contract by default (matches Go)
    let mut positionals = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == OsStr::new("--json") {
            json = true;
        } else if arg == OsStr::new("--plain") {
            plain = true;
        } else if arg == OsStr::new("braid") {
            if saw_braid {
                return None;
            }
            saw_braid = true;
        } else if let Some(value) = flag_value(arg, "--file") {
            file = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--file") {
            i += 1;
            file = args.get(i)?.to_string_lossy().into_owned();
        } else if let Some(value) = flag_value(arg, "--budget") {
            budget = value.to_string_lossy().parse().ok()?;
        } else if arg == OsStr::new("--budget") {
            i += 1;
            budget = args.get(i)?.to_string_lossy().parse().ok()?;
        } else if let Some(value) = flag_value(arg, "--format") {
            format = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--format") {
            i += 1;
            format = args.get(i)?.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--explain") {
            explain = true;
        } else if arg == OsStr::new("--dry-run") {
            dry_run = true;
        } else if let Some(value) = flag_value(arg, "--out") {
            out = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--out") {
            i += 1;
            out = args.get(i)?.to_string_lossy().into_owned();
        } else if flag_value(arg, "--braid-engine").is_some() {
        } else if arg == OsStr::new("--braid-engine") {
            i += 1;
            args.get(i)?;
        } else if let Some(value) = flag_value(arg, "--contract") {
            // --contract=false or --contract=true (Go supports this form)
            let v = value.to_string_lossy();
            contract = v != "false" && v != "0";
        } else if arg == OsStr::new("--contract") {
            contract = true;
        } else if arg == OsStr::new("--no-contract") {
            contract = false;
        } else if is_option(arg) {
            return None;
        } else if saw_braid {
            positionals.push(arg.clone());
        } else {
            return None;
        }
        i += 1;
    }
    if !saw_braid || !positionals.is_empty() {
        return None;
    }
    if json {
        format = "json".to_string();
    } else if plain && format == "markdown" {
        format = "plain".to_string();
    }
    Some(BraidArgs {
        file,
        budget,
        format,
        explain,
        dry_run,
        out,
        contract,
    })
}

pub(crate) fn braid_command(args: BraidArgs) -> Result<(), String> {
    if args.budget <= 0 {
        return Err("braid: budget must be positive".to_string());
    }
    let root = env::current_dir().map_err(|err| format!("braid: cwd: {err}"))?;
    let cfg = ctx_braid::load_from_file(Path::new(&args.file)).map_err(|err| err.to_string())?;
    let alloc = ctx_braid::allocate(&cfg, args.budget);
    if !alloc.warning.is_empty() {
        eprint!("{}", alloc.warning);
    }

    let mut selections = Vec::with_capacity(cfg.strands.len());
    let mut reports = Vec::with_capacity(cfg.strands.len());
    for (idx, strand) in cfg.strands.iter().enumerate() {
        let allocation = &alloc.allocations[idx];
        let exec = exec_braid_strand(&root, strand)?;
        let mut trimmed = Vec::new();
        let mut used_tokens = 0_i64;
        let mut dropped = 0_i64;
        for path in &exec.paths {
            let tokens = estimate_path_tokens(&root, path);
            if allocation.budget > 0 && used_tokens + tokens > allocation.budget {
                dropped += 1;
                continue;
            }
            used_tokens += tokens;
            trimmed.push(path.clone());
        }
        selections.push(ctx_braid::StrandSelection {
            name: strand.name.clone(),
            policy: strand.policy.unwrap_or_merge(),
            paths: trimmed.clone(),
        });
        reports.push(ctx_braid::StrandReport {
            name: strand.name.clone(),
            share: allocation.share,
            budget: allocation.budget,
            selected: trimmed.len() as i64,
            tokens: used_tokens,
            policy: strand.policy.unwrap_or_merge(),
            raw_paths: exec.paths.len() as i64,
            trim_note: if dropped > 0 {
                format!("{dropped} paths dropped to fit per-strand budget")
            } else {
                String::new()
            },
        });
    }

    let merged = ctx_braid::merge_paths(&selections);
    let tokens_used = merged
        .iter()
        .map(|file| estimate_path_tokens(&root, &file.path))
        .sum();
    let result = ctx_braid::BraidResult {
        file: args.file.clone(),
        budget: args.budget,
        strands: reports,
        files: merged.clone(),
        tokens_used,
        dry_run: args.dry_run,
        pack_bytes: 0,
        pack_sha256: String::new(),
    };

    if args.dry_run {
        match args.format.as_str() {
            "json" => {
                let bytes = ctx_braid::render_json(&result).map_err(|err| err.to_string())?;
                io::Write::write_all(&mut io::stdout(), &bytes).map_err(|err| err.to_string())?;
            }
            "plain" => print!("{}", ctx_braid::render_plain(&result)),
            "markdown" | "xml" | "" => {
                print!("{}", ctx_braid::render_markdown(&result, args.explain))
            }
            other => return Err(format!("unknown --format value {other:?}")),
        }
        return Ok(());
    }

    // Non-dry-run: walk the repo (RespectCtxignore=false, mirroring Go braid.go),
    // build NativePackFile for each merged path in selection order, render the
    // pack body, and write: allocation-report + "\n" + pack-body to stdout.
    //
    // Go braid.go does:
    //   1. walk.New(root, walkOpts{RespectCtxignore:false}) → Flatten → byPath map
    //   2. packFiles := []*model.FileInfo{...} in merged order (skip if not in byPath)
    //   3. pack.PackWithResult(packTarget, packFiles, packOpts{NoMetadata:true, NoWarnings:true})
    //   4. Render(w, res, format, explain)  ← allocation report
    //   5. w.Write("\n"); w.Write(packBuf.Bytes())
    //
    // We replicate this with the native pack-file builder already present in this file.
    let pack_files = braid_build_pack_files(&root, &merged)?;

    let pack_body = braid_render_pack_body(&pack_files, &args)?;

    // Allocation report goes to stdout first (mirroring Go's Render(w, res, ...))
    match args.format.as_str() {
        "json" => {
            let bytes = ctx_braid::render_json(&result).map_err(|err| err.to_string())?;
            io::Write::write_all(&mut io::stdout(), &bytes).map_err(|err| err.to_string())?;
        }
        "plain" => print!("{}", ctx_braid::render_plain(&result)),
        "markdown" | "xml" | "" => {
            print!("{}", ctx_braid::render_markdown(&result, args.explain))
        }
        other => return Err(format!("unknown --format value {other:?}")),
    }

    // Pack body follows, separated by a blank line (Go: w.Write([]byte("\n"))).
    if args.out.is_empty() || args.out == "-" {
        print!("\n{pack_body}");
    } else {
        std::fs::write(&args.out, &pack_body)
            .map_err(|err| format!("braid: write {}: {err}", args.out))?;
        eprintln!(
            "Writing braid pack to {} ({} bytes)",
            args.out,
            pack_body.len()
        );
    }

    Ok(())
}

/// Build a `Vec<NativePackFile>` for the braid-selected merged paths.
///
/// Mirrors Go's braid.go non-dry-run walk:
///   - RespectCtxignore=false (user-selected paths must not be silently dropped)
///   - Selection order preserved by iterating `merged` in arrival order
///   - Uses estimate_path_tokens (ctx_tokens cl100k_base) for token counts
pub(crate) fn braid_build_pack_files(
    root: &Path,
    merged: &[ctx_braid::MergedFile],
) -> Result<Vec<NativePackFile>, String> {
    let mut pack_files = Vec::with_capacity(merged.len());
    for mf in merged {
        let abs = root.join(&mf.path);
        let content = match std::fs::read_to_string(&abs) {
            Ok(s) => s,
            Err(_) => continue, // skip files that cannot be read (mirrors Go byPath lookup failure)
        };
        let tokens = estimate_path_tokens(root, &mf.path);
        pack_files.push(NativePackFile {
            path: mf.path.clone(),
            abs_path: abs.to_string_lossy().into_owned(),
            content,
            tokens,
            score: 0,
            relevance: "selected".to_string(),
            reason: "braid".to_string(),
            symbols: Vec::new(),
        });
    }
    Ok(pack_files)
}

/// Render the pack body for a non-dry-run braid, mirroring Go's
/// pack.PackWithResult with NoMetadata=true, NoWarnings=true.
///
/// Output format (markdown, no metadata):
///   ## File contents\n
///   \n
///   ### {path}\n\n
///   ```{lang}\n{content}\n```\n
///   \n                          ← trailing blank line after every file (Go compat)
pub(crate) fn braid_render_pack_body(
    files: &[NativePackFile],
    args: &BraidArgs,
) -> Result<String, String> {
    let mut body = match args.format.as_str() {
        "markdown" | "xml" | "" => {
            // Go packMarkdown with NoMetadata=true, NoWarnings=true:
            // no header, no file-list, just "## File contents" + file bodies.
            // XML braid format still uses markdown for the pack body (packFormatFor
            // maps xml→FormatXML but Go's braid fmt=xml uses packFormatFor which
            // returns FormatXML → packXML which emits only a single summary line).
            // For "xml" format we mirror Go: pack body is the XML summary line.
            if args.format == "xml" {
                // Go packXML with NoMetadata produces just the tag line:
                // <context-pack goal="-" used="…" budget="…" files="…" />\n
                let used: i64 = files.iter().map(|f| f.tokens).sum();
                format!(
                    "<context-pack goal=\"-\" used=\"{}\" budget=\"{}\" files=\"{}\" />\n",
                    used,
                    args.budget,
                    files.len()
                )
            } else {
                braid_render_pack_markdown(files, args)
            }
        }
        "plain" => braid_render_pack_plain(files, args),
        "json" => braid_render_pack_json(files, args)?,
        other => return Err(format!("unknown --format value {other:?}")),
    };

    // Append contract manifest when enabled (mirrors Go packWithResult contract path).
    if args.contract && args.format != "xml" && args.format != "json" {
        let contract = build_native_pack_contract(files);
        match args.format.as_str() {
            "plain" => {
                let mut bytes = body.into_bytes();
                ctx_contract::embed::embed_plain(&mut bytes, &contract)
                    .map_err(|err| err.to_string())?;
                body = String::from_utf8(bytes).map_err(|err| err.to_string())?;
            }
            _ => {
                // markdown (default)
                let mut bytes = body.into_bytes();
                ctx_contract::embed::embed_markdown(&mut bytes, &contract)
                    .map_err(|err| err.to_string())?;
                body = String::from_utf8(bytes).map_err(|err| err.to_string())?;
            }
        }
    } else if args.contract && args.format == "json" {
        let contract = build_native_pack_contract(files);
        let patched = ctx_contract::embed::embed_json_patch(body.as_bytes(), &contract)
            .map_err(|err| err.to_string())?;
        body = String::from_utf8(patched).map_err(|err| err.to_string())?;
        if !body.ends_with('\n') {
            body.push('\n');
        }
    } else if args.contract && args.format == "xml" {
        let contract = build_native_pack_contract(files);
        let mut bytes = body.into_bytes();
        ctx_contract::embed::embed_xml(&mut bytes, &contract).map_err(|err| err.to_string())?;
        body = String::from_utf8(bytes).map_err(|err| err.to_string())?;
    }

    Ok(body)
}

/// Markdown pack body: ## File contents + one fenced block per file.
/// Mirrors Go's packMarkdown(NoMetadata=true, NoWarnings=true).
pub(crate) fn braid_render_pack_markdown(files: &[NativePackFile], _args: &BraidArgs) -> String {
    let mut out = String::new();
    out.push_str("## File contents\n");
    out.push('\n');
    for file in files {
        out.push_str(&format!("### {}\n\n", file.path));
        out.push_str(&format!("```{}\n", lang_for_path(&file.path)));
        out.push_str(&file.content);
        if !file.content.ends_with('\n') {
            out.push('\n');
        }
        out.push_str("```\n");
        out.push('\n'); // Go: fmt.Fprintln(w) after each file's closing ```
    }
    out
}

/// Plain pack body: mirrors Go's packPlain(NoMetadata=false …) but braid
/// calls it with NoMetadata=true meaning we get the file-listing variant.
/// Go packPlain with NoMetadata=true → no header, just "=== path ===\ncontent\n\n".
pub(crate) fn braid_render_pack_plain(files: &[NativePackFile], _args: &BraidArgs) -> String {
    // Go packPlain with PlainFileContents=false (default) and NoMetadata=false:
    //   Context Pack\nGoal: -\nBudget: X / Y tokens\nFiles: N\n\n
    //   + path  (N tokens)\n
    // But braid uses NoMetadata=true — Go still falls through to the plain
    // content path. Actually Go packPlain does not branch on NoMetadata;
    // it always emits the header. Let's mirror exactly:
    let used: i64 = files.iter().map(|f| f.tokens).sum();
    let mut out = format!(
        "Context Pack\nGoal: -\nBudget: {} / {} tokens\nFiles: {}\n\n",
        used,
        _args.budget,
        files.len()
    );
    for file in files {
        out.push_str(&format!("+ {}  ({} tokens)\n", file.path, file.tokens));
    }
    out
}

/// JSON pack body: mirrors Go's packJSON output structure.
pub(crate) fn braid_render_pack_json(
    files: &[NativePackFile],
    args: &BraidArgs,
) -> Result<String, String> {
    #[derive(serde::Serialize)]
    struct JsonFile {
        path: String,
        tokens: i64,
        relevance: String,
    }
    #[derive(serde::Serialize)]
    struct JsonPack {
        goal: String,
        used: i64,
        budget: i64,
        included: Vec<JsonFile>,
        skipped: Vec<JsonFile>,
        warnings: Vec<serde_json::Value>,
    }
    let used: i64 = files.iter().map(|f| f.tokens).sum();
    let payload = JsonPack {
        goal: "-".to_string(),
        used,
        budget: args.budget,
        included: files
            .iter()
            .map(|f| JsonFile {
                path: f.path.clone(),
                tokens: f.tokens,
                relevance: f.relevance.clone(),
            })
            .collect(),
        skipped: Vec::new(),
        warnings: Vec::new(),
    };
    let mut s = serde_json::to_string_pretty(&payload).map_err(|err| err.to_string())?;
    s.push('\n');
    Ok(s)
}

pub(crate) fn exec_braid_strand(
    root: &Path,
    strand: &ctx_braid::Strand,
) -> Result<BraidExecResult, String> {
    let source = ctx_braid::strand_subcommand(&strand.source);
    let tokens = ctx_braid::strip_ctx_and_sub(&strand.source)
        .map_err(|err| format!("braid: strand {:?}: {err}", strand.name))?;
    match source.as_str() {
        "where" => exec_braid_where(root, strand, &tokens),
        "focus" => exec_braid_focus(root, strand, &tokens),
        "digest" => exec_braid_digest(strand, &tokens),
        _ => Err(format!(
            "braid: strand {:?}: unsupported source {:?}",
            strand.name, source
        )),
    }
}

pub(crate) fn exec_braid_where(
    root: &Path,
    strand: &ctx_braid::Strand,
    args: &[String],
) -> Result<BraidExecResult, String> {
    let (query, flags) = parse_strand_positional_then_flags(args)?;
    let mut limit = 50_i64;
    let mut regex = String::new();
    let mut require_all = false;
    let mut context_n = 0_i64;
    let mut i = 0;
    while i < flags.len() {
        let arg = &flags[i];
        if let Some(value) = string_flag_value(arg, "--limit") {
            limit = value.parse().map_err(|err| {
                format!(
                    "braid: strand {:?}: parse strand source flags: {err}",
                    strand.name
                )
            })?;
        } else if arg == "--limit" {
            i += 1;
            limit = flags
                .get(i)
                .ok_or_else(|| {
                    format!(
                        "braid: strand {:?}: parse strand source flags: flag needs an argument: --limit",
                        strand.name
                    )
                })?
                .parse()
                .map_err(|err| {
                    format!(
                        "braid: strand {:?}: parse strand source flags: {err}",
                        strand.name
                    )
                })?;
        } else if let Some(value) = string_flag_value(arg, "--regex") {
            regex = value.to_string();
        } else if arg == "--regex" {
            i += 1;
            regex = flags.get(i).cloned().ok_or_else(|| {
                format!(
                    "braid: strand {:?}: parse strand source flags: flag needs an argument: --regex",
                    strand.name
                )
            })?;
        } else if arg == "--all" {
            require_all = true;
        } else if let Some(value) = string_flag_value(arg, "--context") {
            context_n = value.parse().map_err(|err| {
                format!(
                    "braid: strand {:?}: parse strand source flags: {err}",
                    strand.name
                )
            })?;
        } else if arg == "--context" {
            i += 1;
            context_n = flags
                .get(i)
                .ok_or_else(|| {
                    format!(
                        "braid: strand {:?}: parse strand source flags: flag needs an argument: --context",
                        strand.name
                    )
                })?
                .parse()
                .map_err(|err| {
                    format!(
                        "braid: strand {:?}: parse strand source flags: {err}",
                        strand.name
                    )
                })?;
        } else if string_flag_value(arg, "--format").is_some() {
        } else if arg == "--format" {
            i += 1;
            flags.get(i).ok_or_else(|| {
                format!(
                    "braid: strand {:?}: parse strand source flags: flag needs an argument: --format",
                    strand.name
                )
            })?;
        } else {
            return Err(format!(
                "braid: strand {:?}: parse strand source flags: flag provided but not defined: {arg}",
                strand.name
            ));
        }
        i += 1;
    }
    if !regex.is_empty() {
        regex::Regex::new(&regex)
            .map_err(|err| format!("braid: strand {:?}: invalid regex: {err}", strand.name))?;
    }
    let files = where_files(root)?;
    let results = ctx_where::search_with_options(
        &files,
        &query,
        &ctx_where::Options {
            limit,
            context_n,
            require_all,
            regex,
            synonyms: Default::default(),
            explain: false,
        },
    );
    Ok(BraidExecResult {
        paths: results.into_iter().map(|result| result.path).collect(),
    })
}

pub(crate) fn exec_braid_focus(
    root: &Path,
    strand: &ctx_braid::Strand,
    args: &[String],
) -> Result<BraidExecResult, String> {
    let (anchor, flags) = parse_strand_positional_then_flags(args)?;
    let mut hops = 1_i64;
    let mut i = 0;
    while i < flags.len() {
        let arg = &flags[i];
        if let Some(value) = string_flag_value(arg, "--hops") {
            hops = value.parse().map_err(|err| {
                format!(
                    "braid: strand {:?}: parse strand source flags: {err}",
                    strand.name
                )
            })?;
        } else if arg == "--hops" {
            i += 1;
            hops = flags
                .get(i)
                .ok_or_else(|| {
                    format!(
                        "braid: strand {:?}: parse strand source flags: flag needs an argument: --hops",
                        strand.name
                    )
                })?
                .parse()
                .map_err(|err| {
                    format!(
                        "braid: strand {:?}: parse strand source flags: {err}",
                        strand.name
                    )
                })?;
        } else if string_flag_value(arg, "--budget").is_some()
            || string_flag_value(arg, "--format").is_some()
        {
        } else if arg == "--budget" || arg == "--format" {
            i += 1;
            flags.get(i).ok_or_else(|| {
                format!(
                    "braid: strand {:?}: parse strand source flags: flag needs an argument: {arg}",
                    strand.name
                )
            })?;
        } else {
            return Err(format!(
                "braid: strand {:?}: parse strand source flags: flag provided but not defined: {arg}",
                strand.name
            ));
        }
        i += 1;
    }
    if anchor.is_empty() {
        return Err(format!(
            "braid: strand {:?}: focus requires an anchor argument",
            strand.name
        ));
    }
    let files = focus_files(root)?;
    let anchor_info = match ctx_focus::resolve_anchor(&files, &anchor) {
        Ok(anchor) => anchor,
        Err(err) if err.candidates.len() > 1 => {
            let names: Vec<_> = err
                .candidates
                .into_iter()
                .map(|candidate| format!("{}:{}", candidate.path, candidate.line))
                .collect();
            return Err(format!(
                "braid: strand {:?}: ambiguous anchor {:?} ({})",
                strand.name,
                anchor,
                names.join(", ")
            ));
        }
        Err(_) => {
            return Err(format!(
                "braid: strand {:?}: focus: anchor {:?} not found",
                strand.name, anchor
            ))
        }
    };
    let expanded = ctx_focus::expand(&files, &anchor_info, &ctx_focus::ExpandOptions { hops });
    Ok(BraidExecResult {
        paths: expanded.into_iter().map(|file| file.path).collect(),
    })
}

pub(crate) fn exec_braid_digest(
    strand: &ctx_braid::Strand,
    args: &[String],
) -> Result<BraidExecResult, String> {
    let (_, flags) = parse_strand_positional_then_flags(args)?;
    let mut since = "7d".to_string();
    let mut top = 50_usize;
    let mut i = 0;
    while i < flags.len() {
        let arg = &flags[i];
        if let Some(value) = string_flag_value(arg, "--since") {
            since = value.to_string();
        } else if arg == "--since" {
            i += 1;
            since = flags.get(i).cloned().ok_or_else(|| {
                format!(
                    "braid: strand {:?}: parse strand source flags: flag needs an argument: --since",
                    strand.name
                )
            })?;
        } else if let Some(value) = string_flag_value(arg, "--top") {
            top = value.parse().map_err(|err| {
                format!(
                    "braid: strand {:?}: parse strand source flags: {err}",
                    strand.name
                )
            })?;
        } else if arg == "--top" {
            i += 1;
            top = flags
                .get(i)
                .ok_or_else(|| {
                    format!(
                        "braid: strand {:?}: parse strand source flags: flag needs an argument: --top",
                        strand.name
                    )
                })?
                .parse()
                .map_err(|err| {
                    format!(
                        "braid: strand {:?}: parse strand source flags: {err}",
                        strand.name
                    )
                })?;
        } else if string_flag_value(arg, "--format").is_some() {
        } else if arg == "--format" {
            i += 1;
            flags.get(i).ok_or_else(|| {
                format!(
                    "braid: strand {:?}: parse strand source flags: flag needs an argument: --format",
                    strand.name
                )
            })?;
        } else {
            return Err(format!(
                "braid: strand {:?}: parse strand source flags: flag provided but not defined: {arg}",
                strand.name
            ));
        }
        i += 1;
    }
    let digest = generate_digest(&since, top)
        .map_err(|err| format!("braid: strand {:?}: digest: {err}", strand.name))?;
    let mut files = digest.hot_files;
    files.sort_by(|a, b| b.commits.cmp(&a.commits).then_with(|| a.path.cmp(&b.path)));
    Ok(BraidExecResult {
        paths: files.into_iter().map(|file| file.path).collect(),
    })
}

pub(crate) fn parse_strand_positional_then_flags(
    args: &[String],
) -> Result<(String, Vec<String>), String> {
    let mut positional = String::new();
    let mut flags = Vec::new();
    let mut seen_positional = false;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if !arg.starts_with('-') {
            if !seen_positional {
                positional = arg.clone();
                seen_positional = true;
                i += 1;
                continue;
            }
            flags.push(arg.clone());
            i += 1;
            continue;
        }
        flags.push(arg.clone());
        if !arg.contains('=') && i + 1 < args.len() && !args[i + 1].starts_with('-') {
            i += 1;
            flags.push(args[i].clone());
        }
        i += 1;
    }
    Ok((positional, flags))
}

pub(crate) fn string_flag_value<'a>(arg: &'a str, name: &str) -> Option<&'a str> {
    arg.strip_prefix(name)
        .and_then(|rest| rest.strip_prefix('='))
}

pub(crate) fn estimate_path_tokens(root: &Path, path: &str) -> i64 {
    let path_only = strip_line_range(path);
    let abs = root.join(path_only);
    let abs_str = abs.to_string_lossy();
    match ctx_tokens::count_file(&abs_str) {
        Ok(n) => n.max(1),
        Err(_) => {
            // Fall back to size estimate when file is unreadable or not UTF-8.
            // Mirrors Go's EstimateBySize fallback in braid/mcp token-count closures.
            match std::fs::metadata(&abs) {
                Ok(meta) => ctx_tokens::estimate_by_size(meta.len() as i64),
                Err(_) => 0,
            }
        }
    }
}

pub(crate) fn strip_line_range(path: &str) -> &str {
    let Some((prefix, suffix)) = path.rsplit_once(':') else {
        return path;
    };
    if suffix
        .split_once('-')
        .map(|(start, end)| {
            !start.is_empty()
                && !end.is_empty()
                && start.chars().all(|ch| ch.is_ascii_digit())
                && end.chars().all(|ch| ch.is_ascii_digit())
        })
        .unwrap_or_else(|| suffix.chars().all(|ch| ch.is_ascii_digit()))
    {
        prefix
    } else {
        path
    }
}

pub(crate) fn is_braid_validation_error(err: &str) -> bool {
    err.starts_with("braid: ")
        && (err.contains("unsupported source")
            || err.contains("schema_version")
            || err.contains("share must be in")
            || err.contains("duplicate strand name")
            || err.contains("unknown policy")
            || err.contains("is required")
            || err.starts_with("braid: at least one"))
}
