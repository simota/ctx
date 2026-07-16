use std::ffi::{OsStr, OsString};
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::{Command as StdCommand, ExitCode, Output};
use std::time::Duration;

const DEFAULT_DEBOUNCE: Duration = Duration::from_millis(200);
const POLL_INTERVAL: Duration = Duration::from_millis(100);
const HELP: &str = r#"Show or watch the working tree diff

Usage: ctx diff [ROOT] [OPTIONS]

Arguments:
  [ROOT]                 Repository root (default: .)

Options:
      --watch            Watch for diff changes
      --debounce <DURATION>
                         Quiet period before a watch event (default: 200ms)
      --path <PATH>      Limit output to PATH (repeatable)
  -h, --help             Print help
"#;

#[derive(Debug)]
struct DiffOptions {
    root: PathBuf,
    watch: bool,
    debounce: Duration,
    paths: Vec<OsString>,
}

pub(crate) fn run_diff_command(args: &[OsString]) -> Option<ExitCode> {
    if args
        .iter()
        .skip(1)
        .any(|arg| arg == OsStr::new("--help") || arg == OsStr::new("-h"))
    {
        print!("{HELP}");
        return Some(ExitCode::SUCCESS);
    }
    let options = match parse_args(args) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("ctx diff: {message}");
            return Some(ExitCode::from(2));
        }
    };

    let result = if options.watch {
        run_watch(&options)
    } else {
        run_once(&options)
    };
    Some(match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("ctx diff: {message}");
            ExitCode::FAILURE
        }
    })
}

fn parse_args(args: &[OsString]) -> Result<DiffOptions, String> {
    let mut root = None;
    let mut watch = false;
    let mut debounce = DEFAULT_DEBOUNCE;
    let mut debounce_set = false;
    let mut paths = Vec::new();
    let mut index = 1;

    while index < args.len() {
        let arg = &args[index];
        if arg == OsStr::new("--watch") {
            watch = true;
        } else if arg == OsStr::new("--debounce") {
            index += 1;
            let value = args
                .get(index)
                .ok_or_else(|| "--debounce requires a duration".to_string())?;
            debounce = parse_duration(value)?;
            debounce_set = true;
        } else if let Some(value) = arg.to_string_lossy().strip_prefix("--debounce=") {
            debounce = parse_duration(OsStr::new(value))?;
            debounce_set = true;
        } else if arg == OsStr::new("--path") {
            index += 1;
            let value = args
                .get(index)
                .ok_or_else(|| "--path requires a value".to_string())?;
            if value.is_empty() {
                return Err("--path cannot be empty".to_string());
            }
            paths.push(value.clone());
        } else if let Some(value) = arg.to_string_lossy().strip_prefix("--path=") {
            if value.is_empty() {
                return Err("--path cannot be empty".to_string());
            }
            paths.push(OsString::from(value));
        } else if arg.to_string_lossy().starts_with('-') {
            return Err(format!("unknown option {}", arg.to_string_lossy()));
        } else if root.is_none() {
            root = Some(PathBuf::from(arg));
        } else {
            return Err(format!("unexpected argument {}", arg.to_string_lossy()));
        }
        index += 1;
    }

    if debounce_set && !watch {
        return Err("--debounce requires --watch".to_string());
    }
    Ok(DiffOptions {
        root: root.unwrap_or_else(|| PathBuf::from(".")),
        watch,
        debounce,
        paths,
    })
}

fn parse_duration(raw: &OsStr) -> Result<Duration, String> {
    let raw = raw.to_string_lossy();
    let split_at = raw
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(raw.len());
    if split_at == 0 {
        return Err(format!("invalid duration {raw}"));
    }
    let value: u64 = raw[..split_at]
        .parse()
        .map_err(|_| format!("invalid duration {raw}"))?;
    if value == 0 {
        return Err(format!("duration must be positive: {raw}"));
    }
    let unit = &raw[split_at..];
    match unit {
        "ms" => Ok(Duration::from_millis(value)),
        "" | "s" => Ok(Duration::from_secs(value)),
        "m" => value
            .checked_mul(60)
            .map(Duration::from_secs)
            .ok_or_else(|| format!("duration out of range {raw}")),
        "h" => value
            .checked_mul(60 * 60)
            .map(Duration::from_secs)
            .ok_or_else(|| format!("duration out of range {raw}")),
        _ => Err(format!("invalid duration {raw}")),
    }
}

fn run_once(options: &DiffOptions) -> Result<(), String> {
    let snapshot = git_diff(options)?;
    let mut stdout = io::stdout().lock();
    stdout
        .write_all(&snapshot)
        .and_then(|()| stdout.flush())
        .map_err(|error| format!("writing stdout: {error}"))
}

fn git_diff(options: &DiffOptions) -> Result<Vec<u8>, String> {
    let output = StdCommand::new("git")
        .arg("-C")
        .arg(&options.root)
        .args([
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "HEAD",
            "--",
        ])
        .args(&options.paths)
        .output()
        .map_err(|error| format!("failed to run git: {error}"))?;
    let mut snapshot = git_output(output, "diff", false)?;

    let output = StdCommand::new("git")
        .arg("-C")
        .arg(&options.root)
        .args([
            "-c",
            "core.quotepath=false",
            "ls-files",
            "--others",
            "--exclude-standard",
            "-z",
            "--",
        ])
        .args(&options.paths)
        .output()
        .map_err(|error| format!("failed to run git: {error}"))?;
    let untracked = git_output(output, "ls-files", false)?;
    for path in parse_untracked_paths(&untracked)? {
        let output = StdCommand::new("git")
            .arg("-C")
            .arg(&options.root)
            .args([
                "diff",
                "--no-index",
                "--no-ext-diff",
                "--no-textconv",
                "--no-color",
                "--",
                "/dev/null",
            ])
            .arg(path)
            .output()
            .map_err(|error| format!("failed to run git: {error}"))?;
        snapshot.extend(git_output(output, "diff --no-index", true)?);
    }
    Ok(snapshot)
}

fn git_output(
    output: Output,
    operation: &str,
    differences_are_success: bool,
) -> Result<Vec<u8>, String> {
    if output.status.success() || (differences_are_success && output.status.code() == Some(1)) {
        return Ok(output.stdout);
    }
    let message = String::from_utf8_lossy(&output.stderr);
    let message = message.trim();
    if message.is_empty() {
        Err(format!("git {operation} failed with {}", output.status))
    } else {
        Err(message.to_string())
    }
}

fn parse_untracked_paths(output: &[u8]) -> Result<Vec<OsString>, String> {
    output
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(os_string_from_git_path)
        .collect()
}

#[cfg(unix)]
fn os_string_from_git_path(path: &[u8]) -> Result<OsString, String> {
    use std::os::unix::ffi::OsStringExt;
    Ok(OsString::from_vec(path.to_vec()))
}

#[cfg(not(unix))]
fn os_string_from_git_path(path: &[u8]) -> Result<OsString, String> {
    String::from_utf8(path.to_vec())
        .map(OsString::from)
        .map_err(|error| format!("git returned a non-UTF-8 path: {error}"))
}

async fn git_diff_async(options: &DiffOptions) -> Result<Vec<u8>, String> {
    let mut command = tokio::process::Command::new("git");
    command
        .arg("-C")
        .arg(&options.root)
        .args([
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "HEAD",
            "--",
        ])
        .args(&options.paths)
        .kill_on_drop(true);
    let output = command
        .output()
        .await
        .map_err(|error| format!("failed to run git: {error}"))?;
    let mut snapshot = git_output(output, "diff", false)?;

    let mut command = tokio::process::Command::new("git");
    command
        .arg("-C")
        .arg(&options.root)
        .args([
            "-c",
            "core.quotepath=false",
            "ls-files",
            "--others",
            "--exclude-standard",
            "-z",
            "--",
        ])
        .args(&options.paths)
        .kill_on_drop(true);
    let output = command
        .output()
        .await
        .map_err(|error| format!("failed to run git: {error}"))?;
    let untracked = git_output(output, "ls-files", false)?;
    for path in parse_untracked_paths(&untracked)? {
        let mut command = tokio::process::Command::new("git");
        command
            .arg("-C")
            .arg(&options.root)
            .args([
                "diff",
                "--no-index",
                "--no-ext-diff",
                "--no-textconv",
                "--no-color",
                "--",
                "/dev/null",
            ])
            .arg(path)
            .kill_on_drop(true);
        let output = command
            .output()
            .await
            .map_err(|error| format!("failed to run git: {error}"))?;
        snapshot.extend(git_output(output, "diff --no-index", true)?);
    }
    Ok(snapshot)
}

fn run_watch(options: &DiffOptions) -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("starting watch runtime: {error}"))?;
    runtime.block_on(watch_loop(options))
}

async fn watch_loop(options: &DiffOptions) -> Result<(), String> {
    let interrupt = tokio::signal::ctrl_c();
    tokio::pin!(interrupt);
    let mut last_emitted = tokio::select! {
        result = &mut interrupt => return interrupt_result(result),
        result = git_diff_async(options) => result?,
    };
    let mut event = 1;
    emit_event(event, &last_emitted)?;
    let mut pending: Option<(Vec<u8>, tokio::time::Instant)> = None;

    loop {
        tokio::select! {
            result = &mut interrupt => {
                return interrupt_result(result);
            }
            () = tokio::time::sleep(POLL_INTERVAL) => {}
        }

        let snapshot = tokio::select! {
            result = &mut interrupt => return interrupt_result(result),
            result = git_diff_async(options) => result?,
        };
        if snapshot == last_emitted {
            pending = None;
            continue;
        }

        let now = tokio::time::Instant::now();
        match &mut pending {
            Some((pending_snapshot, stable_since)) if *pending_snapshot == snapshot => {
                if now.duration_since(*stable_since) >= options.debounce {
                    event += 1;
                    emit_event(event, &snapshot)?;
                    last_emitted = snapshot;
                    pending = None;
                }
            }
            Some((pending_snapshot, stable_since)) => {
                *pending_snapshot = snapshot;
                *stable_since = now;
            }
            None => pending = Some((snapshot, now)),
        }
    }
}

fn interrupt_result(result: io::Result<()>) -> Result<(), String> {
    result.map_err(|error| format!("listening for Ctrl-C: {error}"))
}

fn emit_event(event: u64, snapshot: &[u8]) -> Result<(), String> {
    let state = if snapshot.is_empty() {
        "clean"
    } else {
        "dirty"
    };
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "@@ ctx-diff event={event} state={state}")
        .and_then(|()| stdout.write_all(snapshot))
        .and_then(|()| writeln!(stdout, "@@ ctx-diff end event={event}"))
        .and_then(|()| stdout.flush())
        .map_err(|error| format!("writing stdout: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_durations() {
        assert_eq!(
            parse_duration(OsStr::new("25ms")).unwrap(),
            Duration::from_millis(25)
        );
        assert_eq!(
            parse_duration(OsStr::new("2s")).unwrap(),
            Duration::from_secs(2)
        );
        assert_eq!(
            parse_duration(OsStr::new("3m")).unwrap(),
            Duration::from_secs(180)
        );
        assert!(parse_duration(OsStr::new("0ms")).is_err());
        assert!(parse_duration(OsStr::new("1.5s")).is_err());
    }
}
