use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::protocol::*;
use crate::types::*;

pub(crate) fn digest_since_days(since: &str) -> Result<i64, RpcError> {
    let lower = since.to_ascii_lowercase();
    for (suffix, multiplier) in [("mo", 30), ("w", 7), ("d", 1), ("y", 365), ("h", 1)] {
        if let Some(num) = lower.strip_suffix(suffix) {
            if num.is_empty() || !num.chars().all(|ch| ch.is_ascii_digit()) {
                return Err(tool_error(format!(
                    "parsing duration {since:?}: invalid numeric part"
                )));
            }
            let value = num.parse::<i64>().map_err(|_| {
                tool_error(format!("parsing duration {since:?}: invalid numeric part"))
            })?;
            return Ok(if suffix == "h" {
                (value + 23) / 24
            } else {
                value * multiplier
            });
        }
    }
    Ok(0)
}

pub(crate) fn value_or_dash(input: &str) -> &str {
    if input.is_empty() {
        "-"
    } else {
        input
    }
}

pub(crate) fn format_count(n: i64) -> String {
    if n < 1000 {
        n.to_string()
    } else if n < 1_000_000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    }
}

pub(crate) fn generated_timestamp() -> &'static str {
    "1970-01-01T00:00:00Z"
}

pub(crate) fn detect_skim_lang(path: &Path, lang: &str) -> String {
    if !lang.is_empty() && lang != "auto" {
        return lang.to_string();
    }
    match path
        .extension()
        .and_then(OsStr::to_str)
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "go" => "go",
        "ts" => "ts",
        "tsx" => "tsx",
        "js" => "js",
        "jsx" => "jsx",
        "mjs" => "mjs",
        "py" => "python",
        "rb" => "ruby",
        "rs" => "rust",
        "java" => "java",
        "kt" => "kotlin",
        "swift" => "swift",
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" => "cpp",
        "md" => "markdown",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "sh" | "bash" => "shell",
        _ => "unknown",
    }
    .to_string()
}

pub(crate) fn absolute(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(path)
}

pub(crate) fn roots_path() -> Result<PathBuf, String> {
    if let Ok(path) = std::env::var("CTX_ROOTS_FILE") {
        return Ok(expand_home(&path));
    }
    let home = std::env::var("HOME").map_err(|err| format!("roots: locate home dir: {err}"))?;
    Ok(PathBuf::from(home).join(".ctx").join("roots.toml"))
}

pub(crate) fn load_roots() -> Result<RootsFile, String> {
    let path = roots_path()?;
    if !path.exists() {
        return Ok(RootsFile { roots: Vec::new() });
    }
    let raw = std::fs::read_to_string(&path)
        .map_err(|err| format!("roots: read {}: {err}", path.display()))?;
    toml::from_str(&raw).map_err(|err| format!("roots: decode {}: {err}", path.display()))
}

pub(crate) fn expand_home(path: &str) -> PathBuf {
    if path == "~" {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home);
        }
    }
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(path)
}

pub(crate) fn canonicalize_for_compare(path: &Path) -> Option<PathBuf> {
    if path.as_os_str().is_empty() {
        return None;
    }
    let abs = absolute(path);
    Some(std::fs::canonicalize(&abs).unwrap_or(abs))
}
