use std::env;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::commands::pack::*;
use crate::commands::where_cmd::where_files;
use crate::common::*;
use serde::Serialize;

#[derive(Debug)]
pub(crate) struct FocusArgs {
    anchor: String,
    hops: i64,
    budget: i64,
    format: String,
}

/// Focus command error variants, mirroring Go's exit behaviour:
/// - Ambiguous: manually printed to stderr + cobra-empty trailer (ExitError{Code:1}).
/// - NotFound:  cobra prints "Error: <msg>" + main prints "<msg>" again (standard RunE return).
/// - Other:     simple eprintln (internal errors).
pub(crate) enum FocusError {
    /// Anchor matches multiple definitions — printed before returning ExitError{Code:1}.
    /// Go's cobra then adds "Error: \n" (empty ExitError.Error()).
    Ambiguous(String),
    /// Anchor not resolved. Go returns fmt.Errorf(...) from RunE so cobra prints
    /// "Error: <msg>\n" and main.go prints it again.
    NotFound(String),
    /// Generic internal error.
    Other(String),
}

pub(crate) fn run_focus_command(args: &[OsString]) -> Option<ExitCode> {
    let parsed = parse_focus_args(args)?;
    match focus_command(parsed) {
        Ok(()) => Some(ExitCode::SUCCESS),
        Err(FocusError::Ambiguous(msg)) => {
            // Mirror Go: print the detailed message to stderr, then the cobra empty trailer.
            eprint!("{msg}");
            print_cobra_empty_error();
            Some(ExitCode::from(1))
        }
        Err(FocusError::NotFound(msg)) => {
            // Mirror Go's RunE + main.go double-print:
            //   cobra: "Error: <msg>\n"
            //   main:  "<msg>\n" (because err.Error() != "")
            eprintln!("Error: {msg}");
            eprintln!("{msg}");
            Some(ExitCode::from(1))
        }
        Err(FocusError::Other(err)) => {
            eprintln!("{err}");
            Some(ExitCode::from(1))
        }
    }
}

pub(crate) fn parse_focus_args(args: &[OsString]) -> Option<FocusArgs> {
    let mut saw_focus = false;
    let mut json = false;
    let mut plain = false;
    let mut hops = 1;
    let mut budget = 8000;
    let mut format = "markdown".to_string();
    let mut positionals = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == OsStr::new("--json") {
            json = true;
        } else if arg == OsStr::new("--plain") {
            plain = true;
        } else if arg == OsStr::new("focus") {
            if saw_focus {
                return None;
            }
            saw_focus = true;
        } else if let Some(value) = flag_value(arg, "--hops") {
            hops = value.to_string_lossy().parse().ok()?;
        } else if arg == OsStr::new("--hops") {
            i += 1;
            hops = args.get(i)?.to_string_lossy().parse().ok()?;
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
        } else if flag_value(arg, "--focus-engine").is_some() {
        } else if arg == OsStr::new("--focus-engine") {
            i += 1;
            args.get(i)?;
        } else if is_option(arg) {
            return None;
        } else if saw_focus {
            positionals.push(arg.clone());
        } else {
            return None;
        }
        i += 1;
    }
    if json {
        format = "json".to_string();
    } else if plain && format == "markdown" {
        format = "plain".to_string();
    }
    Some(FocusArgs {
        anchor: match positionals.as_slice() {
            [anchor] => anchor.to_string_lossy().into_owned(),
            _ => return None,
        },
        hops,
        budget,
        format,
    })
}

pub(crate) fn focus_command(args: FocusArgs) -> Result<(), FocusError> {
    let root = env::current_dir().map_err(|err| {
        FocusError::Other(format!("focus: cannot determine working directory: {err}"))
    })?;
    let files = focus_files(&root).map_err(FocusError::Other)?;
    let anchor = match ctx_focus::resolve_anchor(&files, &args.anchor) {
        Ok(anchor) => anchor,
        Err(err) if err.candidates.len() > 1 => {
            let mut msg = format!(
                "error: anchor {:?} matches multiple definitions:\n",
                args.anchor
            );
            for c in err.candidates {
                msg.push_str(&format!("  - {}:{} ({})\n", c.path, c.line, c.kind));
            }
            // Note: Go appends a newline after hint via fmt.Fprintf, not via the hint string itself.
            msg.push_str(
                "hint: pass a more specific anchor like \"pack.go\" or \"internal/pack/pack.go\"\n",
            );
            return Err(FocusError::Ambiguous(msg));
        }
        Err(_) => {
            // Mirror Go's fmt.Errorf("anchor %q not found (tried symbol, basename, path)", raw)
            return Err(FocusError::NotFound(format!(
                "anchor {:?} not found (tried symbol, basename, path)",
                args.anchor
            )));
        }
    };
    let expanded = ctx_focus::expand(
        &files,
        &anchor,
        &ctx_focus::ExpandOptions { hops: args.hops },
    );

    // Build included list: apply budget filter, read raw file bytes from disk,
    // and score relevance using pack's scorer (empty goal → "score 3: source file"
    // for .go files, matching Go's pack.buildPlan behaviour).
    let ctx = ctx_pack::RelevanceContext::new("", args.budget);
    let mut included: Vec<NativePackFile> = Vec::new();
    let mut used = 0_i64;
    for fi in expanded {
        let Some(input) = files.iter().find(|item| item.path == fi.path) else {
            continue;
        };
        let tokens = estimate_focus_tokens(&root, input);
        if args.budget > 0 && used + tokens > args.budget {
            continue;
        }
        used += tokens;

        // Read raw file bytes from disk (matches Go's os.ReadFile in fileContent).
        let abs_path = root.join(&input.path);
        let content = std::fs::read_to_string(&abs_path).unwrap_or_else(|_| input.lines.join("\n"));

        // Score via pack relevance (empty goal) to match Go's buildPlan output.
        let pack_input = ctx_pack::FileInput {
            path: input.path.clone(),
            abs_path: abs_path.to_string_lossy().into_owned(),
            is_dir: false,
            tokens,
            role: String::new(),
            metadata: ctx_pack::MetadataInput {
                size: 0,
                tokens_est: tokens,
                role: String::new(),
                symbols: input
                    .symbols
                    .iter()
                    .map(|sym| ctx_pack::SymbolInput {
                        name: sym.name.clone(),
                        kind: sym.kind.clone(),
                        line: sym.line,
                    })
                    .collect(),
            },
            content_head: content.as_bytes().iter().take(512).copied().collect(),
        };
        let scored = ctx_pack::relevance::score_relevance_with_ctx(&pack_input, &ctx, tokens);
        let symbols: Vec<String> = input.symbols.iter().map(|sym| sym.name.clone()).collect();
        included.push(NativePackFile {
            path: input.path.clone(),
            abs_path: abs_path.to_string_lossy().into_owned(),
            content,
            tokens,
            score: scored.score,
            relevance: if scored.tier.is_empty() {
                "Medium".to_string()
            } else {
                scored.tier
            },
            reason: scored.reason,
            symbols,
        });
    }

    // Emit meta line (matches Go's fmt.Fprintf exactly).
    println!(
        "# anchor={} origin={} hops={} files={} tokens={}/{}",
        args.anchor,
        anchor.origin_path,
        args.hops,
        included.len(),
        used,
        args.budget
    );

    // Render the pack body. JSON uses a special path to match Go's packJSON
    // exactly: compact (single-line) encoding + null for empty slices (Go nil).
    // All other formats use render_native_pack with NoMetadata=true, NoContract=true.
    if args.format == "json" {
        // Mirror Go packJSON: json.NewEncoder → compact single-line, nil slices → null.
        // Empty Vec in Go serialises as null (nil slice); we use Option<Vec<_>> for parity.
        #[derive(Serialize)]
        struct FocusJsonFile {
            path: String,
            tokens: i64,
            relevance: String,
            #[serde(skip_serializing_if = "String::is_empty")]
            reason: String,
        }
        #[derive(Serialize)]
        struct FocusJson {
            goal: &'static str,
            used: i64,
            budget: i64,
            // Go nil slice → JSON null; non-empty slice → JSON array.
            included: Option<Vec<FocusJsonFile>>,
            skipped: Option<()>,
            warnings: Option<()>,
        }
        let included_vec: Vec<FocusJsonFile> = included
            .iter()
            .map(|f| FocusJsonFile {
                path: f.path.clone(),
                tokens: f.tokens,
                relevance: f.relevance.clone(),
                reason: f.reason.clone(),
            })
            .collect();
        let payload = FocusJson {
            goal: "-",
            used,
            budget: args.budget,
            included: if included_vec.is_empty() {
                None
            } else {
                Some(included_vec)
            },
            skipped: None,
            warnings: None,
        };
        let json_str =
            serde_json::to_string(&payload).map_err(|err| FocusError::Other(err.to_string()))?;
        println!("{json_str}");
    } else {
        let pack_args = build_focus_pack_args(&args, args.budget);
        let rendered =
            render_native_pack(&pack_args, &included, None).map_err(FocusError::Other)?;
        print!("{rendered}");
    }
    Ok(())
}

/// Build a minimal PackArgs for focus rendering: no metadata, no contract,
/// empty goal (renders as "-"), matching Go's pack.Options in focus.go.
pub(crate) fn build_focus_pack_args(args: &FocusArgs, budget: i64) -> PackArgs {
    PackArgs {
        root: PathBuf::from("."),
        from_where: false,
        from_stdin: false,
        diff_spec: String::new(),
        since: String::new(),
        until: String::new(),
        use_mtime: false,
        budget,
        format: args.format.clone(),
        out: String::new(),
        goal: String::new(),
        no_contract: true,
        no_warnings: true,
        no_paths: false,
        no_metadata: true,
        frontmatter: String::new(),
        plain_file_contents: false,
        explain: false,
        preset: String::new(),
        changed: false,
        api_only: false,
        layout: "sequential".to_string(),
        from_mix: String::new(),
        why_paths: Vec::new(),
        snapshot_id: String::new(),
        since_snapshot: String::new(),
        replay_shared: false,
        replay_strict: false,
        format_changed: false,
        goal_changed: false,
        budget_changed: false,
        preset_changed: false,
        no_warnings_changed: false,
        no_paths_changed: false,
        no_metadata_changed: false,
        frontmatter_changed: false,
    }
}

pub(crate) fn focus_files(root: &Path) -> Result<Vec<ctx_focus::FileInput>, String> {
    let where_inputs = where_files(root)?;
    Ok(where_inputs
        .into_iter()
        .map(|fi| ctx_focus::FileInput {
            path: fi.path,
            is_dir: fi.is_dir,
            symbols: fi
                .symbols
                .into_iter()
                .map(|sym| ctx_focus::SymbolInfo {
                    name: sym.name,
                    kind: sym.kind,
                    line: sym.line,
                })
                .collect(),
            lines: fi.lines,
        })
        .collect())
}

pub(crate) fn estimate_focus_tokens(root: &Path, input: &ctx_focus::FileInput) -> i64 {
    // Mirror Go's tokens.CountFile on the raw file (see estimate_where_tokens).
    let abs = root.join(&input.path);
    let abs_str = abs.to_string_lossy();
    match ctx_tokens::count_file(&abs_str) {
        Ok(n) => n.max(1),
        Err(_) => ctx_tokens::count_str(&input.lines.join("\n")).max(1),
    }
}
