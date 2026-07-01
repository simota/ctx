use std::env;
use std::ffi::{OsStr, OsString};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use serde::Deserialize;
use sha2::{Digest, Sha256};

pub(crate) fn print_cobra_empty_error() {
    eprintln!("Error: ");
}

pub(crate) fn expand_home(path: &str) -> PathBuf {
    if path == "~" {
        env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(path))
    } else if let Some(rest) = path.strip_prefix("~/") {
        env::var("HOME")
            .map(|home| PathBuf::from(home).join(rest))
            .unwrap_or_else(|_| PathBuf::from(path))
    } else {
        PathBuf::from(path)
    }
}

pub(crate) fn git_output(args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .map_err(|err| err.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
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

pub(crate) fn current_date_utc() -> String {
    // Implementation moved to ctx_pack::assemble alongside the pack renderer.
    ctx_pack::assemble::current_date_utc()
}

pub(crate) fn format_k_tokens(n: i64) -> String {
    if n >= 1000 {
        format!("{}k", n / 1000)
    } else {
        n.to_string()
    }
}

pub(crate) fn format_number_i64(n: i64) -> String {
    let s = n.to_string();
    if s.len() <= 3 {
        return s;
    }
    let mut out = String::new();
    let mut first = s.len() % 3;
    if first == 0 {
        first = 3;
    }
    out.push_str(&s[..first]);
    let mut idx = first;
    while idx < s.len() {
        out.push(',');
        out.push_str(&s[idx..idx + 3]);
        idx += 3;
    }
    out
}

pub(crate) fn flag_value(arg: &OsStr, name: &str) -> Option<OsString> {
    let text = arg.to_string_lossy();
    let prefix = format!("{name}=");
    if text.starts_with(&prefix) {
        Some(OsString::from(&text[prefix.len()..]))
    } else {
        None
    }
}

/// Format an OS file-read error in Go style: `open <path>: <lowercase message>`.
///
/// Go's os.ReadFile uses PathError{Op:"open", Path:name, Err:syscall.ENOENT}, which
/// formats as `open <path>: no such file or directory`. Rust's io::Error formats
/// differently (`No such file or directory (os error 2)`), so we normalise here.
pub(crate) fn go_style_read_error(path: &OsStr, err: &io::Error) -> String {
    let desc = match err.kind() {
        io::ErrorKind::NotFound => "no such file or directory".to_string(),
        io::ErrorKind::PermissionDenied => "permission denied".to_string(),
        _ => {
            // For other errors keep the raw OS error but lowercase it.
            let s = err.to_string();
            let s = if let Some(idx) = s.find(" (os error ") {
                &s[..idx]
            } else {
                &s
            };
            s.to_lowercase()
        }
    };
    format!("open {}: {}", path.to_string_lossy(), desc)
}

pub(crate) fn read_maybe_stdin(path: &OsStr) -> Result<Vec<u8>, String> {
    if path == OsStr::new("-") {
        let mut body = Vec::new();
        io::stdin()
            .read_to_end(&mut body)
            .map_err(|err| err.to_string())?;
        return Ok(body);
    }
    std::fs::read(Path::new(path)).map_err(|err| go_style_read_error(path, &err))
}

pub(crate) fn read_response(path: Option<&OsStr>) -> Result<Vec<u8>, String> {
    match path {
        Some(path) if path != OsStr::new("-") => {
            std::fs::read(Path::new(path)).map_err(|err| go_style_read_error(path, &err))
        }
        _ => {
            let mut body = Vec::new();
            io::stdin()
                .read_to_end(&mut body)
                .map_err(|err| err.to_string())?;
            Ok(body)
        }
    }
}

pub(crate) fn sha256_hex(raw: &str) -> String {
    format!("{:x}", Sha256::digest(raw.as_bytes()))
}

pub(crate) fn sha256_file_hex(path: &Path) -> Result<String, String> {
    let data = std::fs::read(path).map_err(|err| format!("hash {}: {err}", path.display()))?;
    Ok(format!("{:x}", Sha256::digest(&data)))
}

pub(crate) fn default_audit_log_path() -> Option<PathBuf> {
    if env::var_os("CTX_AUDIT_DISABLE").as_deref() == Some(OsStr::new("1")) {
        return None;
    }
    env::var_os("CTX_AUDIT_LOG")
        .filter(|path| !path.is_empty())
        .map(expand_path)
        .or_else(|| Some(expand_path(OsString::from("~/.ctx/audit.log"))))
}

pub(crate) fn expand_path(path: OsString) -> PathBuf {
    let path_text = path.to_string_lossy();
    if path_text == "~" {
        if let Some(home) = env::var_os("HOME") {
            return PathBuf::from(home);
        }
    }
    if let Some(rest) = path_text.strip_prefix("~/") {
        if let Some(home) = env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(path)
}

pub(crate) fn is_option(arg: &OsStr) -> bool {
    arg.to_string_lossy().starts_with('-')
}

/// Render native clap-style help for a nested verify subcommand (e.g.
/// `audit verify`, `contract verify`) whose dispatch is intercepted before the
/// top-level clap parser. clap's help formatting differs from cobra's — that is
/// an accepted divergence after Go elimination (help is human-facing). Prints to
/// stdout and returns a success exit code, matching clap's `--help` convention.
pub(crate) fn render_subcommand_help(name: &str, about: &str, usage_args: &str) -> ExitCode {
    println!("{about}");
    println!();
    println!("Usage: {name} {usage_args}");
    ExitCode::SUCCESS
}

#[derive(Debug, Deserialize)]
pub(crate) struct SecurityToml {
    #[serde(default)]
    pub(crate) strict_offline: bool,
    #[serde(default = "default_true")]
    pub(crate) secret_scan: bool,
    #[serde(default = "default_true")]
    pub(crate) redact: bool,
    #[serde(default)]
    pub(crate) allowlist: Vec<String>,
    #[serde(default)]
    pub(crate) allowlist_files: Vec<String>,
    #[serde(default)]
    pub(crate) enable_entropy: bool,
}

impl Default for SecurityToml {
    fn default() -> Self {
        Self {
            strict_offline: false,
            secret_scan: true,
            redact: true,
            allowlist: Vec::new(),
            allowlist_files: Vec::new(),
            enable_entropy: false,
        }
    }
}

pub(crate) fn default_true() -> bool {
    true
}

pub(crate) fn rfc3339_now_utc() -> String {
    let output = Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output();
    match output {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        _ => "1970-01-01T00:00:00Z".to_string(),
    }
}
