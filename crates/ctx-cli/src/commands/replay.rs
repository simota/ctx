use std::ffi::{OsStr, OsString};
use std::io;
use std::process::ExitCode;

use crate::common::*;

#[derive(Debug)]
pub(crate) struct ReplayArgs {
    command: ReplayCommand,
    shared: bool,
    json: bool,
}

#[derive(Debug)]
pub(crate) enum ReplayCommand {
    List,
    Show {
        id: String,
    },
    Prune {
        older_than: String,
    },
    Diff {
        a: String,
        b: String,
        by: String,
        format: String,
    },
}

pub(crate) fn run_replay_command(args: &[OsString]) -> Option<ExitCode> {
    let parsed = parse_replay_args(args)?;
    match replay_command(parsed) {
        Ok(()) => Some(ExitCode::SUCCESS),
        Err(err) => {
            eprintln!("{err}");
            Some(ExitCode::from(1))
        }
    }
}

pub(crate) fn parse_replay_args(args: &[OsString]) -> Option<ReplayArgs> {
    let mut saw_replay = false;
    let mut shared = false;
    let mut json = false;
    let mut subcommand: Option<String> = None;
    let mut positionals = Vec::new();
    let mut older_than = String::new();
    let mut by = "tier".to_string();
    let mut format = "markdown".to_string();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == OsStr::new("--json") {
            json = true;
        } else if arg == OsStr::new("--shared") {
            shared = true;
        } else if arg == OsStr::new("replay") {
            if saw_replay {
                return None;
            }
            saw_replay = true;
        } else if let Some(value) = flag_value(arg, "--older-than") {
            older_than = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--older-than") {
            i += 1;
            older_than = args.get(i)?.to_string_lossy().into_owned();
        } else if let Some(value) = flag_value(arg, "--by") {
            by = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--by") {
            i += 1;
            by = args.get(i)?.to_string_lossy().into_owned();
        } else if let Some(value) = flag_value(arg, "--format") {
            format = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--format") {
            i += 1;
            format = args.get(i)?.to_string_lossy().into_owned();
        } else if flag_value(arg, "--replay-engine").is_some() {
        } else if arg == OsStr::new("--replay-engine") {
            i += 1;
            args.get(i)?;
        } else if is_option(arg) {
            return None;
        } else if saw_replay && subcommand.is_none() {
            subcommand = Some(arg.to_string_lossy().into_owned());
        } else if saw_replay {
            positionals.push(arg.clone());
        } else {
            return None;
        }
        i += 1;
    }
    let command = match subcommand.as_deref()? {
        "list" => {
            if !positionals.is_empty() {
                return None;
            }
            ReplayCommand::List
        }
        "show" => match positionals.as_slice() {
            [id] => ReplayCommand::Show {
                id: id.to_string_lossy().into_owned(),
            },
            _ => return None,
        },
        "prune" => {
            if !positionals.is_empty() || older_than.is_empty() {
                return None;
            }
            ReplayCommand::Prune { older_than }
        }
        "diff" => match positionals.as_slice() {
            [a, b] => ReplayCommand::Diff {
                a: a.to_string_lossy().into_owned(),
                b: b.to_string_lossy().into_owned(),
                by,
                format,
            },
            _ => return None,
        },
        _ => return None,
    };
    Some(ReplayArgs {
        command,
        shared,
        json,
    })
}

pub(crate) fn replay_command(args: ReplayArgs) -> Result<(), String> {
    let store = open_replay_store(args.shared)?;
    match args.command {
        ReplayCommand::List => {
            let manifests = store.list().map_err(|err| err.to_string())?;
            if args.json {
                serde_json::to_writer_pretty(io::stdout(), &manifests)
                    .map_err(|err| err.to_string())?;
                println!();
                return Ok(());
            }
            // Mirror Go's tabwriter.NewWriter(w, 0, 0, 2, ' ', 0):
            // build all rows first, then emit with column-aligned spacing.
            let mut rows: Vec<[String; 5]> = Vec::new();
            rows.push([
                "ID".to_string(),
                "CREATED".to_string(),
                "GOAL".to_string(),
                "FILES".to_string(),
                "TOKENS".to_string(),
            ]);
            for m in manifests {
                let goal = if m.goal.is_empty() {
                    "-".to_string()
                } else {
                    m.goal.clone()
                };
                rows.push([
                    m.id.clone(),
                    display_replay_created_at(&m.created_at),
                    goal,
                    m.entries.len().to_string(),
                    m.used.to_string(),
                ]);
            }
            print!("{}", tabwriter_format(&rows, 2));
        }
        ReplayCommand::Show { id } => {
            let manifest = store.load(&id).map_err(|err| err.to_string())?;
            serde_json::to_writer_pretty(io::stdout(), &manifest).map_err(|err| err.to_string())?;
            println!();
        }
        ReplayCommand::Prune { older_than } => {
            let older = ctx_replay::parse_duration(&older_than)?;
            let now = rfc3339_now_utc();
            let result = ctx_replay::prune(&store, &now, older).map_err(|err| err.to_string())?;
            if args.json {
                // Mirror Go: empty deleted slice serialises as null (Go []string nil → JSON null).
                let deleted_json = if result.deleted.is_empty() {
                    serde_json::Value::Null
                } else {
                    serde_json::Value::Array(
                        result
                            .deleted
                            .iter()
                            .map(|s| serde_json::Value::String(s.clone()))
                            .collect(),
                    )
                };
                serde_json::to_writer_pretty(
                    io::stdout(),
                    &serde_json::json!({
                        "deleted": deleted_json,
                        "kept": result.kept,
                    }),
                )
                .map_err(|err| err.to_string())?;
                println!();
            } else if result.deleted.is_empty() {
                println!("no snapshots pruned");
            } else {
                println!("Pruned {} snapshot(s):", result.deleted.len());
                for id in result.deleted {
                    println!("  - {id}");
                }
            }
        }
        ReplayCommand::Diff { a, b, by, format } => {
            let a_manifest = store
                .load(&a)
                .map_err(|err| format!("manifest {a}: {err}"))?;
            let b_manifest = store
                .load(&b)
                .map_err(|err| format!("manifest {b}: {err}"))?;
            let mut summary = ctx_replay::compute_selection_diff(&a_manifest, &b_manifest);
            ctx_replay::sort_selection_diff(&mut summary, &by);
            // Go's diff command only checks --format, NOT the global --json flag.
            // The --json flag affects list/prune but NOT diff (diff has its own
            // --format flag that controls its output mode).
            if format == "json" {
                serde_json::to_writer_pretty(io::stdout(), &summary)
                    .map_err(|err| err.to_string())?;
                println!();
            } else {
                print!("{}", write_replay_diff_markdown(&summary));
            }
        }
    }
    Ok(())
}

/// Mirrors Go's tabwriter.NewWriter(w, 0, 0, padding, ' ', 0).
/// Formats rows as tab-separated columns, padding each column to the maximum
/// RUNE (Unicode character) width in that column plus `padding` spaces.
/// Go's tabwriter counts runes, not bytes, so we must do the same to match
/// byte-for-byte on cells that contain multi-byte UTF-8 characters (e.g. →).
pub(crate) fn tabwriter_format(rows: &[[String; 5]], padding: usize) -> String {
    let ncols = 5;
    // Compute column widths in RUNE count (not bytes), matching Go tabwriter.
    let mut widths = vec![0usize; ncols];
    for row in rows {
        for (j, cell) in row.iter().enumerate() {
            widths[j] = widths[j].max(cell.chars().count());
        }
    }
    let mut out = String::new();
    for row in rows {
        for (j, cell) in row.iter().enumerate() {
            out.push_str(cell);
            if j < ncols - 1 {
                // pad = (column rune width - cell rune width) + padding spaces
                let cell_runes = cell.chars().count();
                let pad = widths[j] - cell_runes + padding;
                for _ in 0..pad {
                    out.push(' ');
                }
            }
        }
        out.push('\n');
    }
    out
}

/// Mirrors Go's `replay.WriteSelectionDiffMarkdown` using tabwriter (padding=2).
///
/// Go uses `tabwriter.NewWriter(w, 0, 0, 2, ' ', 0)`. Each line in Go is
/// formatted as `"| Col0\t| Col1\t| Col2\t| Col3\t|\n"` — five tab-separated
/// segments: the four content columns and a trailing `|`. We replicate this
/// with a 5-column tabwriter pass per section.
pub(crate) fn write_replay_diff_markdown(s: &ctx_replay::SelectionSummary) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Replay Diff — {} → {}\n\n", s.a, s.b));

    if !s.changes.added.is_empty() {
        let token_sum: i64 = s.changes.added.iter().map(|c| c.cur_tokens).sum();
        out.push_str(&format!(
            "**Added** ({} file(s), +{} tokens)\n",
            s.changes.added.len(),
            token_sum
        ));
        let mut rows: Vec<[String; 5]> = vec![
            [
                "| Path".into(),
                "| Score".into(),
                "| Tier".into(),
                "| Tokens".into(),
                "|".into(),
            ],
            [
                "|---".into(),
                "|---".into(),
                "|---".into(),
                "|---".into(),
                "|".into(),
            ],
        ];
        for c in &s.changes.added {
            let tier = if c.cur_tier.is_empty() {
                "-".to_string()
            } else {
                c.cur_tier.clone()
            };
            rows.push([
                format!("| {}", c.path),
                format!("| {}", c.cur_score),
                format!("| {}", tier),
                format!("| {}", c.cur_tokens),
                "|".into(),
            ]);
        }
        out.push_str(&tabwriter_format(&rows, 2));
        out.push('\n');
    }

    if !s.changes.promoted.is_empty() {
        out.push_str(&format!(
            "**Promoted** ({} file(s))\n",
            s.changes.promoted.len()
        ));
        let mut rows: Vec<[String; 5]> = vec![
            [
                "| Path".into(),
                "| Score A\u{2192}B".into(),
                "| Tier A\u{2192}B".into(),
                "| Tokens".into(),
                "|".into(),
            ],
            [
                "|---".into(),
                "|---".into(),
                "|---".into(),
                "|---".into(),
                "|".into(),
            ],
        ];
        for c in &s.changes.promoted {
            rows.push([
                format!("| {}", c.path),
                format!("| {} \u{2192} {}", c.base_score, c.cur_score),
                format!("| {} \u{2192} {}", c.base_tier, c.cur_tier),
                format!("| {}", c.cur_tokens),
                "|".into(),
            ]);
        }
        out.push_str(&tabwriter_format(&rows, 2));
        out.push('\n');
    }

    if !s.changes.demoted.is_empty() {
        out.push_str(&format!(
            "**Demoted** ({} file(s))\n",
            s.changes.demoted.len()
        ));
        let mut rows: Vec<[String; 5]> = vec![
            [
                "| Path".into(),
                "| Score A\u{2192}B".into(),
                "| Tier A\u{2192}B".into(),
                "| Tokens".into(),
                "|".into(),
            ],
            [
                "|---".into(),
                "|---".into(),
                "|---".into(),
                "|---".into(),
                "|".into(),
            ],
        ];
        for c in &s.changes.demoted {
            rows.push([
                format!("| {}", c.path),
                format!("| {} \u{2192} {}", c.base_score, c.cur_score),
                format!("| {} \u{2192} {}", c.base_tier, c.cur_tier),
                format!("| {}", c.cur_tokens),
                "|".into(),
            ]);
        }
        out.push_str(&tabwriter_format(&rows, 2));
        out.push('\n');
    }

    if !s.changes.removed.is_empty() {
        let token_sum: i64 = s.changes.removed.iter().map(|c| c.base_tokens).sum();
        out.push_str(&format!(
            "**Removed** ({} file(s), -{} tokens)\n",
            s.changes.removed.len(),
            token_sum
        ));
        let mut rows: Vec<[String; 5]> = vec![
            [
                "| Path".into(),
                "| Score".into(),
                "| Tier".into(),
                "| Tokens".into(),
                "|".into(),
            ],
            [
                "|---".into(),
                "|---".into(),
                "|---".into(),
                "|---".into(),
                "|".into(),
            ],
        ];
        for c in &s.changes.removed {
            let tier = if c.base_tier.is_empty() {
                "-".to_string()
            } else {
                c.base_tier.clone()
            };
            rows.push([
                format!("| {}", c.path),
                format!("| {}", c.base_score),
                format!("| {}", tier),
                format!("| {}", c.base_tokens),
                "|".into(),
            ]);
        }
        out.push_str(&tabwriter_format(&rows, 2));
        out.push('\n');
    }

    if !s.changes.score_changed.is_empty() {
        out.push_str(&format!(
            "**Score Changed (same tier)** ({} file(s))\n",
            s.changes.score_changed.len()
        ));
        let mut rows: Vec<[String; 5]> = vec![
            [
                "| Path".into(),
                "| Score A\u{2192}B".into(),
                "| Tier".into(),
                "| Reason".into(),
                "|".into(),
            ],
            [
                "|---".into(),
                "|---".into(),
                "|---".into(),
                "|---".into(),
                "|".into(),
            ],
        ];
        for c in &s.changes.score_changed {
            let tier = if c.cur_tier.is_empty() {
                "-".to_string()
            } else {
                c.cur_tier.clone()
            };
            rows.push([
                format!("| {}", c.path),
                format!("| {} \u{2192} {}", c.base_score, c.cur_score),
                format!("| {}", tier),
                format!("| {}", c.reason_change),
                "|".into(),
            ]);
        }
        out.push_str(&tabwriter_format(&rows, 2));
        out.push('\n');
    }

    if s.summary.added == 0
        && s.summary.removed == 0
        && s.summary.promoted == 0
        && s.summary.demoted == 0
        && s.summary.score_changed == 0
    {
        out.push_str("_No selection changes between the two snapshots._\n");
    }

    out
}

pub(crate) fn open_replay_store(shared: bool) -> Result<ctx_replay::Store, String> {
    let dir = ctx_replay::resolve(ctx_replay::ResolveOptions {
        shared,
        root: ".".to_string(),
    })
    .map_err(|err| err.to_string())?;
    ctx_replay::open_store(&dir).map_err(|err| err.to_string())
}

pub(crate) fn display_replay_created_at(raw: &str) -> String {
    raw.strip_suffix('Z')
        .and_then(|s| s.split_once('T'))
        .map(|(date, time)| format!("{} {}", date, time.split('.').next().unwrap_or(time)))
        .unwrap_or_else(|| raw.to_string())
}
