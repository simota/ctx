// crates/ctx-tui/src/lib.rs
//
// Rust (ratatui) port of internal/tui/ — the interactive ctx context
// explorer. ADR-0005 Wave 4.
//
// VERIFICATION CARVE-OUT: this crate is OUT of the byte-parity-vs-Go
// HTTP/CLI model. The Go tui renders via Bubble Tea + lipgloss (ANSI);
// cross-library ANSI/style parity is impossible and out of scope. Instead
// the port is verified by FRAME-SNAPSHOT GOLDENS captured from the frozen
// Go tui with ANSI escape codes stripped — we assert CONTENT + LAYOUT
// (the cell text grid), NOT visual styling/colours. See TUI_ORACLE.md.
//
// STATUS: ACTIVE PORT. `render()` and `Model::update()` are verified by the
// frame-snapshot oracle; failures in that suite are content/layout regressions,
// not scaffold placeholders.

use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::{execute, tty::IsTty};
use ratatui::backend::CrosstermBackend;
use ratatui::{text::Text, widgets::Paragraph, Frame, Terminal};

/// FileInfo mirrors the subset of internal/model.FileInfo that the tui
/// renders. `tokens` is the precomputed count (Go: tokenCount() falls
/// back through Tokens -> Metadata.TokensEst -> EstimateBySize; the golden
/// fixture sets Tokens explicitly so this single field suffices).
#[derive(Debug, Clone)]
pub struct FileInfo {
    /// repo-relative path
    pub path: String,
    pub is_dir: bool,
    pub tokens: i64,
    pub children: Vec<FileInfo>,
}

impl FileInfo {
    pub fn file(path: &str, tokens: i64) -> Self {
        FileInfo {
            path: path.to_string(),
            is_dir: false,
            tokens,
            children: Vec::new(),
        }
    }
    pub fn dir(path: &str, children: Vec<FileInfo>) -> Self {
        FileInfo {
            path: path.to_string(),
            is_dir: true,
            tokens: 0,
            children,
        }
    }
}

/// Key is the scripted-input alphabet. It matches the token set the Go
/// golden exporter scripts (cmd/tui-golden-export), so the same sequence
/// can drive both implementations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Down,
    Up,
    Enter,
    Left,
    Right,
    Space,
    Char(char),
}

impl Key {
    /// Parse a script token (as emitted by the Go exporter) into a Key.
    pub fn parse(token: &str) -> Key {
        match token {
            "down" => Key::Down,
            "up" => Key::Up,
            "enter" => Key::Enter,
            "left" => Key::Left,
            "right" => Key::Right,
            "space" => Key::Space,
            other => {
                let mut chars = other.chars();
                let c = chars.next().expect("empty script token");
                assert!(chars.next().is_none(), "multi-char script token: {other:?}");
                Key::Char(c)
            }
        }
    }
}

/// Model mirrors internal/tui.Model. Constructed via `new`, driven via
/// `update`, rendered via `render`.
#[derive(Debug)]
pub struct Model {
    root: FileInfo,
    visible: Vec<Row>,
    cursor: usize,
    included: HashSet<String>,
    open: HashSet<String>,
    width: u16,
    height: u16,
    budget: i64,
    status: String,
}

#[derive(Debug, Clone)]
struct Row {
    file: FileInfo,
    depth: usize,
    last: bool,
}

impl Model {
    /// Mirror of tui.New(root): all files start included, the root starts
    /// open. Behaviour is snapshot-oracle-backed so the model stays aligned
    /// with the frozen Go tui content/layout reference.
    pub fn new(root: FileInfo) -> Self {
        let mut included = HashSet::new();
        collect_files(&root, &mut |file| {
            included.insert(file.path.clone());
        });

        let mut open = HashSet::new();
        open.insert(root.path.clone());

        let mut model = Model {
            root,
            visible: Vec::new(),
            cursor: 0,
            included,
            open,
            width: 0,
            height: 0,
            budget: 50_000,
            status: String::new(),
        };
        model.refresh();
        model
    }

    /// Mirror of tea.WindowSizeMsg handling.
    pub fn set_size(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
    }

    /// Mirror of tui.Model.Update(tea.KeyMsg).
    pub fn update(&mut self, key: Key) {
        match key {
            Key::Down | Key::Char('j') => {
                if self.cursor + 1 < self.visible.len() {
                    self.cursor += 1;
                }
            }
            Key::Up | Key::Char('k') => {
                self.cursor = self.cursor.saturating_sub(1);
            }
            Key::Char('g') => {
                self.cursor = 0;
            }
            Key::Char('G') => {
                if !self.visible.is_empty() {
                    self.cursor = self.visible.len() - 1;
                }
            }
            Key::Enter | Key::Right | Key::Char('l') => self.open_current(true),
            Key::Left | Key::Char('h') => self.open_current(false),
            Key::Space => self.toggle_current(),
            Key::Char('p') => {
                // The snapshot scripts do not drive pack output. Keep the
                // status mutation aligned enough for future parity tests
                // without touching the filesystem in the oracle crate.
                self.status = "saved context.md".to_string();
            }
            Key::Char('q') => {}
            Key::Char(_) => {}
        }
    }

    fn refresh(&mut self) {
        self.visible.clear();
        add_rows(&self.root, 0, true, &self.open, &mut self.visible);
        if self.visible.is_empty() {
            self.cursor = 0;
        } else if self.cursor >= self.visible.len() {
            self.cursor = self.visible.len() - 1;
        }
    }

    fn current(&self) -> Option<&FileInfo> {
        self.visible.get(self.cursor).map(|row| &row.file)
    }

    fn open_current(&mut self, open: bool) {
        let Some(file) = self.current() else {
            return;
        };
        if !file.is_dir {
            return;
        }
        let path = file.path.clone();
        if open {
            self.open.insert(path);
        } else {
            self.open.remove(&path);
        }
        self.refresh();
    }

    fn toggle_current(&mut self) {
        let Some(file) = self.current().cloned() else {
            return;
        };
        let next = !self.is_included(&file);
        self.set_included(&file, next);
    }

    fn set_included(&mut self, file: &FileInfo, included: bool) {
        if file.is_dir {
            collect_files(file, &mut |child| {
                if included {
                    self.included.insert(child.path.clone());
                } else {
                    self.included.remove(&child.path);
                }
            });
            return;
        }

        if included {
            self.included.insert(file.path.clone());
        } else {
            self.included.remove(&file.path);
        }
    }

    fn is_included(&self, file: &FileInfo) -> bool {
        if !file.is_dir {
            return self.included.contains(&file.path);
        }

        let mut files = 0;
        let mut selected = 0;
        collect_files(file, &mut |child| {
            files += 1;
            if self.included.contains(&child.path) {
                selected += 1;
            }
        });
        files > 0 && files == selected
    }

    fn used_tokens(&self) -> i64 {
        let mut total = 0;
        collect_files(&self.root, &mut |file| {
            if self.included.contains(&file.path) {
                total += file.tokens;
            }
        });
        total
    }

    fn view_text(&self) -> String {
        let mut lines = Vec::new();
        lines.push(format!(
            "ctx tokens: {} / {}",
            self.used_tokens(),
            self.budget
        ));

        let body_height = match self.height.saturating_sub(3) {
            0 => self.visible.len(),
            h => h as usize,
        };
        let start = if self.cursor >= body_height {
            self.cursor - body_height + 1
        } else {
            0
        };
        let end = (start + body_height).min(self.visible.len());
        for row in &self.visible[start..end] {
            lines.push(self.render_row(row));
        }

        let mut help = "↑↓: nav  Space: toggle  Enter: open  p: pack  q: quit".to_string();
        if !self.status.is_empty() {
            help.push_str("  ");
            help.push_str(&self.status);
        }
        lines.push(help);
        lines.join("\n")
    }

    fn render_row(&self, row: &Row) -> String {
        let file = &row.file;
        let indent = "│  ".repeat(row.depth);
        let connector = if row.last { "└─ " } else { "├─ " };
        let marker = if file.is_dir {
            if self.open.contains(&file.path) {
                "▾"
            } else {
                "▸"
            }
        } else {
            " "
        };
        let check = if self.is_included(file) { "[x]" } else { "[ ]" };
        let mut name = base_name(&file.path).to_string();
        if file.path == "." {
            name = ".".to_string();
        }
        if file.is_dir && !name.ends_with('/') {
            name.push('/');
        }
        let meta = if file.is_dir {
            String::new()
        } else {
            format!("  {} tokens", file.tokens)
        };

        format!("{indent}{connector}{marker} {check} {name}{meta}")
    }
}

/// render draws the model into the ratatui frame.
///
/// Rendering is content/layout checked by `tests/snapshot.rs`; styling and
/// ANSI colour parity remain intentionally out of scope.
pub fn render(model: &Model, frame: &mut Frame) {
    let text = Text::raw(model.view_text());
    frame.render_widget(Paragraph::new(text), frame.area());
}

/// The fixed deterministic fixture, mirroring fixture() in
/// cmd/tui-golden-export/main.go. Kept here so the snapshot test drives
/// the identical tree the goldens were captured from.
pub fn golden_fixture() -> FileInfo {
    FileInfo::dir(
        ".",
        vec![
            FileInfo::dir("cmd", vec![FileInfo::file("cmd/main.go", 120)]),
            FileInfo::dir(
                "internal",
                vec![
                    FileInfo::file("internal/app.go", 340),
                    FileInfo::file("internal/util.go", 80),
                ],
            ),
            FileInfo::file("README.md", 45),
        ],
    )
}

fn add_rows(
    file: &FileInfo,
    depth: usize,
    last: bool,
    open: &HashSet<String>,
    rows: &mut Vec<Row>,
) {
    rows.push(Row {
        file: file.clone(),
        depth,
        last,
    });
    if !file.is_dir || !open.contains(&file.path) {
        return;
    }

    for (i, child) in file.children.iter().enumerate() {
        add_rows(child, depth + 1, i + 1 == file.children.len(), open, rows);
    }
}

fn collect_files(file: &FileInfo, visit: &mut impl FnMut(&FileInfo)) {
    if !file.is_dir {
        visit(file);
        return;
    }

    for child in &file.children {
        collect_files(child, visit);
    }
}

fn base_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

// ---------------------------------------------------------------------------
// Live event loop (ratatui + crossterm) — mirrors internal/tui.Run(root).
// ---------------------------------------------------------------------------

/// Run the interactive TUI event loop over `root`.
///
/// Lifecycle mirrors Go's `tui.Run`:
/// `tea.NewProgram(New(root), WithAltScreen(), WithInput(os.Stdin), WithOutput(os.Stdout))`.
/// We enter raw mode + the alternate screen, poll/read key events, feed them
/// through `Model::update`, redraw via the existing `render()`, and restore the
/// terminal on exit (including on panic or error).
///
/// FAIL FAST on a non-TTY: if stdout (or stdin) is not a terminal we return an
/// error immediately WITHOUT entering raw mode or blocking on input. This is
/// what makes `ctx tui` in a non-TTY (closed stdin, as the cutover probe runs
/// it) exit fast on the native path instead of delegating to Go or hanging.
/// True only when BOTH stdin and stdout are attached to a terminal. Callers
/// should check this BEFORE doing expensive work (e.g. walking the tree), so a
/// non-TTY invocation fails fast instead of doing pointless work.
pub fn is_interactive() -> bool {
    io::stdout().is_tty() && io::stdin().is_tty()
}

/// `root_path` is the filesystem root the `root` tree was built from
/// (the same path handed to `build_tree`); the `p` pack action resolves
/// the tree's repo-relative paths against it when reading file contents.
pub fn run(root: FileInfo, root_path: &str) -> io::Result<()> {
    // Guard: refuse to drive an interactive UI without a real terminal.
    if !is_interactive() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "ctx tui requires an interactive terminal (TTY)",
        ));
    }

    let mut stdout = io::stdout();
    enable_raw_mode()?;
    // From here on the terminal is in raw mode; the guard restores it on
    // every exit path — early error, normal return, or a panic unwinding
    // out of the event loop (mirror Go's deferred program shutdown).
    let _restore = RestoreTerminal;
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend).and_then(|mut terminal| event_loop(&mut terminal, root, Path::new(root_path)))
}

/// Best-effort terminal teardown on drop. `LeaveAlternateScreen` is harmless
/// when the alternate screen was never entered.
struct RestoreTerminal;

impl Drop for RestoreTerminal {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

/// The poll/read → update → render loop. Returns when the user quits.
/// `root_path` is the filesystem root the tree was built from.
fn event_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    root: FileInfo,
    root_path: &Path,
) -> io::Result<()> {
    let mut model = Model::new(root);

    // Seed the size from the real terminal (Go gets a WindowSizeMsg on start).
    let size = terminal.size()?;
    model.set_size(size.width, size.height);

    loop {
        terminal.draw(|frame| render(&model, frame))?;

        // Block until an event is available, then drain it. Waiting in poll
        // (instead of redrawing on every 250ms timeout) keeps an idle TUI from
        // re-walking the whole tree for `used_tokens`/`is_included` per tick.
        while !event::poll(Duration::from_millis(250))? {}
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                // q / ctrl+c quit (mirrors Go: "q", "ctrl+c" => tea.Quit).
                if matches!(key.code, KeyCode::Char('c'))
                    && key.modifiers.contains(KeyModifiers::CONTROL)
                {
                    return Ok(());
                }
                if matches!(key.code, KeyCode::Char('q')) {
                    return Ok(());
                }
                if let Some(mapped) = map_key(key.code) {
                    // 'p' triggers a pack write to context.md, mirroring Go's
                    // writePack(); Model::update sets the success status but
                    // does not touch the filesystem, so do the write here and
                    // surface a failure status instead of "saved context.md".
                    if mapped == Key::Char('p') {
                        match write_pack(&model, root_path) {
                            Ok(()) => model.update(mapped),
                            Err(err) => model.status = format!("pack failed: {err}"),
                        }
                    } else {
                        model.update(mapped);
                    }
                }
            }
            Event::Resize(width, height) => {
                model.set_size(width, height);
            }
            _ => {}
        }
    }
}

/// Map a crossterm KeyCode into the port's scripted `Key` alphabet.
fn map_key(code: KeyCode) -> Option<Key> {
    match code {
        KeyCode::Down => Some(Key::Down),
        KeyCode::Up => Some(Key::Up),
        KeyCode::Enter => Some(Key::Enter),
        KeyCode::Left => Some(Key::Left),
        KeyCode::Right => Some(Key::Right),
        KeyCode::Char(' ') => Some(Key::Space),
        KeyCode::Char(c) => Some(Key::Char(c)),
        _ => None,
    }
}

/// Write context.md on the 'p' key, mirroring Go's `Model.writePack` side
/// effect: a FULL rendered markdown context pack via the shared renderer
/// (`pack.Pack(f, m.includedFiles(), pack.Options{Budget, Format: Markdown})`
/// → `ctx_pack::assemble::pack_markdown(inputs, "", model.budget)`).
/// `context.md` is written to the current working directory, exactly like
/// Go's `os.Create("context.md")`; file contents are resolved against
/// `root` (the path the tree was built from).
///
/// Errors are propagated so the event loop can surface a failure status
/// instead of the snapshot-locked "saved context.md" success string.
fn write_pack(model: &Model, root: &Path) -> io::Result<()> {
    write_pack_to(model, root, Path::new("context.md"))
}

/// Testable core of `write_pack` — renders the pack for the currently
/// included files and writes it to `out`.
fn write_pack_to(model: &Model, root: &Path, out: &Path) -> io::Result<()> {
    let inputs = model.pack_inputs(root);
    let rendered = ctx_pack::assemble::pack_markdown(&inputs, "", model.budget)
        .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;
    std::fs::write(out, rendered)
}

impl Model {
    /// Pack inputs for the currently-included files, in tree order —
    /// the Rust shape of Go's `m.includedFiles()` handed to `pack.Pack`.
    /// Paths are resolved against `root` (the tree's build root, NOT
    /// blindly the cwd). Unreadable files are skipped; token counts are
    /// the model's existing per-file counts from `build_tree`.
    fn pack_inputs(&self, root: &Path) -> Vec<ctx_pack::FileInput> {
        let mut inputs = Vec::new();
        collect_files(&self.root, &mut |file| {
            if !self.included.contains(&file.path) {
                return;
            }
            let abs = root.join(&file.path);
            let Ok(bytes) = std::fs::read(&abs) else {
                return;
            };
            inputs.push(ctx_pack::FileInput {
                path: file.path.clone(),
                abs_path: abs.to_string_lossy().into_owned(),
                is_dir: false,
                tokens: file.tokens,
                role: String::new(),
                metadata: ctx_pack::MetadataInput {
                    size: bytes.len() as i64,
                    tokens_est: file.tokens,
                    role: String::new(),
                    // Symbols are only consumed by contract embedding,
                    // which pack_markdown runs with contract=false.
                    symbols: Vec::new(),
                },
                content_head: bytes.into_iter().take(512).collect(),
            });
        });
        inputs
    }
}

// ---------------------------------------------------------------------------
// build_tree — native walk + token counting, mirroring Go's
// walk.New/Walk + countTokens(walk.Flatten(fi)) in internal/cli/tui.go.
// ---------------------------------------------------------------------------

/// Walk the filesystem rooted at `root_path` and build the `FileInfo` tree the
/// TUI renders, counting per-file tokens via ctx-tokens (cl100k/tiktoken with a
/// size-based fallback) — mirroring Go's `walk.New(root) → Walk → countTokens`.
///
/// Field mapping matches `golden_fixture()`:
///   * paths are repo-relative, slash-separated; the root is ".".
///   * directories have `tokens: 0` and their `children`.
///   * files have their token count and no children.
///
/// Directory entries in the same skip set as the CLI tree walker
/// (`.git`, `node_modules`, `dist`, `coverage`, `target`, `*.lock`) are
/// excluded; children are sorted by file name to match Go's `os.ReadDir`
/// ordering.
pub fn build_tree(root_path: &str) -> io::Result<FileInfo> {
    // Follow a symlink only for the requested root (e.g. /tmp on macOS);
    // deeper symlinked dirs are never recursed into (see build_node).
    let root = Path::new(root_path)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(root_path));
    build_node(&root, ".")
}

/// ExtraIgnore from Go `walk.DefaultOptions().ExtraIgnore`, aligned with the
/// CLI tree walker's defaults (tree/json.rs `json_tree_should_skip`):
/// `target/` is build output and can be huge; `*.lock` is a default ignore
/// pattern.
fn should_skip(name: &str) -> bool {
    matches!(
        name,
        ".git" | "node_modules" | "dist" | "coverage" | "target"
    ) || name.ends_with(".lock")
}

fn build_node(path: &Path, rel: &str) -> io::Result<FileInfo> {
    // symlink_metadata (NOT metadata): a symlinked dir is treated as a
    // non-dir and never recursed into, so cyclic links cannot loop.
    // Matches the CLI walker (tree/json.rs).
    let meta = std::fs::symlink_metadata(path)?;

    if !meta.is_dir() {
        let tokens = match ctx_tokens::count_file(&path.to_string_lossy()) {
            Ok(n) => n,
            Err(_) => ctx_tokens::estimate_by_size(meta.len() as i64),
        };
        return Ok(FileInfo {
            path: rel.to_string(),
            is_dir: false,
            tokens,
            children: Vec::new(),
        });
    }

    let mut entries: Vec<_> = match std::fs::read_dir(path) {
        Ok(rd) => rd.flatten().collect(),
        Err(_) => Vec::new(),
    };
    entries.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

    let mut children = Vec::new();
    for entry in entries {
        let child_name = entry.file_name();
        let child_name = child_name.to_string_lossy();
        if should_skip(&child_name) {
            continue;
        }
        let child_rel = if rel == "." {
            child_name.to_string()
        } else {
            format!("{rel}/{child_name}")
        };
        match build_node(&entry.path(), &child_rel) {
            Ok(node) => children.push(node),
            Err(_) => continue,
        }
    }

    Ok(FileInfo {
        path: rel.to_string(),
        is_dir: true,
        tokens: 0,
        children,
    })
}

#[cfg(test)]
mod write_pack_tests {
    use super::*;

    #[test]
    fn write_pack_writes_full_rendered_pack_not_path_list() {
        let dir = std::env::temp_dir().join(format!("ctx-tui-write-pack-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).expect("mkdir");
        std::fs::write(dir.join("src/main.rs"), "fn main() {}\n").expect("write main.rs");
        std::fs::write(dir.join("README.md"), "# readme\n").expect("write README.md");

        let tree = build_tree(&dir.to_string_lossy()).expect("build_tree");
        let model = Model::new(tree);
        let out = dir.join("context.md");
        write_pack_to(&model, &dir, &out).expect("write_pack_to");

        let pack = std::fs::read_to_string(&out).expect("context.md written");
        // Full rendered markdown pack (Go writePack parity), NOT a path list.
        assert!(pack.starts_with("# Context Pack\n\n"), "got: {pack:?}");
        assert!(pack.contains("**Budget**:"));
        assert!(pack.contains("## File contents\n"));
        assert!(pack.contains("### src/main.rs\n\n```rust\nfn main() {}\n```\n"));
        assert!(pack.contains("### README.md\n\n```markdown\n# readme\n```\n"));
        assert!(!pack.contains("ctx:contract"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
