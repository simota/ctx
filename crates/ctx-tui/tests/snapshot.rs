// crates/ctx-tui/tests/snapshot.rs
//
// Frame-snapshot oracle for the ratatui port of internal/tui.
//
// For each scripted session, this test:
//   1. builds the fixed golden_fixture() Model,
//   2. sends the 80x24 window size,
//   3. replays the FIXED scripted key sequence (identical to the Go
//      exporter, cmd/tui-golden-export), rendering to an 80x24 ratatui
//      Buffer after each step,
//   4. extracts the cell TEXT grid (ANSI-free by construction — a ratatui
//      Buffer holds plain symbols, no escape codes),
//   5. asserts it byte-equals the corresponding ANSI-stripped golden
//      frame captured from the frozen Go tui.
//
// The goldens live in tests/goldens/<session>.txt, one file per session,
// frames separated by the "===== FRAME N =====" delimiter.
//
// CONTENT-ONLY CARVE-OUT: we compare cell text (content + layout), NOT
// colour/style. See ../TUI_ORACLE.md.
//
// CURRENT STATUS: the snapshot oracle is active. A mismatch here is a real
// content/layout regression against the frozen Go tui reference.

use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::Terminal;

use ctx_tui::{golden_fixture, Key, Model};

const WIDTH: u16 = 80;
const HEIGHT: u16 = 24;
const FRAME_DELIM_PREFIX: &str = "===== FRAME ";

/// The scripted sessions, mirroring sessions() in
/// cmd/tui-golden-export/main.go. (session name -> script tokens.)
fn sessions() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        (
            "nav_toggle_open",
            vec![
                "down", "enter", "down", "space", "up", "left", "down", "enter", "down", "space",
            ],
        ),
        (
            "expand_all_scroll",
            vec![
                "down", "enter", "down", "down", "enter", "G", "g", "down", "down",
            ],
        ),
    ]
}

/// Render the model to an 80x24 buffer and return its text grid, trimmed
/// to match the golden normalisation (trailing spaces per line stripped,
/// trailing blank lines stripped).
fn render_frame(model: &Model) -> String {
    let backend = TestBackend::new(WIDTH, HEIGHT);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|f| ctx_tui::render(model, f)).expect("draw");
    buffer_to_text(terminal.backend().buffer())
}

/// Extract the plain-text grid from a ratatui Buffer (no ANSI by
/// construction), then normalise: trim trailing whitespace per row and
/// drop trailing blank rows.
fn buffer_to_text(buf: &Buffer) -> String {
    let area = buf.area();
    let mut lines: Vec<String> = Vec::with_capacity(area.height as usize);
    for y in 0..area.height {
        let mut line = String::new();
        for x in 0..area.width {
            line.push_str(buf[(x, y)].symbol());
        }
        lines.push(line.trim_end().to_string());
    }
    normalise(lines)
}

/// Join lines, dropping trailing blank lines (the Go frames have no
/// trailing padding; the ratatui buffer pads to 24 rows).
fn normalise(mut lines: Vec<String>) -> String {
    while lines.last().map(|l| l.is_empty()).unwrap_or(false) {
        lines.pop();
    }
    lines.join("\n")
}

/// Parse a golden file into per-frame normalised strings.
fn parse_goldens(name: &str) -> Vec<String> {
    let path = format!("{}/tests/goldens/{}.txt", env!("CARGO_MANIFEST_DIR"), name);
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read golden {path}: {e}"));

    let mut frames: Vec<String> = Vec::new();
    let mut current: Option<Vec<String>> = None;
    for line in raw.lines() {
        if line.starts_with(FRAME_DELIM_PREFIX) {
            if let Some(lines) = current.take() {
                frames.push(normalise(lines));
            }
            current = Some(Vec::new());
        } else if let Some(lines) = current.as_mut() {
            lines.push(line.to_string());
        }
    }
    if let Some(lines) = current.take() {
        frames.push(normalise(lines));
    }
    frames
}

/// Drive a session and compare every frame to its golden. STEP 0 is the
/// initial frame (after window size, before any key); step i renders
/// after applying script[i-1].
fn run_session(name: &str, script: &[&str]) {
    let goldens = parse_goldens(name);
    assert_eq!(
        goldens.len(),
        script.len() + 1,
        "session {name}: golden frame count ({}) != script steps + 1 ({})",
        goldens.len(),
        script.len() + 1,
    );

    let mut model = Model::new(golden_fixture());
    model.set_size(WIDTH, HEIGHT);

    // STEP 0
    assert_frame(name, 0, &render_frame(&model), &goldens[0]);

    for (i, tok) in script.iter().enumerate() {
        model.update(Key::parse(tok));
        assert_frame(name, i + 1, &render_frame(&model), &goldens[i + 1]);
    }
}

fn assert_frame(session: &str, step: usize, got: &str, want: &str) {
    pretty_assertions::assert_eq!(
        got,
        want,
        "session {session} frame {step}: rendered grid != golden (content/layout mismatch)",
    );
}

// Per-session #[test] cases so a port loop can count green/red sessions.

#[test]
fn snapshot_nav_toggle_open() {
    run_session("nav_toggle_open", &sessions()[0].1);
}

#[test]
fn snapshot_expand_all_scroll() {
    run_session("expand_all_scroll", &sessions()[1].1);
}
