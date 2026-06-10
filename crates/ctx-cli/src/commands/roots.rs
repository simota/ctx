use std::env;
use std::ffi::{OsStr, OsString};
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crate::common::*;
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub(crate) struct RootsArgs {
    command: RootsCommand,
}

#[derive(Debug)]
pub(crate) enum RootsCommand {
    Add {
        path: PathBuf,
        name: String,
    },
    List,
    Open {
        name_or_path: String,
        port: u16,
        bind: String,
        no_open: bool,
        audit: bool,
        timeout: Duration,
    },
    Remove {
        name_or_path: String,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct RootsFile {
    #[serde(default)]
    roots: Vec<RootEntry>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct RootEntry {
    name: String,
    path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    added_at: Option<toml::value::Datetime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_opened_at: Option<toml::value::Datetime>,
}

pub(crate) fn run_roots_command(args: &[OsString]) -> Option<ExitCode> {
    let parsed = parse_roots_args(args)?;
    match roots_command(parsed) {
        Ok(()) => Some(ExitCode::SUCCESS),
        Err(err) => {
            // Cobra (Go) prints the error twice for non-ExitError returns:
            // once as "Error: <msg>" and once as "<msg>" (via main's fmt.Fprintln).
            eprintln!("Error: {err}");
            eprintln!("{err}");
            Some(ExitCode::from(1))
        }
    }
}

pub(crate) fn parse_roots_args(args: &[OsString]) -> Option<RootsArgs> {
    let mut saw_roots = false;
    let mut subcommand: Option<String> = None;
    let mut name = String::new();
    let mut port: u16 = 0;
    let mut bind = "127.0.0.1".to_string();
    let mut no_open = false;
    let mut audit = false;
    let mut timeout = Duration::from_secs(10);
    let mut open_options_seen = false;
    let mut positionals = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == OsStr::new("roots") {
            if saw_roots {
                return None;
            }
            saw_roots = true;
        } else if let Some(value) = flag_value(arg, "--name") {
            name = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--name") {
            i += 1;
            name = args.get(i)?.to_string_lossy().into_owned();
        } else if let Some(value) = flag_value(arg, "--port") {
            open_options_seen = true;
            port = parse_roots_port(&value)?;
        } else if arg == OsStr::new("--port") {
            i += 1;
            open_options_seen = true;
            port = parse_roots_port(args.get(i)?)?;
        } else if let Some(value) = flag_value(arg, "--bind") {
            open_options_seen = true;
            bind = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--bind") {
            i += 1;
            open_options_seen = true;
            bind = args.get(i)?.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--no-open") {
            open_options_seen = true;
            no_open = true;
        } else if arg == OsStr::new("--audit") {
            open_options_seen = true;
            audit = true;
        } else if let Some(value) = flag_value(arg, "--timeout") {
            open_options_seen = true;
            timeout = parse_duration_arg(&value.to_string_lossy())?;
        } else if arg == OsStr::new("--timeout") {
            i += 1;
            open_options_seen = true;
            timeout = parse_duration_arg(&args.get(i)?.to_string_lossy())?;
        } else if is_option(arg) {
            return None;
        } else if saw_roots && subcommand.is_none() {
            subcommand = Some(arg.to_string_lossy().into_owned());
        } else if saw_roots {
            positionals.push(arg.clone());
        } else {
            return None;
        }
        i += 1;
    }
    let command = match subcommand.as_deref()? {
        "add" | "register" => {
            if positionals.len() > 1 || open_options_seen {
                return None;
            }
            RootsCommand::Add {
                path: positionals
                    .first()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from(".")),
                name,
            }
        }
        "list" | "ls" => {
            if !positionals.is_empty() || !name.is_empty() || open_options_seen {
                return None;
            }
            RootsCommand::List
        }
        "remove" | "rm" => match positionals.as_slice() {
            [target] if name.is_empty() && !open_options_seen => RootsCommand::Remove {
                name_or_path: target.to_string_lossy().into_owned(),
            },
            _ => return None,
        },
        "open" | "o" => match positionals.as_slice() {
            [target] if name.is_empty() => RootsCommand::Open {
                name_or_path: target.to_string_lossy().into_owned(),
                port,
                bind,
                no_open,
                audit,
                timeout,
            },
            _ => return None,
        },
        _ => return None,
    };
    Some(RootsArgs { command })
}

pub(crate) fn roots_command(args: RootsArgs) -> Result<(), String> {
    let path = roots_path()?;
    let mut registry = load_roots(&path)?;
    match args.command {
        RootsCommand::Add {
            path: raw_path,
            name,
        } => {
            let (added, entry) = roots_add(&mut registry, &raw_path, &name)?;
            save_roots(&path, &registry)?;
            if added {
                println!("ctx roots: registered {} -> {}", entry.name, entry.path);
            } else {
                println!(
                    "ctx roots: already registered {} -> {}",
                    entry.name, entry.path
                );
            }
        }
        RootsCommand::List => {
            let mut entries = registry.roots.clone();
            entries.sort_by(|a, b| {
                a.name
                    .to_ascii_lowercase()
                    .cmp(&b.name.to_ascii_lowercase())
            });
            if entries.is_empty() {
                println!("(no roots registered; run 'ctx roots add' or 'ctx browse <path>')");
            } else {
                // Emulate Go's tabwriter.NewWriter(w, 0, 0, 2, ' ', 0):
                // collect rows, compute per-column widths, print space-aligned.
                let mut rows: Vec<[String; 3]> = Vec::new();
                rows.push([
                    "NAME".to_string(),
                    "PATH".to_string(),
                    "LAST OPENED".to_string(),
                ]);
                for entry in entries {
                    let last = entry
                        .last_opened_at
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| "-".to_string());
                    rows.push([entry.name, entry.path, last]);
                }
                let col0_w = rows.iter().map(|r| r[0].len()).max().unwrap_or(0);
                let col1_w = rows.iter().map(|r| r[1].len()).max().unwrap_or(0);
                for row in &rows {
                    // Go tabwriter padding=2: each cell padded to col_width + 2 spaces
                    // except the last column which is not padded.
                    println!(
                        "{:<width0$}{:<width1$}{}",
                        row[0],
                        row[1],
                        row[2],
                        width0 = col0_w + 2,
                        width1 = col1_w + 2,
                    );
                }
            }
        }
        RootsCommand::Remove { name_or_path } => {
            let removed = roots_remove(&mut registry, &name_or_path)?;
            if !removed {
                return Err(format!("ctx roots: no entry matches {name_or_path:?}"));
            }
            save_roots(&path, &registry)?;
            println!("ctx roots: removed {name_or_path}");
        }
        RootsCommand::Open {
            name_or_path,
            port,
            bind,
            no_open,
            audit,
            timeout,
        } => {
            let entry = roots_find(&registry, &name_or_path)?.ok_or_else(|| {
                format!("ctx roots open: no registered root matches {name_or_path:?}")
            })?;
            let self_path = env::current_exe()
                .map_err(|err| format!("ctx roots open: locate self binary: {err}"))?;
            let child_args = browse_child_args(&entry.path, port, &bind, audit);
            let (url, child_pid) = spawn_browse_child(&self_path, &child_args, timeout)?;
            roots_mark_opened(&mut registry, &entry.name);
            if let Err(err) = save_roots(&path, &registry) {
                eprintln!("warning: roots: save: {err}");
            }
            println!(
                "ctx roots: opened {} at {} (pid {})",
                entry.name, url, child_pid
            );
            if !no_open {
                if let Err(err) = open_url(&url) {
                    eprintln!("warning: could not launch browser: {err}");
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn roots_path() -> Result<PathBuf, String> {
    if let Ok(path) = env::var("CTX_ROOTS_FILE") {
        if !path.trim().is_empty() {
            return Ok(expand_home(&path));
        }
    }
    let home = env::var("HOME").map_err(|err| format!("roots: locate home dir: {err}"))?;
    Ok(PathBuf::from(home).join(".ctx").join("roots.toml"))
}

pub(crate) fn load_roots(path: &Path) -> Result<RootsFile, String> {
    if !path.exists() {
        return Ok(RootsFile::default());
    }
    let raw = std::fs::read_to_string(path)
        .map_err(|err| format!("roots: read {}: {err}", path.display()))?;
    toml::from_str(&raw).map_err(|err| format!("roots: decode {}: {err}", path.display()))
}

pub(crate) fn save_roots(path: &Path, registry: &RootsFile) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("roots: mkdir {}: {err}", parent.display()))?;
    }
    let raw = toml::to_string(registry).map_err(|err| format!("roots: encode: {err}"))?;
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, raw).map_err(|err| format!("roots: write {}: {err}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|err| format!("roots: rename {}: {err}", path.display()))
}

pub(crate) fn roots_add(
    registry: &mut RootsFile,
    raw_path: &Path,
    raw_name: &str,
) -> Result<(bool, RootEntry), String> {
    let canon = canonical_root(raw_path)?;
    let name = if raw_name.is_empty() {
        Path::new(&canon)
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or("root")
            .to_string()
    } else {
        raw_name.to_string()
    };

    if let Some(existing_idx) = registry
        .roots
        .iter()
        .position(|entry| canonical_root(Path::new(&entry.path)).is_ok_and(|p| p == canon))
    {
        if registry
            .roots
            .iter()
            .enumerate()
            .any(|(idx, entry)| idx != existing_idx && entry.name == name)
        {
            let other = registry
                .roots
                .iter()
                .find(|entry| entry.name == name)
                .unwrap();
            return Err(format!(
                "roots: name {name:?} already used by {}",
                other.path
            ));
        }
        registry.roots[existing_idx].name = name;
        registry.roots[existing_idx].path = canon;
        return Ok((false, registry.roots[existing_idx].clone()));
    }
    if let Some(other) = registry.roots.iter().find(|entry| entry.name == name) {
        return Err(format!(
            "roots: name {name:?} already used by {}",
            other.path
        ));
    }
    let entry = RootEntry {
        name,
        path: canon,
        added_at: rfc3339_now_utc().parse().ok(),
        last_opened_at: None,
    };
    registry.roots.push(entry.clone());
    Ok((true, entry))
}

pub(crate) fn roots_remove(registry: &mut RootsFile, target: &str) -> Result<bool, String> {
    if let Some(idx) = registry.roots.iter().position(|entry| entry.name == target) {
        registry.roots.remove(idx);
        return Ok(true);
    }
    let canon = canonical_root(Path::new(target))?;
    if let Some(idx) = registry
        .roots
        .iter()
        .position(|entry| canonical_root(Path::new(&entry.path)).is_ok_and(|p| p == canon))
    {
        registry.roots.remove(idx);
        return Ok(true);
    }
    Ok(false)
}

pub(crate) fn roots_find(registry: &RootsFile, target: &str) -> Result<Option<RootEntry>, String> {
    if let Some(entry) = registry.roots.iter().find(|entry| entry.name == target) {
        return Ok(Some(entry.clone()));
    }
    let canon = canonical_root(Path::new(target))?;
    Ok(registry
        .roots
        .iter()
        .find(|entry| canonical_root(Path::new(&entry.path)).is_ok_and(|path| path == canon))
        .cloned())
}

pub(crate) fn roots_mark_opened(registry: &mut RootsFile, name: &str) {
    let now = rfc3339_now_utc().parse().ok();
    if let Some(entry) = registry.roots.iter_mut().find(|entry| entry.name == name) {
        entry.last_opened_at = now;
    }
}

pub(crate) fn parse_roots_port(raw: &OsStr) -> Option<u16> {
    raw.to_string_lossy().parse().ok()
}

pub(crate) fn parse_duration_arg(raw: &str) -> Option<Duration> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let split_at = raw
        .find(|ch: char| !ch.is_ascii_digit())
        .unwrap_or(raw.len());
    if split_at == 0 {
        return None;
    }
    let value: u64 = raw[..split_at].parse().ok()?;
    let unit = &raw[split_at..];
    match unit {
        "" | "s" => Some(Duration::from_secs(value)),
        "ms" => Some(Duration::from_millis(value)),
        "m" => value.checked_mul(60).map(Duration::from_secs),
        "h" => value.checked_mul(60 * 60).map(Duration::from_secs),
        _ => None,
    }
}

pub(crate) fn browse_child_args(path: &str, port: u16, bind: &str, audit: bool) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("browse"),
        OsString::from(path),
        OsString::from("--port"),
        OsString::from(port.to_string()),
        OsString::from("--no-open"),
        OsString::from("--bind"),
        OsString::from(bind),
        OsString::from("--no-register"),
    ];
    if audit {
        args.push(OsString::from("--audit"));
    }
    args
}

pub(crate) fn spawn_browse_child(
    self_path: &Path,
    args: &[OsString],
    timeout: Duration,
) -> Result<(String, u32), String> {
    let mut child = Command::new(self_path)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|err| format!("start ctx browse: {err}"))?;
    let pid = child.id();
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "stdout pipe unavailable".to_string())?;
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    let _ = tx.send(Err("ctx browse exited before emitting URL".to_string()));
                    return;
                }
                Ok(_) => {
                    if let Some(url) = extract_browse_url(&line) {
                        let _ = tx.send(Ok(url));
                        let mut sink = io::sink();
                        let _ = io::copy(&mut reader, &mut sink);
                        return;
                    }
                }
                Err(err) => {
                    let _ = tx.send(Err(format!("read ctx browse stdout: {err}")));
                    return;
                }
            }
        }
    });

    match rx.recv_timeout(timeout) {
        Ok(Ok(url)) => Ok((url, pid)),
        Ok(Err(err)) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(err)
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(format!(
                "timed out waiting for ctx browse URL after {}",
                display_duration(timeout)
            ))
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            let _ = child.kill();
            let _ = child.wait();
            Err("ctx browse URL reader stopped".to_string())
        }
    }
}

pub(crate) fn extract_browse_url(line: &str) -> Option<String> {
    for marker in ["http://", "https://"] {
        if let Some(start) = line.find(marker) {
            let rest = &line[start..];
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            let url = rest[..end].trim_end_matches(['.', ',', ';', ':']);
            if !url.is_empty() {
                return Some(url.to_string());
            }
        }
    }
    None
}

pub(crate) fn display_duration(duration: Duration) -> String {
    if duration.as_millis() % 1000 == 0 {
        format!("{}s", duration.as_secs())
    } else {
        format!("{}ms", duration.as_millis())
    }
}

pub(crate) fn open_url(url: &str) -> Result<(), String> {
    let (program, args): (&str, Vec<&str>) = if cfg!(target_os = "macos") {
        ("open", vec![url])
    } else if cfg!(target_os = "windows") {
        ("cmd", vec!["/C", "start", "", url])
    } else {
        ("xdg-open", vec![url])
    };
    Command::new(program)
        .args(args)
        .status()
        .map_err(|err| err.to_string())
        .and_then(|status| {
            if status.success() {
                Ok(())
            } else {
                Err(status.to_string())
            }
        })
}

pub(crate) fn canonical_root(path: &Path) -> Result<String, String> {
    if path.as_os_str().is_empty() {
        return Err("roots: empty path".to_string());
    }
    let expanded = expand_home(&path.to_string_lossy());
    let abs = if expanded.is_absolute() {
        expanded
    } else {
        env::current_dir()
            .map_err(|err| err.to_string())?
            .join(expanded)
    };
    let resolved = std::fs::canonicalize(&abs).unwrap_or(abs);
    Ok(resolved.to_string_lossy().into_owned())
}
