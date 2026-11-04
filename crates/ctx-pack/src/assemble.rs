// crates/ctx-pack/src/assemble.rs
//
// Pack assembly + rendering — the Rust equivalent of Go's
// internal/pack `packMarkdown` / `packJSON` / `packPlain` / `packXML`
// renderers plus the explicit-file-set planner used by Go's
// `pack.Pack(w, files, Options)`.
//
// HISTORY: this code was moved VERBATIM from ctx-cli's
// commands/pack/render.rs (render_native_pack + helpers) so that both
// the `ctx pack` CLI (byte-parity-gated against the Go oracle) and the
// ctx-tui `p` action can share one renderer. Byte-parity comments from
// the original are preserved — do not "clean them up".

use serde::Serialize;

use crate::relevance::score_relevance_with_ctx;
use crate::types::{FileInput, ScoreBreakdown};
use crate::RelevanceContext;

/// Rendering options — the renderer-facing subset of ctx-cli's PackArgs
/// (and of Go's pack.Options). NOTE: ctx-cli's CLI flag is `--no-contract`
/// (args.no_contract); callers must map `contract = !args.no_contract`.
#[derive(Debug, Clone)]
pub struct RenderOptions {
    pub format: String,
    pub goal: String,
    pub budget: i64,
    pub explain: bool,
    pub no_metadata: bool,
    pub no_paths: bool,
    pub frontmatter: String,
    pub plain_file_contents: bool,
    /// When true, a `ctx:contract v1` manifest is embedded into the
    /// output (Go: Options.Contract).
    pub contract: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        RenderOptions {
            format: "markdown".to_string(),
            goal: String::new(),
            budget: 0,
            explain: false,
            no_metadata: false,
            no_paths: false,
            frontmatter: String::new(),
            plain_file_contents: false,
            contract: false,
        }
    }
}

#[derive(Debug, Serialize)]
struct PackJson {
    goal: String,
    used: i64,
    budget: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    replay: Option<ReplayHeader>,
    included: Vec<PackJsonFile>,
    skipped: Vec<PackJsonSkipped>,
    warnings: Vec<PackWarning>,
}

/// Replay narrowing header (Go: pack.ReplayHeader). Serialised with
/// `base` as the JSON key for the base snapshot id.
#[derive(Debug, Clone, Serialize)]
pub struct ReplayHeader {
    #[serde(rename = "base")]
    pub base_id: String,
    pub added: i64,
    pub modified: i64,
    pub removed: i64,
    pub token_delta: i64,
}

#[derive(Debug, Serialize)]
struct PackJsonFile {
    path: String,
    tokens: i64,
    relevance: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    reason: String,
}

#[derive(Debug, Serialize)]
struct PackJsonSkipped {
    path: String,
    reason: String,
}

#[derive(Debug, Serialize)]
struct PackWarning {
    kind: String,
    severity: String,
    message: String,
}

pub fn is_zero_i64(value: &i64) -> bool {
    *value == 0
}

pub fn is_default_breakdown(value: &ScoreBreakdown) -> bool {
    value.basename == 0
        && value.path == 0
        && value.symbol == 0
        && value.content == 0
        && value.role == 0
}

/// A planned, content-loaded file ready for rendering (Go: the
/// candidate/IncludedFile pair the renderers consume).
#[derive(Debug)]
pub struct PackFile {
    pub path: String,
    pub abs_path: String,
    pub content: String,
    pub tokens: i64,
    pub score: i64,
    pub relevance: String,
    pub reason: String,
    pub symbols: Vec<String>,
}

/// Render a pack in the requested format, optionally embedding a
/// `ctx:contract v1` manifest. Verbatim move of ctx-cli's
/// render_native_pack — output is byte-parity-gated for the CLI.
pub fn render(
    opts: &RenderOptions,
    files: &[PackFile],
    replay_header: Option<&ReplayHeader>,
) -> Result<String, String> {
    let mut out: String = match opts.format.as_str() {
        "json" => {
            let payload = PackJson {
                goal: if opts.goal.is_empty() {
                    "-".to_string()
                } else {
                    opts.goal.clone()
                },
                used: files.iter().map(|file| file.tokens).sum(),
                budget: opts.budget,
                replay: replay_header.cloned(),
                included: files
                    .iter()
                    .map(|file| PackJsonFile {
                        path: file.path.clone(),
                        tokens: file.tokens,
                        relevance: file.relevance.clone(),
                        reason: file.reason.clone(),
                    })
                    .collect(),
                skipped: Vec::new(),
                warnings: Vec::new(),
            };
            let mut out = serde_json::to_string_pretty(&payload).map_err(|err| err.to_string())?;
            out.push('\n');
            Ok(out)
        }
        "plain" => {
            let mut out = String::new();
            append_replay_header(&mut out, replay_header);
            if opts.plain_file_contents {
                for file in files {
                    out.push_str(&format!("=== {} ===\n", file.path));
                    out.push_str(&file.content);
                    if !out.ends_with('\n') {
                        out.push('\n');
                    }
                    out.push('\n');
                }
            } else {
                out.push_str(&format!(
                    "Context Pack\nGoal: {}\nBudget: {} / {} tokens\nFiles: {}\n\n",
                    value_or_dash(&opts.goal),
                    files.iter().map(|file| file.tokens).sum::<i64>(),
                    opts.budget,
                    files.len()
                ));
                for file in files {
                    out.push_str(&format!("+ {}  ({} tokens)\n", file.path, file.tokens));
                }
                // No trailing blank line — mirrors Go's packPlain which ends with the
                // last "+ path  (N tokens)\n" and no additional fmt.Fprintln(w).
            }
            Ok(out)
        }
        "xml" => {
            let mut out = String::new();
            if let Some(header) = replay_header {
                out.push_str(&format!(
                    "<!-- base={} added={} modified={} removed={} token-delta={:+} -->\n",
                    header.base_id,
                    header.added,
                    header.modified,
                    header.removed,
                    header.token_delta
                ));
            }
            out.push_str(&format!(
                "<context-pack goal={:?} used={:?} budget={:?} files={:?} />\n",
                value_or_dash(&opts.goal),
                files
                    .iter()
                    .map(|file| file.tokens)
                    .sum::<i64>()
                    .to_string(),
                opts.budget.to_string(),
                files.len().to_string()
            ));
            Ok(out)
        }
        "markdown" | "" => {
            let mut out = String::new();
            append_replay_header(&mut out, replay_header);
            if matches!(opts.frontmatter.as_str(), "mdx" | "jekyll") {
                out.push_str("---\n");
                out.push_str("title: \"Context Pack\"\n");
                out.push_str(&format!("date: {}\n", current_date_utc()));
                out.push_str("---\n\n");
            }
            if !opts.no_metadata {
                out.push_str("# Context Pack\n\n");
                out.push_str(&format!("**Goal**: {}\n", value_or_dash(&opts.goal)));
                out.push_str(&format!("**Generated**: {}\n", current_rfc3339_utc()));
                out.push_str(&format!(
                    "**Budget**: {} / {} tokens\n\n",
                    files.iter().map(|file| file.tokens).sum::<i64>(),
                    opts.budget
                ));
                out.push_str(&format!(
                    "## Included files ({} files, {} tokens)\n\n",
                    files.len(),
                    files.iter().map(|file| file.tokens).sum::<i64>()
                ));
                for file in files {
                    if opts.explain && !file.reason.is_empty() {
                        out.push_str(&format!(
                            "- {} ({} tokens) - {}\n",
                            file.path, file.tokens, file.reason
                        ));
                    } else {
                        out.push_str(&format!("- {} ({} tokens)\n", file.path, file.tokens));
                    }
                }
                out.push_str("\n---\n\n");
            }
            // Mirror Go's packMarkdown (NoMetadata=true) + writeFileContent:
            //   fmt.Fprintln(w, "## File contents") → ## File contents\n
            //   fmt.Fprintln(w)                      → \n  (blank after header)
            //   for each file: writeFileContent writes
            //     fmt.Fprintf(w, "### %s\n\n", path)
            //     fmt.Fprintf(w, "```%s\n", lang)
            //     content bytes + trailing newline if needed
            //     fmt.Fprintln(w, "```") → ```\n
            //     fmt.Fprintln(w)        → \n  (blank after each file's closing ```)
            out.push_str("## File contents\n");
            out.push('\n'); // blank line after "## File contents" header
            for file in files {
                if !opts.no_paths {
                    out.push_str(&format!("### {}\n\n", file.path));
                }
                out.push_str(&format!("```{}\n", lang_for_path(&file.path)));
                out.push_str(&file.content);
                if !file.content.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str("```\n");
                out.push('\n'); // blank line after each file's closing ```
            }
            Ok(out)
        }
        other => Err(format!("unknown --format value {other:?}")),
    }?;
    if !opts.contract {
        return Ok(out);
    }
    let contract = build_pack_contract(files);
    match opts.format.as_str() {
        "json" => {
            let patched = ctx_contract::embed::embed_json_patch(out.as_bytes(), &contract)
                .map_err(|err| err.to_string())?;
            out = String::from_utf8(patched).map_err(|err| err.to_string())?;
            if !out.ends_with('\n') {
                out.push('\n');
            }
        }
        "plain" => {
            let mut bytes = out.into_bytes();
            ctx_contract::embed::embed_plain(&mut bytes, &contract)
                .map_err(|err| err.to_string())?;
            out = String::from_utf8(bytes).map_err(|err| err.to_string())?;
        }
        "xml" => {
            let mut bytes = out.into_bytes();
            ctx_contract::embed::embed_xml(&mut bytes, &contract).map_err(|err| err.to_string())?;
            out = String::from_utf8(bytes).map_err(|err| err.to_string())?;
        }
        "markdown" | "" => {
            let mut bytes = out.into_bytes();
            ctx_contract::embed::embed_markdown(&mut bytes, &contract)
                .map_err(|err| err.to_string())?;
            out = String::from_utf8(bytes).map_err(|err| err.to_string())?;
        }
        _ => {}
    }
    Ok(out)
}

pub fn append_replay_header(out: &mut String, replay_header: Option<&ReplayHeader>) {
    if let Some(header) = replay_header {
        out.push_str(&format!(
            "# base={} added={} modified={} removed={} token-delta={:+}\n\n",
            header.base_id, header.added, header.modified, header.removed, header.token_delta
        ));
    }
}

pub fn build_pack_contract(files: &[PackFile]) -> ctx_contract::Contract {
    ctx_contract::builder::build(
        files
            .iter()
            .map(|file| ctx_contract::FileInput {
                path: file.path.clone(),
                content: file.content.as_bytes().to_vec(),
                symbols: file.symbols.clone(),
            })
            .collect(),
    )
}

// ---------------------------------------------------------------------------
// Helpers (moved from ctx-cli commands/pack/util.rs + common.rs so the
// renderer is self-contained; ctx-cli delegates to these).
// ---------------------------------------------------------------------------

pub fn estimate_text_tokens(input: &str) -> i64 {
    ctx_tokens::count_str(input).max(1)
}

pub fn value_or_dash(input: &str) -> &str {
    if input.is_empty() {
        "-"
    } else {
        input
    }
}

/// Today's UTC date as `YYYY-MM-DD` (what `date -u +%Y-%m-%d` printed —
/// computed in-process instead of spawning `date` on every pack).
pub fn current_date_utc() -> String {
    let (year, month, day, ..) = utc_parts(unix_now_secs());
    format!("{year:04}-{month:02}-{day:02}")
}

/// The current UTC time as RFC 3339 with second precision
/// (`date -u +%Y-%m-%dT%H:%M:%SZ`).
pub fn current_rfc3339_utc() -> String {
    format_rfc3339_utc(unix_now_secs())
}

/// Formats Unix seconds as `YYYY-MM-DDTHH:MM:SSZ`.
pub fn format_rfc3339_utc(secs: i64) -> String {
    let (year, month, day, hour, minute, second) = utc_parts(secs);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn unix_now_secs() -> i64 {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(since) => since.as_secs() as i64,
        // Before the epoch: floor to the whole second, as `date` does.
        Err(err) => {
            let before = err.duration();
            -(before.as_secs() as i64) - i64::from(before.subsec_nanos() > 0)
        }
    }
}

/// Splits Unix seconds into UTC (year, month, day, hour, minute, second)
/// using the proleptic Gregorian calendar (Howard Hinnant's
/// `civil_from_days`).
fn utc_parts(secs: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (
        year,
        month,
        day,
        (rem / 3_600) as u32,
        (rem % 3_600 / 60) as u32,
        (rem % 60) as u32,
    )
}

pub fn lang_for_path(path: &str) -> &'static str {
    crate::diff::lang_from_path(path)
}

// ---------------------------------------------------------------------------
// High-level API mirroring Go's pack.Pack for an explicit file set.
// ---------------------------------------------------------------------------

/// Plan and render a markdown context pack for an explicit set of file
/// inputs — the Rust equivalent of Go's
/// `pack.Pack(w, files, pack.Options{Goal, Budget, Format: Markdown})`
/// (Contract=false, NoMetadata=false). Used by ctx-tui's `p` action.
///
/// The plan mirrors ctx-cli's read_pack_root scoring loop exactly:
/// score every input with the shared RelevanceContext, drop empty-tier
/// (skipped) files, sort by score desc then path asc, then apply the
/// budget cut (a file whose token estimate would exceed the remaining
/// budget is skipped, later smaller files may still fit).
pub fn pack_markdown(inputs: &[FileInput], goal: &str, budget: i64) -> Result<String, String> {
    let ctx = RelevanceContext::new(goal, budget);
    let mut scored = Vec::new();
    for input in inputs {
        let result = score_relevance_with_ctx(input, &ctx, input.tokens);
        if result.tier.is_empty() {
            continue;
        }
        scored.push((input, result));
    }
    scored.sort_by(|a, b| {
        if a.1.score != b.1.score {
            b.1.score.cmp(&a.1.score)
        } else {
            a.0.path.cmp(&b.0.path)
        }
    });

    let mut files = Vec::new();
    let mut used = 0_i64;
    for (input, result) in scored {
        let content = match std::fs::read_to_string(&input.abs_path) {
            Ok(content) => content,
            // The binary check only sees the first 512 bytes, so a text file
            // with a non-UTF-8 byte further in reaches here. Skip it, as the
            // CLI pack does, instead of failing the whole pack.
            Err(err) if err.kind() == std::io::ErrorKind::InvalidData => continue,
            Err(err) => return Err(format!("pack: read {}: {err}", input.path)),
        };
        // Redact as the CLI pack does with its default `[security]` settings.
        let content = crate::redact::redact_secrets(&input.abs_path, &content, &Default::default());
        // Gate and accumulate with the SAME measured count: gating on the
        // caller-supplied estimate (input.tokens) lets a low/zero estimate
        // overrun the budget.
        let tokens = estimate_text_tokens(&content);
        if budget > 0 && used + tokens > budget {
            continue;
        }
        used += tokens;
        files.push(PackFile {
            path: input.path.clone(),
            abs_path: input.abs_path.clone(),
            content,
            tokens,
            score: result.score,
            relevance: result.tier,
            reason: result.reason,
            symbols: input
                .metadata
                .symbols
                .iter()
                .map(|sym| sym.name.clone())
                .collect(),
        });
    }
    let opts = RenderOptions {
        goal: goal.to_string(),
        budget,
        ..RenderOptions::default()
    };
    render(&opts, &files, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::MetadataInput;

    #[test]
    fn formats_unix_seconds_as_utc_rfc3339() {
        for (secs, want) in [
            (0, "1970-01-01T00:00:00Z"),
            (-1, "1969-12-31T23:59:59Z"),
            (951_782_400, "2000-02-29T00:00:00Z"),
            (1_709_208_000, "2024-02-29T12:00:00Z"),
            (1_735_689_599, "2024-12-31T23:59:59Z"),
            (1_793_836_800, "2026-11-05T00:00:00Z"),
            (4_102_444_799, "2099-12-31T23:59:59Z"),
            (4_107_542_400, "2100-03-01T00:00:00Z"),
        ] {
            assert_eq!(format_rfc3339_utc(secs), want, "{secs}");
        }
    }

    #[test]
    fn utc_parts_walks_every_day_in_order() {
        // Consecutive days from 1900 to 2200 advance the civil date by one.
        let mut prev = utc_parts(-2_208_988_800); // 1900-01-01
        assert_eq!(prev, (1900, 1, 1, 0, 0, 0));
        for day in -25_566..84_000_i64 {
            let next = utc_parts(day * 86_400);
            let leap = |y: i64| (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
            let days_in = |y: i64, m: u32| match m {
                2 if leap(y) => 29,
                2 => 28,
                4 | 6 | 9 | 11 => 30,
                _ => 31,
            };
            let expected = if prev.2 < days_in(prev.0, prev.1) {
                (prev.0, prev.1, prev.2 + 1)
            } else if prev.1 < 12 {
                (prev.0, prev.1 + 1, 1)
            } else {
                (prev.0 + 1, 1, 1)
            };
            assert_eq!((next.0, next.1, next.2), expected, "day {day}");
            prev = next;
        }
    }

    #[test]
    fn current_time_matches_the_date_command() {
        // Where `date` exists, the in-process clock agrees with it (date part;
        // the seconds could tick between the two reads).
        let Ok(out) = std::process::Command::new("date")
            .args(["-u", "+%Y-%m-%d"])
            .output()
        else {
            return;
        };
        let date = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let ours = current_date_utc();
        if ours != date {
            // Midnight rollover between the two reads.
            assert_eq!(current_date_utc(), ours);
        } else {
            assert!(current_rfc3339_utc().starts_with(&date));
        }
    }

    fn pack_file(path: &str, content: &str, tokens: i64) -> PackFile {
        PackFile {
            path: path.to_string(),
            abs_path: String::new(),
            content: content.to_string(),
            tokens,
            score: 3,
            relevance: "Medium".to_string(),
            reason: String::new(),
            symbols: Vec::new(),
        }
    }

    #[test]
    fn render_markdown_full_shape_without_contract() {
        let files = vec![
            pack_file("src/a.rs", "fn a() {}\n", 5),
            pack_file("README.md", "# hello", 3),
        ];
        let opts = RenderOptions {
            budget: 100,
            ..RenderOptions::default()
        };
        let out = render(&opts, &files, None).expect("render");
        assert!(out.starts_with("# Context Pack\n\n"), "header: {out:?}");
        assert!(out.contains("**Goal**: -\n"));
        assert!(out.contains("**Budget**: 8 / 100 tokens\n"));
        assert!(out.contains("## Included files (2 files, 8 tokens)\n"));
        assert!(out.contains("- src/a.rs (5 tokens)\n"));
        assert!(out.contains("## File contents\n"));
        assert!(out.contains("### src/a.rs\n\n```rust\nfn a() {}\n```\n"));
        // Content without a trailing newline gets one before the fence closes.
        assert!(out.contains("### README.md\n\n```markdown\n# hello\n```\n"));
        // contract=false → no manifest embedded.
        assert!(!out.contains("ctx:contract"));
    }

    #[test]
    fn render_markdown_embeds_contract_when_enabled() {
        let files = vec![pack_file("src/a.rs", "fn a() {}\n", 5)];
        let opts = RenderOptions {
            budget: 100,
            contract: true,
            ..RenderOptions::default()
        };
        let out = render(&opts, &files, None).expect("render");
        assert!(out.contains("ctx:contract"));
    }

    #[test]
    fn render_rejects_unknown_format() {
        let opts = RenderOptions {
            format: "bogus".to_string(),
            ..RenderOptions::default()
        };
        let err = render(&opts, &[], None).unwrap_err();
        assert!(err.contains("unknown --format"));
    }

    fn temp_workspace(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ctx-pack-assemble-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp workspace");
        dir
    }

    fn file_input(dir: &std::path::Path, rel: &str, content: &str) -> FileInput {
        let abs = dir.join(rel);
        std::fs::write(&abs, content).expect("write fixture");
        let tokens = estimate_text_tokens(content);
        FileInput {
            path: rel.to_string(),
            abs_path: abs.to_string_lossy().into_owned(),
            is_dir: false,
            tokens,
            role: String::new(),
            metadata: MetadataInput {
                size: content.len() as i64,
                tokens_est: tokens,
                role: String::new(),
                symbols: Vec::new(),
            },
            content_head: content.as_bytes().iter().take(512).copied().collect(),
        }
    }

    #[test]
    fn pack_markdown_renders_full_pack() {
        let dir = temp_workspace("full");
        let inputs = vec![
            file_input(&dir, "a.rs", "fn a() {}\n"),
            file_input(&dir, "b.md", "# b\n"),
        ];
        let out = pack_markdown(&inputs, "", 50_000).expect("pack_markdown");
        assert!(out.starts_with("# Context Pack\n\n"));
        assert!(out.contains("**Budget**:"));
        assert!(out.contains("## File contents\n"));
        assert!(out.contains("### a.rs\n\n```rust\nfn a() {}\n```\n"));
        assert!(out.contains("### b.md\n\n```markdown\n# b\n```\n"));
        assert!(!out.contains("ctx:contract"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pack_markdown_skips_a_file_that_is_not_utf8_past_the_head() {
        let dir = temp_workspace("non-utf8");
        let text = file_input(&dir, "a.rs", "fn a() {}\n");
        let mut body = "a".repeat(770).into_bytes();
        body.extend_from_slice(b"\ncaf\xe9\n");
        let notes = file_input(&dir, "NOTES.md", &"a".repeat(770));
        std::fs::write(&notes.abs_path, &body).expect("write latin-1 tail");
        let out = pack_markdown(&[text, notes], "", 50_000).expect("pack_markdown");
        assert!(out.contains("### a.rs\n"), "text file kept: {out}");
        assert!(
            !out.contains("### NOTES.md"),
            "non-UTF-8 file skipped: {out}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pack_markdown_redacts_secrets() {
        let dir = temp_workspace("redact");
        let token = ["ghp_", "123456789012345678901234567890123456"].concat();
        let input = file_input(&dir, "a.rs", &format!("const T: &str = \"{token}\";\n"));
        let out = pack_markdown(&[input], "", 50_000).expect("pack_markdown");
        assert!(!out.contains(&token), "{out}");
        assert!(out.contains("[REDACTED — kind=github_pat]"), "{out}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pack_markdown_budget_cut_drops_over_budget_file() {
        let dir = temp_workspace("budget");
        let small = file_input(&dir, "a.rs", "fn a() {}\n");
        let big_body = "fn big() {}\n".repeat(500);
        let big = file_input(&dir, "zz_big.rs", &big_body);
        assert!(small.tokens + big.tokens > 50);
        assert!(small.tokens <= 50);
        let out = pack_markdown(&[small, big], "", 50).expect("pack_markdown");
        assert!(out.contains("### a.rs\n"), "small file kept: {out}");
        assert!(!out.contains("zz_big.rs"), "big file dropped: {out}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
