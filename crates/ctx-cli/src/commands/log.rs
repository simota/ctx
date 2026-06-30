use std::collections::HashMap;
use std::env;
use std::ffi::{OsStr, OsString};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::{Frame, Terminal};
use serde::Serialize;

use crate::commands::where_cmd::where_files;
use crate::common::{flag_value, is_option};

const DEFAULT_LIMIT: usize = 100;
const MAX_LIMIT: usize = 500;
const DIFF_COLUMN_HEADER: &str = "   old  new | code";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputMode {
    Tui,
    Plain,
    Json,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LogArgs {
    root: PathBuf,
    limit: usize,
    ref_name: Option<String>,
    path: Option<String>,
    query: Option<String>,
    output_mode: OutputMode,
}

#[derive(Debug, Clone, Serialize)]
struct LogData {
    root: String,
    source: LogSource,
    commits: Vec<LogCommit>,
    truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
struct LogSource {
    kind: String,
    label: String,
    matched_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct LogCommit {
    hash: String,
    hash_full: String,
    author: String,
    author_email: String,
    subject: String,
    date: i64,
    parents: Vec<String>,
    matched_paths: Vec<String>,
    #[serde(skip)]
    is_worktree: bool,
}

#[derive(Debug, Clone, Default)]
struct CommitDetail {
    files: Vec<ctx_git::CommitFile>,
    lines: Vec<String>,
    diff_loaded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActivePanel {
    Commits,
    Detail,
}

impl ActivePanel {
    fn label(self) -> &'static str {
        match self {
            ActivePanel::Commits => "commits",
            ActivePanel::Detail => "diff",
        }
    }
}

struct LogState {
    root: PathBuf,
    data: LogData,
    selected_commit: usize,
    diff_scroll: usize,
    detail: CommitDetail,
    diff_loading: bool,
    error: Option<String>,
    active_panel: ActivePanel,
}

pub(crate) fn run_log_command(args: &[OsString]) -> Option<ExitCode> {
    let parsed = match parse_log_args(args) {
        Ok(parsed) => parsed?,
        Err(err) => {
            eprintln!("ctx log: {err}");
            return Some(ExitCode::from(2));
        }
    };

    if parsed.output_mode == OutputMode::Tui && !ctx_tui::is_interactive() {
        eprintln!("ctx log: requires an interactive terminal (TTY)");
        return Some(ExitCode::from(1));
    }

    let root = parsed.root.clone();
    let data = match load_log_data(&parsed) {
        Ok(data) => data,
        Err(err) => {
            eprintln!("ctx log: {err}");
            return Some(ExitCode::from(1));
        }
    };

    let result = match parsed.output_mode {
        OutputMode::Tui => run_viewer(root, data),
        OutputMode::Plain => render_plain(&data),
        OutputMode::Json => render_json(&data),
    };
    match result {
        Ok(()) => Some(ExitCode::SUCCESS),
        Err(err) => {
            eprintln!("ctx log: {err}");
            Some(ExitCode::from(1))
        }
    }
}

fn parse_log_args(args: &[OsString]) -> Result<Option<LogArgs>, String> {
    let mut saw_log = false;
    let mut limit = DEFAULT_LIMIT;
    let mut ref_name: Option<String> = None;
    let mut path: Option<String> = None;
    let mut query: Option<String> = None;
    let mut output_mode = OutputMode::Tui;
    let mut positionals: Vec<OsString> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == OsStr::new("log") {
            if saw_log {
                return Err("duplicate log command".to_string());
            }
            saw_log = true;
        } else if arg == OsStr::new("--plain") {
            if output_mode == OutputMode::Json {
                return Err("choose only one of --plain or --json".to_string());
            }
            output_mode = OutputMode::Plain;
        } else if arg == OsStr::new("--json") {
            if output_mode == OutputMode::Plain {
                return Err("choose only one of --plain or --json".to_string());
            }
            output_mode = OutputMode::Json;
        } else if let Some(value) = flag_value(arg, "--limit") {
            limit = parse_limit(&value.to_string_lossy())?;
        } else if arg == OsStr::new("--limit") {
            i += 1;
            let value = args
                .get(i)
                .ok_or_else(|| "--limit requires a value".to_string())?;
            limit = parse_limit(&value.to_string_lossy())?;
        } else if let Some(value) = flag_value(arg, "--ref") {
            ref_name = Some(parse_ref(&value.to_string_lossy())?);
        } else if arg == OsStr::new("--ref") {
            i += 1;
            let value = args
                .get(i)
                .ok_or_else(|| "--ref requires a value".to_string())?;
            ref_name = Some(parse_ref(&value.to_string_lossy())?);
        } else if let Some(value) = flag_value(arg, "--path") {
            path = Some(parse_nonempty_value("--path", &value.to_string_lossy())?);
        } else if arg == OsStr::new("--path") {
            i += 1;
            let value = args
                .get(i)
                .ok_or_else(|| "--path requires a value".to_string())?;
            path = Some(parse_nonempty_value("--path", &value.to_string_lossy())?);
        } else if let Some(value) = flag_value(arg, "--query") {
            query = Some(parse_nonempty_value("--query", &value.to_string_lossy())?);
        } else if arg == OsStr::new("--query") {
            i += 1;
            let value = args
                .get(i)
                .ok_or_else(|| "--query requires a value".to_string())?;
            query = Some(parse_nonempty_value("--query", &value.to_string_lossy())?);
        } else if is_option(arg) {
            return Err(format!("unrecognized argument {}", arg.to_string_lossy()));
        } else if saw_log {
            positionals.push(arg.clone());
        } else {
            return Ok(None);
        }
        i += 1;
    }

    if !saw_log {
        return Ok(None);
    }
    if positionals.len() > 1 {
        return Err("accepts at most one positional path".to_string());
    }
    if let Some(positional) = positionals.first() {
        if path.is_some() {
            return Err("use either positional path or --path, not both".to_string());
        }
        path = Some(parse_nonempty_value("path", &positional.to_string_lossy())?);
    }
    if path.is_some() && query.is_some() {
        return Err("use either --path or --query for this MVP".to_string());
    }

    let root = env::current_dir().map_err(|err| format!("cannot determine current dir: {err}"))?;
    let path = match path {
        Some(path) => Some(normalize_log_path(&root, &path)?),
        None => None,
    };

    Ok(Some(LogArgs {
        root,
        limit,
        ref_name,
        path,
        query,
        output_mode,
    }))
}

fn parse_limit(raw: &str) -> Result<usize, String> {
    let limit = raw
        .parse::<usize>()
        .map_err(|_| "--limit must be a number from 1 to 500".to_string())?;
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err("--limit must be between 1 and 500".to_string());
    }
    Ok(limit)
}

fn parse_ref(raw: &str) -> Result<String, String> {
    let value = parse_nonempty_value("--ref", raw)?;
    if !is_valid_ref(&value) {
        return Err(format!("invalid --ref {value:?}"));
    }
    Ok(value)
}

fn parse_nonempty_value(name: &str, raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(format!("{name} requires a non-empty value"));
    }
    Ok(trimmed.to_string())
}

/// A ref is accepted when it cannot be interpreted as a git option and every
/// character is from a conservative branch/hash/tag set.
fn is_valid_ref(ref_name: &str) -> bool {
    !ref_name.starts_with('-')
        && !ref_name.is_empty()
        && ref_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '/' | '@' | '-'))
}

fn normalize_log_path(root: &Path, raw: &str) -> Result<String, String> {
    let raw_path = PathBuf::from(raw);
    let path = if raw_path.is_absolute() {
        raw_path
            .strip_prefix(root)
            .map_err(|err| format!("path {raw:?} is outside the repository root: {err}"))?
            .to_path_buf()
    } else {
        raw_path
    };
    clean_relative_path(&path).ok_or_else(|| format!("path {raw:?} is outside the repository root"))
}

fn clean_relative_path(path: &Path) -> Option<String> {
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

fn load_log_data(args: &LogArgs) -> Result<LogData, String> {
    let mut data = if let Some(query) = &args.query {
        load_query_log(args, query)?
    } else if let Some(path) = &args.path {
        load_path_log(args, path)?
    } else {
        load_repo_log(args)?
    };
    if args.output_mode == OutputMode::Tui && args.ref_name.is_none() {
        prepend_worktree_commit(args, &mut data)?;
    }
    Ok(data)
}

fn load_repo_log(args: &LogArgs) -> Result<LogData, String> {
    let (entries, truncated) = ctx_git::repo_log(&args.root, args.limit, args.ref_name.as_deref())
        .map_err(|err| err.to_string())?;
    Ok(LogData {
        root: args.root.display().to_string(),
        source: LogSource {
            kind: "repo".to_string(),
            label: args.ref_name.clone().unwrap_or_else(|| "HEAD".to_string()),
            matched_paths: Vec::new(),
        },
        commits: entries.into_iter().map(LogCommit::from_repo).collect(),
        truncated,
    })
}

fn load_path_log(args: &LogArgs, path: &str) -> Result<LogData, String> {
    let (entries, truncated) =
        ctx_git::file_log_ref(&args.root, path, args.limit, args.ref_name.as_deref())
            .map_err(|err| err.to_string())?;
    Ok(LogData {
        root: args.root.display().to_string(),
        source: LogSource {
            kind: "path".to_string(),
            label: format!("path {path}"),
            matched_paths: vec![path.to_string()],
        },
        commits: entries
            .into_iter()
            .map(|entry| LogCommit::from_file(entry, vec![path.to_string()]))
            .collect(),
        truncated,
    })
}

fn load_query_log(args: &LogArgs, query: &str) -> Result<LogData, String> {
    let files = where_files(&args.root)?;
    let opts = ctx_where::Options {
        limit: 10,
        context_n: 0,
        require_all: false,
        regex: String::new(),
        synonyms: Default::default(),
        explain: false,
    };
    let results = ctx_where::search_with_options(&files, query, &opts);
    let matched_paths: Vec<String> = results.into_iter().map(|result| result.path).collect();

    let mut by_hash: HashMap<String, LogCommit> = HashMap::new();
    let mut truncated = false;
    for path in &matched_paths {
        let (entries, path_truncated) =
            ctx_git::file_log_ref(&args.root, path, args.limit, args.ref_name.as_deref())
                .map_err(|err| err.to_string())?;
        truncated |= path_truncated;
        for entry in entries {
            let hash = entry.hash_full.clone();
            by_hash
                .entry(hash)
                .and_modify(|commit| push_unique(&mut commit.matched_paths, path.clone()))
                .or_insert_with(|| LogCommit::from_file(entry, vec![path.clone()]));
        }
    }

    let mut commits: Vec<LogCommit> = by_hash.into_values().collect();
    commits.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.hash.cmp(&b.hash)));
    if commits.len() > args.limit {
        commits.truncate(args.limit);
        truncated = true;
    }

    Ok(LogData {
        root: args.root.display().to_string(),
        source: LogSource {
            kind: "query".to_string(),
            label: format!("query {query:?}"),
            matched_paths,
        },
        commits,
        truncated,
    })
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn prepend_worktree_commit(args: &LogArgs, data: &mut LogData) -> Result<(), String> {
    let files = filtered_worktree_files(args, &data.source.matched_paths)?;
    if files.is_empty() {
        return Ok(());
    }
    let matched_paths = if args.query.is_some() {
        files.iter().map(|file| file.path.clone()).collect()
    } else {
        args.path.iter().cloned().collect()
    };
    data.commits
        .insert(0, LogCommit::from_worktree(files.len(), matched_paths));
    Ok(())
}

fn filtered_worktree_files(
    args: &LogArgs,
    matched_paths: &[String],
) -> Result<Vec<ctx_git::CommitFile>, String> {
    let mut files = ctx_git::worktree_files(&args.root).map_err(|err| err.to_string())?;
    if let Some(path) = &args.path {
        files.retain(|file| path_matches_filter(&file.path, path));
    } else if args.query.is_some() {
        files.retain(|file| {
            matched_paths
                .iter()
                .any(|path| path_matches_filter(&file.path, path))
        });
    }
    Ok(files)
}

fn path_matches_filter(path: &str, filter: &str) -> bool {
    path == filter
        || path
            .strip_prefix(filter)
            .is_some_and(|tail| tail.starts_with('/'))
}

impl LogCommit {
    fn from_repo(entry: ctx_git::RepoLogEntry) -> Self {
        Self {
            hash: entry.hash,
            hash_full: entry.hash_full,
            author: entry.author,
            author_email: entry.author_email,
            subject: entry.subject,
            date: entry.date,
            parents: entry.parents,
            matched_paths: Vec::new(),
            is_worktree: false,
        }
    }

    fn from_file(entry: ctx_git::FileLogEntry, matched_paths: Vec<String>) -> Self {
        Self {
            hash: entry.hash,
            hash_full: entry.hash_full,
            author: entry.author,
            author_email: entry.author_email,
            subject: entry.subject,
            date: entry.date,
            parents: Vec::new(),
            matched_paths,
            is_worktree: false,
        }
    }

    fn from_worktree(file_count: usize, matched_paths: Vec<String>) -> Self {
        Self {
            hash: "worktree".to_string(),
            hash_full: "worktree".to_string(),
            author: "working tree".to_string(),
            author_email: "".to_string(),
            subject: format!("uncommitted changes ({file_count} files)"),
            date: 0,
            parents: Vec::new(),
            matched_paths,
            is_worktree: true,
        }
    }
}

fn render_plain(data: &LogData) -> Result<(), String> {
    let mut out = io::stdout();
    writeln!(out, "ctx log {} ({})", data.source.label, data.source.kind)
        .map_err(|err| err.to_string())?;
    if !data.source.matched_paths.is_empty() {
        writeln!(
            out,
            "matched paths: {}",
            data.source.matched_paths.join(", ")
        )
        .map_err(|err| err.to_string())?;
    }
    if data.commits.is_empty() {
        writeln!(out, "no commits").map_err(|err| err.to_string())?;
        return Ok(());
    }
    for commit in &data.commits {
        writeln!(
            out,
            "{} {} {}",
            commit.hash,
            format_commit_date(commit.date),
            commit.subject
        )
        .map_err(|err| err.to_string())?;
        if !commit.matched_paths.is_empty() {
            writeln!(out, "  matched: {}", commit.matched_paths.join(", "))
                .map_err(|err| err.to_string())?;
        }
    }
    if data.truncated {
        writeln!(out, "history truncated by --limit").map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn render_json(data: &LogData) -> Result<(), String> {
    serde_json::to_writer_pretty(io::stdout(), data).map_err(|err| err.to_string())?;
    println!();
    Ok(())
}

fn run_viewer(root: PathBuf, data: LogData) -> Result<(), String> {
    let mut stdout = io::stdout();
    terminal::enable_raw_mode().map_err(|err| err.to_string())?;
    let _guard = TerminalGuard;
    execute!(stdout, EnterAlternateScreen).map_err(|err| err.to_string())?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(|err| err.to_string())?;
    let mut state = LogState::new(root, data);

    loop {
        terminal
            .draw(|frame| render_log_view(frame, &state))
            .map_err(|err| err.to_string())?;
        let event = event::read().map_err(|err| err.to_string())?;
        let Event::Key(key) = event else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            break;
        }
        match key.code {
            KeyCode::Char('q') => break,
            KeyCode::Left => state.focus_commits(),
            KeyCode::Right => state.focus_detail(),
            KeyCode::Down | KeyCode::Char('j') => state.move_active_down(),
            KeyCode::Up | KeyCode::Char('k') => state.move_active_up(),
            KeyCode::PageDown | KeyCode::Char(' ') | KeyCode::Char('f') => {
                state.focus_detail();
                state.scroll_diff(10);
            }
            KeyCode::PageUp | KeyCode::Char('b') => {
                state.focus_detail();
                state.scroll_diff(-10);
            }
            KeyCode::Char('n') => {
                state.focus_detail();
                state.jump_file(1);
            }
            KeyCode::Char('p') => {
                state.focus_detail();
                state.jump_file(-1);
            }
            KeyCode::Home | KeyCode::Char('g') => state.move_home(),
            KeyCode::End | KeyCode::Char('G') => state.move_end(),
            KeyCode::Enter | KeyCode::Char('d') => {
                state.focus_detail();
                if !state.detail.diff_loaded {
                    state.mark_diff_loading();
                    terminal
                        .draw(|frame| render_log_view(frame, &state))
                        .map_err(|err| err.to_string())?;
                }
                state.load_selected_diff();
            }
            _ => {}
        }
    }
    Ok(())
}

impl LogState {
    fn new(root: PathBuf, data: LogData) -> Self {
        let mut state = Self {
            root,
            data,
            selected_commit: 0,
            diff_scroll: 0,
            detail: CommitDetail::default(),
            diff_loading: false,
            error: None,
            active_panel: ActivePanel::Commits,
        };
        state.load_selected_summary();
        state
    }

    fn focus_commits(&mut self) {
        self.active_panel = ActivePanel::Commits;
    }

    fn focus_detail(&mut self) {
        self.active_panel = ActivePanel::Detail;
    }

    fn move_active_down(&mut self) {
        match self.active_panel {
            ActivePanel::Commits => self.move_commit(1),
            ActivePanel::Detail => self.scroll_diff(1),
        }
    }

    fn move_active_up(&mut self) {
        match self.active_panel {
            ActivePanel::Commits => self.move_commit(-1),
            ActivePanel::Detail => self.scroll_diff(-1),
        }
    }

    fn move_commit(&mut self, delta: isize) {
        if self.data.commits.is_empty() {
            return;
        }
        let next = (self.selected_commit as isize + delta)
            .clamp(0, self.data.commits.len().saturating_sub(1) as isize);
        if next as usize != self.selected_commit {
            self.selected_commit = next as usize;
            self.diff_scroll = 0;
            self.load_selected_summary();
        }
    }

    fn move_home(&mut self) {
        if self.selected_commit != 0 {
            self.selected_commit = 0;
            self.diff_scroll = 0;
            self.load_selected_summary();
        }
    }

    fn move_end(&mut self) {
        let last = self.data.commits.len().saturating_sub(1);
        if self.selected_commit != last {
            self.selected_commit = last;
            self.diff_scroll = 0;
            self.load_selected_summary();
        }
    }

    fn scroll_diff(&mut self, delta: isize) {
        let max = self.detail.lines.len().saturating_sub(1);
        let next = (self.diff_scroll as isize + delta).clamp(0, max as isize);
        self.diff_scroll = next as usize;
    }

    fn jump_file(&mut self, delta: isize) {
        if let Some(target) = file_jump_target(&self.detail.lines, self.diff_scroll, delta) {
            self.diff_scroll = target;
        }
    }

    fn mark_diff_loading(&mut self) {
        if !self.data.commits.is_empty() {
            self.diff_loading = true;
            self.error = None;
        }
    }

    fn load_selected_summary(&mut self) {
        self.diff_loading = false;
        let Some(commit) = self.data.commits.get(self.selected_commit) else {
            self.detail = CommitDetail::default();
            self.error = None;
            return;
        };
        match build_commit_detail(&self.root, commit, false) {
            Ok(detail) => {
                self.detail = detail;
                self.error = None;
            }
            Err(err) => {
                self.detail = CommitDetail::default();
                self.error = Some(err);
            }
        }
    }

    fn load_selected_diff(&mut self) {
        if self.detail.diff_loaded {
            self.diff_scroll = 0;
            self.diff_loading = false;
            return;
        }
        let Some(commit) = self.data.commits.get(self.selected_commit) else {
            self.diff_loading = false;
            return;
        };
        match build_commit_detail(&self.root, commit, true) {
            Ok(detail) => {
                self.detail = detail;
                self.error = None;
                self.diff_scroll = 0;
                self.diff_loading = false;
            }
            Err(err) => {
                self.detail = CommitDetail::default();
                self.error = Some(err);
                self.diff_loading = false;
            }
        }
    }
}

fn build_commit_detail(
    root: &Path,
    commit: &LogCommit,
    include_diff: bool,
) -> Result<CommitDetail, String> {
    if commit.is_worktree {
        return build_worktree_detail(root, commit, include_diff);
    }

    let files = ctx_git::commit_files(root, &commit.hash_full).map_err(|err| err.to_string())?;
    let mut lines = Vec::new();
    lines.push(format!("commit {}", commit.hash_full));
    lines.push(format!(
        "Author: {} <{}>",
        commit.author, commit.author_email
    ));
    lines.push(format!("Date:   {}", format_commit_date(commit.date)));
    lines.push(String::new());
    lines.push(format!("    {}", commit.subject));
    lines.push(String::new());
    lines.push(format!("{} file(s) changed", files.len()));
    for file in &files {
        lines.push(format_file_stat(file));
    }
    lines.push(String::new());

    if !include_diff {
        lines.push("diff body not loaded for faster navigation".to_string());
        lines.push("press Enter or d to load the selected commit diff".to_string());
        return Ok(CommitDetail {
            files,
            lines,
            diff_loaded: false,
        });
    }

    let from = format!("{}^", commit.hash_full);
    for file in &files {
        lines.push(format!("diff -- {}", file.path));
        lines.push(format_file_stat(file));
        match ctx_git::commit_diff(root, &from, &commit.hash_full, &file.path) {
            Ok(diff) if diff.binary => lines.push("  [binary] file changed".to_string()),
            Ok(diff) if diff.no_change => lines.push("  [no text changes]".to_string()),
            Ok(diff) => {
                if diff.truncated {
                    lines.push("  [diff truncated]".to_string());
                }
                if !diff.lines.is_empty() {
                    lines.push(DIFF_COLUMN_HEADER.to_string());
                }
                for line in diff.lines {
                    lines.push(format_diff_line(&line));
                }
            }
            Err(err) => lines.push(format!("  diff unavailable: {err}")),
        }
        lines.push(String::new());
    }

    Ok(CommitDetail {
        files,
        lines,
        diff_loaded: true,
    })
}

fn build_worktree_detail(
    root: &Path,
    commit: &LogCommit,
    include_diff: bool,
) -> Result<CommitDetail, String> {
    let mut files = ctx_git::worktree_files(root).map_err(|err| err.to_string())?;
    if !commit.matched_paths.is_empty() {
        files.retain(|file| {
            commit
                .matched_paths
                .iter()
                .any(|path| path_matches_filter(&file.path, path))
        });
    }

    let mut lines = Vec::new();
    lines.push("commit worktree".to_string());
    lines.push("Author: working tree".to_string());
    lines.push("Date:   uncommitted".to_string());
    lines.push(String::new());
    lines.push(format!("    {}", commit.subject));
    lines.push(String::new());
    lines.push(format!("{} file(s) changed", files.len()));
    for file in &files {
        lines.push(format_file_stat(file));
    }
    lines.push(String::new());

    if !include_diff {
        lines.push("diff body not loaded for faster navigation".to_string());
        lines.push("press Enter or d to load the selected commit diff".to_string());
        return Ok(CommitDetail {
            files,
            lines,
            diff_loaded: false,
        });
    }

    for file in &files {
        lines.push(format!("diff -- {}", file.path));
        lines.push(format_file_stat(file));
        match ctx_git::worktree_diff(root, &file.path) {
            Ok(diff) if diff.binary => lines.push("  [binary] file changed".to_string()),
            Ok(diff) if diff.no_change => lines.push("  [no text changes]".to_string()),
            Ok(diff) => {
                if diff.truncated {
                    lines.push("  [diff truncated]".to_string());
                }
                if !diff.lines.is_empty() {
                    lines.push(DIFF_COLUMN_HEADER.to_string());
                }
                for line in diff.lines {
                    lines.push(format_diff_line(&line));
                }
            }
            Err(err) => lines.push(format!("  diff unavailable: {err}")),
        }
        lines.push(String::new());
    }

    Ok(CommitDetail {
        files,
        lines,
        diff_loaded: true,
    })
}

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

fn render_log_view(frame: &mut Frame, state: &LogState) {
    let area = frame.area();
    if area.width == 0 || area.height == 0 {
        return;
    }
    if area.height == 1 {
        render_footer(frame, state, area);
        return;
    }

    let mut constraints = vec![Constraint::Length(1)];
    if state.data.truncated {
        constraints.push(Constraint::Length(1));
    }
    if state.error.is_some() {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Min(0));
    constraints.push(Constraint::Length(1));

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let mut idx = 0;
    render_header(frame, state, chunks[idx]);
    idx += 1;

    if state.data.truncated {
        render_alert(
            frame,
            "history truncated by --limit",
            chunks[idx],
            Color::LightYellow,
        );
        idx += 1;
    }
    if let Some(err) = &state.error {
        render_alert(
            frame,
            &format!("error: {err}"),
            chunks[idx],
            Color::LightRed,
        );
        idx += 1;
    }

    render_body(frame, state, chunks[idx]);
    render_footer(frame, state, chunks[idx + 1]);
}

fn render_header(frame: &mut Frame, state: &LogState, area: Rect) {
    let line = Line::from(vec![
        Span::styled(
            "ctx log",
            Style::default()
                .fg(Color::LightCyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" | "),
        Span::styled(
            format!("{} ({})", state.data.source.label, state.data.source.kind),
            Style::default().fg(Color::White),
        ),
        Span::styled(
            format!(
                " | commits {}{}",
                state.data.commits.len(),
                if state.data.truncated { "+" } else { "" }
            ),
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn render_alert(frame: &mut Frame, message: &str, area: Rect, color: Color) {
    let paragraph = Paragraph::new(message.to_string())
        .style(Style::default().fg(color).add_modifier(Modifier::BOLD));
    frame.render_widget(paragraph, area);
}

fn render_body(frame: &mut Frame, state: &LogState, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    if area.width < 96 {
        let commit_height = if area.height <= 8 {
            area.height / 2
        } else {
            (area.height / 3).max(5).min(area.height.saturating_sub(3))
        };
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(commit_height), Constraint::Min(0)])
            .split(area);
        render_commit_panel(frame, state, chunks[0]);
        render_detail_panel(frame, state, chunks[1]);
    } else {
        let commit_width = area.width.clamp(38, 48);
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(commit_width), Constraint::Min(48)])
            .split(area);
        render_commit_panel(frame, state, chunks[0]);
        render_detail_panel(frame, state, chunks[1]);
    }
}

fn render_commit_panel(frame: &mut Frame, state: &LogState, area: Rect) {
    let active = state.active_panel == ActivePanel::Commits;
    let title = format!(
        " commits {}{}{} ",
        commit_position_label(state.selected_commit, state.data.commits.len()),
        if state.data.truncated { "+" } else { "" },
        if active { " [focus]" } else { "" }
    );
    let block = panel_block(title, active);
    if area.height <= 2 || area.width <= 2 {
        frame.render_widget(block, area);
        return;
    }

    if state.data.commits.is_empty() {
        let paragraph = Paragraph::new(Line::styled(
            "no commits for this source",
            Style::default().fg(Color::DarkGray),
        ))
        .block(block)
        .alignment(Alignment::Left);
        frame.render_widget(paragraph, area);
        return;
    }

    let visible = area.height.saturating_sub(2) as usize;
    let start = scroll_start(state.selected_commit, visible);
    let items = state
        .data
        .commits
        .iter()
        .enumerate()
        .skip(start)
        .take(visible)
        .map(|(idx, commit)| commit_item(idx, state.selected_commit, commit, active))
        .collect::<Vec<_>>();

    frame.render_widget(List::new(items).block(block), area);
}

fn render_detail_panel(frame: &mut Frame, state: &LogState, area: Rect) {
    let active = state.active_panel == ActivePanel::Detail;
    let visible = area.height.saturating_sub(2) as usize;
    let title = detail_title(state, visible, active);
    let block = panel_block(title, active);

    if area.height <= 2 || area.width <= 2 {
        frame.render_widget(block, area);
        return;
    }

    let text = if state.diff_loading {
        Text::from(vec![
            Line::styled(
                "loading full diff",
                Style::default()
                    .fg(Color::LightYellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Line::styled(
                "large commits can take a moment",
                Style::default().fg(Color::DarkGray),
            ),
            Line::styled(
                "files view will switch to diff when ready",
                Style::default().fg(Color::DarkGray),
            ),
        ])
    } else if state.detail.lines.is_empty() {
        Text::from(Line::styled(
            "select a commit to inspect files",
            Style::default().fg(Color::DarkGray),
        ))
    } else {
        let start = visible_scroll_start(state.diff_scroll, visible, state.detail.lines.len());
        let lines = state
            .detail
            .lines
            .iter()
            .skip(start)
            .take(visible)
            .map(|line| styled_detail_line(line))
            .collect::<Vec<_>>();
        Text::from(lines)
    };

    frame.render_widget(Paragraph::new(text).block(block), area);
}

fn render_footer(frame: &mut Frame, state: &LogState, area: Rect) {
    let paragraph =
        Paragraph::new(footer_text(state, area.width)).style(Style::default().fg(Color::DarkGray));
    frame.render_widget(paragraph, area);
}

fn panel_block(title: String, active: bool) -> Block<'static> {
    let border_style = if active {
        Style::default()
            .fg(Color::LightCyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let title_style = if active {
        Style::default()
            .fg(Color::LightCyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };
    Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(Line::from(Span::styled(title, title_style)))
}

fn commit_item(idx: usize, selected: usize, commit: &LogCommit, active: bool) -> ListItem<'static> {
    let selected = idx == selected;
    let base_style = if selected {
        if active {
            Style::default()
                .fg(Color::White)
                .bg(Color::Blue)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(Color::White)
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD)
        }
    } else {
        Style::default().fg(Color::Gray)
    };
    let hash_style = if selected {
        base_style
    } else {
        Style::default().fg(Color::LightCyan)
    };
    let matched = if commit.matched_paths.is_empty() {
        String::new()
    } else {
        format!(" [{}]", commit.matched_paths.join(", "))
    };

    ListItem::new(Line::from(vec![
        Span::styled(if selected { "> " } else { "  " }, base_style),
        Span::styled(format!("{} ", commit.hash), hash_style),
        Span::styled(commit.subject.clone(), base_style),
        Span::styled(matched, Style::default().fg(Color::DarkGray)),
    ]))
    .style(base_style)
}

fn detail_title(state: &LogState, visible: usize, active: bool) -> String {
    state
        .data
        .commits
        .get(state.selected_commit)
        .map(|commit| {
            let label = if state.diff_loading {
                "loading"
            } else if state.detail.diff_loaded {
                "diff"
            } else {
                "files"
            };
            let start = visible_scroll_start(state.diff_scroll, visible, state.detail.lines.len());
            let range = visible_range_label(start, visible, state.detail.lines.len());
            let hint = if state.diff_loading {
                " | wait".to_string()
            } else if state.detail.diff_loaded {
                String::new()
            } else {
                " | d diff".to_string()
            };
            format!(
                " {label} {} {range} | {} files{hint}{} ",
                commit.hash,
                state.detail.files.len(),
                if active { " [focus]" } else { "" },
            )
        })
        .unwrap_or_else(|| " files | no commit selected ".to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DetailLineKind {
    Blank,
    Commit,
    Meta,
    Subject,
    Summary,
    FileHeader,
    FileStatAdd,
    FileStatDelete,
    FileStatModify,
    DiffColumnHeader,
    Add,
    Delete,
    Context,
    Notice,
    Hint,
    Warning,
    Error,
    Other,
}

fn styled_detail_line(line: &str) -> Line<'static> {
    let kind = classify_detail_line(line);
    match kind {
        DetailLineKind::Blank => Line::raw(""),
        DetailLineKind::FileHeader => {
            let path = line.strip_prefix("diff -- ").unwrap_or(line);
            Line::from(vec![
                Span::styled("file ", detail_line_style(kind)),
                Span::styled(path.to_string(), detail_line_style(kind)),
            ])
        }
        DetailLineKind::FileStatAdd
        | DetailLineKind::FileStatDelete
        | DetailLineKind::FileStatModify => styled_file_stat_line(line, kind),
        DetailLineKind::Add | DetailLineKind::Delete | DetailLineKind::Context => {
            styled_diff_body_line(line, kind)
        }
        _ => Line::styled(line.to_string(), detail_line_style(kind)),
    }
}

fn styled_file_stat_line(line: &str, kind: DetailLineKind) -> Line<'static> {
    let style = detail_line_style(kind);
    if line.len() >= 15 {
        return Line::from(vec![
            Span::raw(" "),
            Span::styled(line[1..2].to_string(), style.add_modifier(Modifier::BOLD)),
            Span::styled(" ".to_string(), Style::default().fg(Color::DarkGray)),
            Span::styled(
                line[3..8].to_string(),
                Style::default().fg(Color::LightGreen),
            ),
            Span::styled(" ".to_string(), Style::default().fg(Color::DarkGray)),
            Span::styled(
                line[9..14].to_string(),
                Style::default().fg(Color::LightRed),
            ),
            Span::styled(" ".to_string(), Style::default().fg(Color::DarkGray)),
            Span::styled(line[15..].to_string(), style),
        ]);
    }

    Line::styled(line.to_string(), style)
}

fn styled_diff_body_line(line: &str, kind: DetailLineKind) -> Line<'static> {
    let marker_style = detail_line_style(kind).add_modifier(Modifier::BOLD);
    let body_style = diff_body_style(kind);
    let Some((head, text)) = line.split_once(" | ") else {
        return Line::styled(line.to_string(), body_style);
    };
    if head.is_empty() {
        return Line::styled(line.to_string(), body_style);
    }
    let (marker, numbers) = head.split_at(1);
    Line::from(vec![
        Span::styled(marker.to_string(), marker_style),
        Span::styled(numbers.to_string(), Style::default().fg(Color::DarkGray)),
        Span::styled(" | ", Style::default().fg(Color::DarkGray)),
        Span::styled(text.to_string(), body_style),
    ])
}

fn classify_detail_line(line: &str) -> DetailLineKind {
    if line.is_empty() {
        DetailLineKind::Blank
    } else if line == DIFF_COLUMN_HEADER {
        DetailLineKind::DiffColumnHeader
    } else if line.starts_with('+') && line.contains(" | ") {
        DetailLineKind::Add
    } else if line.starts_with('-') && line.contains(" | ") {
        DetailLineKind::Delete
    } else if line.starts_with("  ") && line.contains(" | ") {
        DetailLineKind::Context
    } else if line.starts_with("diff -- ") {
        DetailLineKind::FileHeader
    } else if line.starts_with(" A") {
        DetailLineKind::FileStatAdd
    } else if line.starts_with(" D") {
        DetailLineKind::FileStatDelete
    } else if line.starts_with(" M") {
        DetailLineKind::FileStatModify
    } else if line.contains("[diff truncated]") {
        DetailLineKind::Warning
    } else if line.contains("[binary]") || line.contains("[no text changes]") {
        DetailLineKind::Notice
    } else if line.contains("diff unavailable") {
        DetailLineKind::Error
    } else if line == "diff body not loaded for faster navigation"
        || line == "press Enter or d to load the selected commit diff"
    {
        DetailLineKind::Hint
    } else if line.starts_with("commit ") {
        DetailLineKind::Commit
    } else if line.starts_with("Author:") || line.starts_with("Date:") {
        DetailLineKind::Meta
    } else if line.starts_with("    ") {
        DetailLineKind::Subject
    } else if line.ends_with(" file(s) changed") {
        DetailLineKind::Summary
    } else {
        DetailLineKind::Other
    }
}

fn detail_line_style(kind: DetailLineKind) -> Style {
    match kind {
        DetailLineKind::Blank => Style::default(),
        DetailLineKind::Commit => Style::default()
            .fg(Color::LightCyan)
            .add_modifier(Modifier::BOLD),
        DetailLineKind::Meta => Style::default().fg(Color::DarkGray),
        DetailLineKind::Subject => Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
        DetailLineKind::Summary => Style::default().fg(Color::LightMagenta),
        DetailLineKind::FileHeader => Style::default()
            .fg(Color::LightCyan)
            .add_modifier(Modifier::BOLD),
        DetailLineKind::FileStatAdd | DetailLineKind::Add => Style::default().fg(Color::LightGreen),
        DetailLineKind::FileStatDelete | DetailLineKind::Delete => {
            Style::default().fg(Color::LightRed)
        }
        DetailLineKind::FileStatModify | DetailLineKind::Warning => {
            Style::default().fg(Color::LightYellow)
        }
        DetailLineKind::DiffColumnHeader => Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
        DetailLineKind::Context => Style::default().fg(Color::Gray),
        DetailLineKind::Notice => Style::default().fg(Color::LightMagenta),
        DetailLineKind::Hint => Style::default().fg(Color::LightYellow),
        DetailLineKind::Error => Style::default()
            .fg(Color::LightRed)
            .add_modifier(Modifier::BOLD),
        DetailLineKind::Other => Style::default().fg(Color::Gray),
    }
}

fn diff_body_style(kind: DetailLineKind) -> Style {
    match kind {
        DetailLineKind::Add => Style::default()
            .fg(Color::LightGreen)
            .bg(Color::Rgb(0, 32, 0)),
        DetailLineKind::Delete => Style::default()
            .fg(Color::LightRed)
            .bg(Color::Rgb(48, 0, 0)),
        _ => detail_line_style(kind),
    }
}

fn footer_text(state: &LogState, width: u16) -> String {
    let position = commit_position_label(state.selected_commit, state.data.commits.len());
    let detail_mode = if state.diff_loading {
        "loading"
    } else if state.detail.diff_loaded {
        "diff"
    } else {
        "files"
    };
    let primary_action = if state.diff_loading {
        "wait"
    } else if state.detail.diff_loaded {
        "d top"
    } else {
        "d load"
    };
    if width < 88 {
        return format!(
            "commit {position} | {} | {primary_action} | left/right focus | j/k | q",
            state.active_panel.label()
        );
    }
    format!(
        "commit {position} | focus {} | {detail_mode} | {primary_action} | left/right focus | j/k move/scroll | f/b page | n/p file | q",
        state.active_panel.label()
    )
}

fn commit_position_label(selected: usize, total: usize) -> String {
    if total == 0 {
        "0/0".to_string()
    } else {
        format!("{}/{}", selected.min(total.saturating_sub(1)) + 1, total)
    }
}

fn visible_range_label(start: usize, visible: usize, total: usize) -> String {
    if total == 0 || visible == 0 {
        "0/0".to_string()
    } else {
        let first = start.min(total.saturating_sub(1)) + 1;
        let last = start.saturating_add(visible).min(total);
        format!("{first}-{last}/{total}")
    }
}

fn visible_scroll_start(start: usize, visible: usize, total: usize) -> usize {
    if visible == 0 || total == 0 {
        0
    } else {
        start.min(total.saturating_sub(visible))
    }
}

fn file_jump_target(lines: &[String], current: usize, delta: isize) -> Option<usize> {
    if delta > 0 {
        let start = current.saturating_add(1).min(lines.len());
        (start..lines.len()).find(|idx| is_file_jump_line(&lines[*idx]))
    } else if delta < 0 {
        let end = current.min(lines.len());
        (0..end).rev().find(|idx| is_file_jump_line(&lines[*idx]))
    } else {
        None
    }
}

fn is_file_jump_line(line: &str) -> bool {
    line.starts_with("diff -- ") || is_file_stat_line(line)
}

fn is_file_stat_line(line: &str) -> bool {
    let bytes = line.as_bytes();
    bytes.len() > 2
        && bytes[0] == b' '
        && matches!(bytes[1], b'A' | b'M' | b'D')
        && bytes[2].is_ascii_whitespace()
}

fn scroll_start(selected: usize, visible: usize) -> usize {
    if visible == 0 || selected < visible {
        0
    } else {
        selected + 1 - visible
    }
}

fn format_file_stat(file: &ctx_git::CommitFile) -> String {
    let binary = if file.binary { " binary" } else { "" };
    format!(
        " {} {:>4}+ {:>4}- {}{}",
        status_letter(&file.status),
        file.additions,
        file.deletions,
        file.path,
        binary
    )
}

fn format_diff_line(line: &ctx_git::WorktreeDiffLine) -> String {
    let prefix = match line.typ.as_str() {
        "add" => "+",
        "del" => "-",
        _ => " ",
    };
    format!(
        "{prefix} {} {} | {}",
        format_diff_number(line.old_num),
        format_diff_number(line.new_num),
        line.text,
    )
}

fn format_diff_number(number: i32) -> String {
    if number <= 0 {
        "    ".to_string()
    } else {
        format!("{number:>4}")
    }
}

fn status_letter(status: &str) -> &'static str {
    match status {
        "added" => "A",
        "deleted" => "D",
        _ => "M",
    }
}

fn format_commit_date(date: i64) -> String {
    if date <= 0 {
        "@unknown".to_string()
    } else {
        format!("@{date}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use std::fs;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn os_args(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    fn buffer_to_text(buf: &Buffer) -> String {
        let area = buf.area();
        let mut lines = Vec::with_capacity(area.height as usize);
        for y in 0..area.height {
            let mut line = String::new();
            for x in 0..area.width {
                line.push_str(buf[(x, y)].symbol());
            }
            lines.push(line.trim_end().to_string());
        }
        while lines.last().map(|line| line.is_empty()).unwrap_or(false) {
            lines.pop();
        }
        lines.join("\n")
    }

    fn test_commit(hash: &str, subject: &str) -> LogCommit {
        LogCommit {
            hash: hash.to_string(),
            hash_full: format!("{hash}000"),
            author: "Test".to_string(),
            author_email: "test@example.com".to_string(),
            subject: subject.to_string(),
            date: 1,
            parents: vec!["parent".to_string()],
            matched_paths: Vec::new(),
            is_worktree: false,
        }
    }

    fn test_log_state() -> LogState {
        LogState {
            root: PathBuf::from("."),
            data: LogData {
                root: ".".to_string(),
                source: LogSource {
                    kind: "repo".to_string(),
                    label: "HEAD".to_string(),
                    matched_paths: Vec::new(),
                },
                commits: vec![
                    test_commit("a1b2c3d", "first"),
                    test_commit("b2c3d4e", "second"),
                ],
                truncated: false,
            },
            selected_commit: 0,
            diff_scroll: 0,
            detail: CommitDetail {
                files: Vec::new(),
                lines: vec![
                    "commit a1b2c3d000".to_string(),
                    "diff -- src/lib.rs".to_string(),
                    "+         1 | added".to_string(),
                ],
                diff_loaded: true,
            },
            diff_loading: false,
            error: None,
            active_panel: ActivePanel::Commits,
        }
    }

    fn unique_temp_dir() -> PathBuf {
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "ctx_cli_log_test_{}_{}_{}",
            std::process::id(),
            nanos,
            seq
        ))
    }

    fn git(root: &Path, args: &[&str]) {
        let mut cmd = Command::new("git");
        cmd.args(args).current_dir(root);
        pin_git_env(&mut cmd);
        assert!(cmd.status().expect("git").success(), "git {args:?}");
    }

    fn commit_all(root: &Path, message: &str) {
        git(root, &["add", "-A"]);
        let mut commit = Command::new("git");
        commit
            .args(["commit", "-q", "-m", message])
            .current_dir(root);
        pin_git_env(&mut commit);
        assert!(commit.status().expect("git commit").success());
    }

    fn pin_git_env(cmd: &mut Command) {
        cmd.env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_AUTHOR_NAME", "Log Test")
            .env("GIT_AUTHOR_EMAIL", "log@example.com")
            .env("GIT_COMMITTER_NAME", "Log Test")
            .env("GIT_COMMITTER_EMAIL", "log@example.com")
            .env("GIT_AUTHOR_DATE", "2020-01-02T03:04:05+00:00")
            .env("GIT_COMMITTER_DATE", "2020-01-02T03:04:05+00:00");
    }

    #[test]
    fn parse_defaults_to_tui_with_limit_100() {
        let args = parse_log_args(&os_args(&["log"])).unwrap().expect("parsed");
        assert_eq!(args.limit, 100);
        assert_eq!(args.output_mode, OutputMode::Tui);
    }

    #[test]
    fn parse_accepts_path_query_and_ref_separately() {
        let path_args =
            parse_log_args(&os_args(&["log", "--path", "src/main.rs", "--ref", "main"]))
                .unwrap()
                .expect("parsed");
        assert_eq!(path_args.path.as_deref(), Some("src/main.rs"));
        assert_eq!(path_args.ref_name.as_deref(), Some("main"));

        let query_args = parse_log_args(&os_args(&["log", "--query=auth", "--plain"]))
            .unwrap()
            .expect("parsed");
        assert_eq!(query_args.query.as_deref(), Some("auth"));
        assert_eq!(query_args.output_mode, OutputMode::Plain);
    }

    #[test]
    fn parse_rejects_bad_limit_and_ref() {
        assert!(parse_log_args(&os_args(&["log", "--limit", "0"])).is_err());
        assert!(parse_log_args(&os_args(&["log", "--limit", "501"])).is_err());
        assert!(parse_log_args(&os_args(&["log", "--ref", "-n1"])).is_err());
        assert!(parse_log_args(&os_args(&["log", "--ref", "bad ref"])).is_err());
    }

    #[test]
    fn ref_validator_allows_conservative_branch_names() {
        assert!(is_valid_ref("main"));
        assert!(is_valid_ref("feature/ctx-log_1"));
        assert!(is_valid_ref("deadbeef"));
        assert!(!is_valid_ref("-n1"));
        assert!(!is_valid_ref("bad ref"));
        assert!(!is_valid_ref(""));
    }

    #[test]
    fn tui_log_prepends_worktree_entry_for_dirty_files() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let root = unique_temp_dir();
        fs::create_dir_all(&root).expect("create temp repo");
        git(&root, &["init", "-q", "-b", "main", "."]);
        fs::write(root.join("tracked.txt"), "one\n").expect("write tracked");
        commit_all(&root, "initial");
        fs::write(root.join("tracked.txt"), "one\ntwo\n").expect("modify tracked");

        let args = LogArgs {
            root: root.clone(),
            limit: 10,
            ref_name: None,
            path: None,
            query: None,
            output_mode: OutputMode::Tui,
        };
        let data = load_log_data(&args).expect("load log data");
        assert!(data.commits[0].is_worktree);
        assert_eq!(data.commits[0].hash, "worktree");

        let detail = build_commit_detail(&root, &data.commits[0], true).expect("worktree detail");
        let body = detail.lines.join("\n");
        assert!(body.contains("commit worktree"));
        assert!(body.contains(" M    1+    0- tracked.txt"));
        assert!(body.contains(DIFF_COLUMN_HEADER));
        assert!(body.contains("two"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn plain_log_does_not_include_worktree_entry() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let root = unique_temp_dir();
        fs::create_dir_all(&root).expect("create temp repo");
        git(&root, &["init", "-q", "-b", "main", "."]);
        fs::write(root.join("tracked.txt"), "one\n").expect("write tracked");
        commit_all(&root, "initial");
        fs::write(root.join("tracked.txt"), "one\ntwo\n").expect("modify tracked");

        let args = LogArgs {
            root: root.clone(),
            limit: 10,
            ref_name: None,
            path: None,
            query: None,
            output_mode: OutputMode::Plain,
        };
        let data = load_log_data(&args).expect("load log data");
        assert!(!data.commits[0].is_worktree);
        assert_eq!(data.commits[0].subject, "initial");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn diff_lines_include_old_and_new_number_columns() {
        let added = ctx_git::WorktreeDiffLine {
            typ: "add".to_string(),
            text: "hello".to_string(),
            old_num: 0,
            new_num: 12,
        };
        let deleted = ctx_git::WorktreeDiffLine {
            typ: "del".to_string(),
            text: "bye".to_string(),
            old_num: 7,
            new_num: 0,
        };
        let context = ctx_git::WorktreeDiffLine {
            typ: "ctx".to_string(),
            text: "same".to_string(),
            old_num: 3,
            new_num: 3,
        };

        let added = format_diff_line(&added);
        let deleted = format_diff_line(&deleted);
        let context = format_diff_line(&context);

        assert!(added.starts_with("+ "));
        assert!(added.contains("  12 | hello"));
        assert!(deleted.starts_with("- "));
        assert!(deleted.contains("   7      | bye"));
        assert!(context.starts_with("  "));
        assert!(context.contains("   3    3 | same"));
    }

    #[test]
    fn detail_line_classification_matches_git_diff_semantics() {
        assert_eq!(
            classify_detail_line("+         1 | added"),
            DetailLineKind::Add
        );
        assert_eq!(
            classify_detail_line("-    1      | removed"),
            DetailLineKind::Delete
        );
        assert_eq!(
            classify_detail_line("     1    1 | context"),
            DetailLineKind::Context
        );
        assert_eq!(
            classify_detail_line(DIFF_COLUMN_HEADER),
            DetailLineKind::DiffColumnHeader
        );
        assert_eq!(
            classify_detail_line("diff -- src/main.rs"),
            DetailLineKind::FileHeader
        );
        assert_eq!(
            classify_detail_line(" A    3+    0- src/new.rs"),
            DetailLineKind::FileStatAdd
        );
        assert_eq!(
            classify_detail_line(" D    0+    3- src/old.rs"),
            DetailLineKind::FileStatDelete
        );
        assert_eq!(
            classify_detail_line(" M    2+    1- src/lib.rs"),
            DetailLineKind::FileStatModify
        );
        assert_eq!(
            classify_detail_line("  [binary] file changed"),
            DetailLineKind::Notice
        );
        assert_eq!(
            classify_detail_line("  [no text changes]"),
            DetailLineKind::Notice
        );
        assert_eq!(classify_detail_line("    subject"), DetailLineKind::Subject);
    }

    #[test]
    fn detail_line_styles_use_high_contrast_diff_colors() {
        assert_eq!(
            detail_line_style(DetailLineKind::Add).fg,
            Some(Color::LightGreen)
        );
        assert_eq!(
            detail_line_style(DetailLineKind::Delete).fg,
            Some(Color::LightRed)
        );
        assert_eq!(
            detail_line_style(DetailLineKind::FileHeader).fg,
            Some(Color::LightCyan)
        );
        assert_eq!(
            detail_line_style(DetailLineKind::FileStatModify).fg,
            Some(Color::LightYellow)
        );
        assert_eq!(
            detail_line_style(DetailLineKind::DiffColumnHeader).fg,
            Some(Color::DarkGray)
        );
        assert_eq!(
            detail_line_style(DetailLineKind::Notice).fg,
            Some(Color::LightMagenta)
        );
        assert_eq!(
            diff_body_style(DetailLineKind::Add).bg,
            Some(Color::Rgb(0, 32, 0))
        );
        assert_eq!(
            diff_body_style(DetailLineKind::Delete).bg,
            Some(Color::Rgb(48, 0, 0))
        );
    }

    #[test]
    fn file_stat_line_colors_additions_and_deletions_separately() {
        let line =
            styled_file_stat_line(" M    2+    1- src/lib.rs", DetailLineKind::FileStatModify);
        assert_eq!(line.spans[3].content.as_ref(), "   2+");
        assert_eq!(line.spans[3].style.fg, Some(Color::LightGreen));
        assert_eq!(line.spans[5].content.as_ref(), "   1-");
        assert_eq!(line.spans[5].style.fg, Some(Color::LightRed));
    }

    #[test]
    fn ratatui_render_frame_contains_diff_columns() {
        let diff_line = format_diff_line(&ctx_git::WorktreeDiffLine {
            typ: "add".to_string(),
            text: "new line".to_string(),
            old_num: 0,
            new_num: 2,
        });
        let state = LogState {
            root: PathBuf::from("."),
            data: LogData {
                root: ".".to_string(),
                source: LogSource {
                    kind: "repo".to_string(),
                    label: "HEAD".to_string(),
                    matched_paths: Vec::new(),
                },
                commits: vec![LogCommit {
                    hash: "a1b2c3d".to_string(),
                    hash_full: "a1b2c3d4".to_string(),
                    author: "Test".to_string(),
                    author_email: "test@example.com".to_string(),
                    subject: "clean diff rendering".to_string(),
                    date: 1,
                    parents: vec!["parent".to_string()],
                    matched_paths: Vec::new(),
                    is_worktree: false,
                }],
                truncated: false,
            },
            selected_commit: 0,
            diff_scroll: 0,
            detail: CommitDetail {
                files: vec![ctx_git::CommitFile {
                    status: "modified".to_string(),
                    path: "src/lib.rs".to_string(),
                    additions: 1,
                    deletions: 0,
                    binary: false,
                }],
                lines: vec![
                    "commit a1b2c3d4".to_string(),
                    "Author: Test <test@example.com>".to_string(),
                    "Date:   @1".to_string(),
                    String::new(),
                    "    clean diff rendering".to_string(),
                    String::new(),
                    "1 file(s) changed".to_string(),
                    " M    1+    0- src/lib.rs".to_string(),
                    String::new(),
                    "diff -- src/lib.rs".to_string(),
                    " M    1+    0- src/lib.rs".to_string(),
                    DIFF_COLUMN_HEADER.to_string(),
                    diff_line.clone(),
                ],
                diff_loaded: true,
            },
            diff_loading: false,
            error: None,
            active_panel: ActivePanel::Detail,
        };

        let backend = TestBackend::new(100, 20);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| render_log_view(frame, &state))
            .expect("draw");
        let rendered = buffer_to_text(terminal.backend().buffer());

        assert!(rendered.contains("ctx log"));
        assert!(rendered.contains("clean diff rendering"));
        assert!(rendered.contains("file src/lib.rs"));
        assert!(rendered.contains(DIFF_COLUMN_HEADER));
        assert!(rendered.contains(&diff_line));
        assert!(rendered.contains("[focus]"));
    }

    #[test]
    fn active_panel_switches_with_left_and_right_behavior() {
        let mut state = test_log_state();
        assert_eq!(state.active_panel, ActivePanel::Commits);

        state.focus_detail();
        state.move_active_down();
        assert_eq!(state.diff_scroll, 1);
        assert_eq!(state.selected_commit, 0);

        state.move_active_up();
        assert_eq!(state.diff_scroll, 0);

        state.focus_commits();
        state.move_active_down();
        assert_eq!(state.selected_commit, 1);

        state.move_active_up();
        assert_eq!(state.selected_commit, 0);
    }

    #[test]
    fn footer_text_shortens_for_narrow_terminals() {
        let state = test_log_state();
        let narrow = footer_text(&state, 60);
        assert!(narrow.contains("left/right focus"));
        assert!(!narrow.contains("f/b page"));

        let wide = footer_text(&state, 120);
        assert!(wide.contains("f/b page"));
        assert!(wide.contains("n/p file"));
    }

    #[test]
    fn commit_position_label_clamps_selection() {
        assert_eq!(commit_position_label(0, 0), "0/0");
        assert_eq!(commit_position_label(0, 3), "1/3");
        assert_eq!(commit_position_label(2, 3), "3/3");
        assert_eq!(commit_position_label(99, 3), "3/3");
    }

    #[test]
    fn visible_range_label_reports_scroll_window() {
        assert_eq!(visible_range_label(0, 0, 25), "0/0");
        assert_eq!(visible_range_label(0, 10, 0), "0/0");
        assert_eq!(visible_range_label(0, 10, 25), "1-10/25");
        assert_eq!(visible_range_label(20, 10, 25), "21-25/25");
        assert_eq!(visible_range_label(99, 10, 25), "25-25/25");
    }

    #[test]
    fn visible_scroll_start_keeps_last_page_dense() {
        assert_eq!(visible_scroll_start(0, 0, 25), 0);
        assert_eq!(visible_scroll_start(0, 10, 0), 0);
        assert_eq!(visible_scroll_start(0, 10, 25), 0);
        assert_eq!(visible_scroll_start(20, 10, 25), 15);
        assert_eq!(visible_scroll_start(99, 10, 25), 15);
        assert_eq!(visible_scroll_start(99, 30, 25), 0);
    }

    #[test]
    fn file_jump_target_moves_between_file_boundaries() {
        let lines = vec![
            "commit abc".to_string(),
            " M    2+    1- src/lib.rs".to_string(),
            "+         1 | added".to_string(),
            "diff -- src/lib.rs".to_string(),
            "     1    1 | context".to_string(),
            "diff -- src/main.rs".to_string(),
        ];

        assert_eq!(file_jump_target(&lines, 0, 1), Some(1));
        assert_eq!(file_jump_target(&lines, 1, 1), Some(3));
        assert_eq!(file_jump_target(&lines, 4, 1), Some(5));
        assert_eq!(file_jump_target(&lines, 5, 1), None);
        assert_eq!(file_jump_target(&lines, 5, -1), Some(3));
        assert_eq!(file_jump_target(&lines, 3, -1), Some(1));
    }

    #[test]
    fn file_jump_line_ignores_regular_diff_body() {
        assert!(is_file_jump_line("diff -- src/lib.rs"));
        assert!(is_file_jump_line(" A    3+    0- src/new.rs"));
        assert!(is_file_jump_line(" D    0+    3- src/old.rs"));
        assert!(is_file_jump_line(" M    2+    1- src/lib.rs"));
        assert!(!is_file_jump_line("+         1 | added"));
        assert!(!is_file_jump_line("-    1      | removed"));
        assert!(!is_file_jump_line("     1    1 | context"));
    }
}
