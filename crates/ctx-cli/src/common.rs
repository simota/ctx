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

/// Canonical form of a walk root, used as the containment boundary for
/// [`walk_entry_kind`]. Falls back to the path itself when it cannot be
/// resolved (the walk will then fail on `read_dir` anyway).
pub(crate) fn canonical_walk_root(root: &Path) -> PathBuf {
    std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf())
}

/// What a directory walker should do with one `read_dir` entry.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum WalkEntryKind {
    /// A real directory (never a symlink to one, so link cycles cannot recurse).
    Dir,
    /// A regular file, or a symlink that resolves to a regular file inside the
    /// root.
    File,
    /// Anything else: a symlink that dangles, leaves the root, or points at a
    /// directory, and non-regular files (FIFOs, sockets, devices), whose read
    /// could block the walk forever.
    Skip,
}

/// Classify a walker entry without following symlinks out of `canonical_root`
/// (see [`canonical_walk_root`]) and without reading through special files.
pub(crate) fn walk_entry_kind(
    canonical_root: &Path,
    path: &Path,
    file_type: std::fs::FileType,
) -> WalkEntryKind {
    if file_type.is_dir() {
        return WalkEntryKind::Dir;
    }
    if file_type.is_file() {
        return WalkEntryKind::File;
    }
    if file_type.is_symlink() && resolves_to_file_in(canonical_root, path) {
        return WalkEntryKind::File;
    }
    WalkEntryKind::Skip
}

/// Whether `path`, after resolving every symlink, is a regular file inside
/// `canonical_root`. Use it before reading a repository path that did not
/// come from [`walk_entry_kind`] (e.g. one taken from git history).
pub(crate) fn resolves_to_file_in(canonical_root: &Path, path: &Path) -> bool {
    std::fs::canonicalize(path).is_ok_and(|target| {
        target.starts_with(canonical_root)
            && std::fs::metadata(&target).is_ok_and(|meta| meta.is_file())
    })
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::path::{Path, PathBuf};
    use std::sync::mpsc;
    use std::time::Duration;

    /// Temp dir that cleans itself up on drop.
    pub(crate) struct TempDir(pub(crate) PathBuf);

    impl TempDir {
        pub(crate) fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("ctx-cli-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        pub(crate) fn write(&self, rel: &str, body: &str) {
            let path = self.0.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, body).unwrap();
        }

        #[cfg(unix)]
        pub(crate) fn symlink(&self, target: impl AsRef<Path>, rel: &str) {
            std::os::unix::fs::symlink(target, self.0.join(rel)).unwrap();
        }

        #[cfg(unix)]
        pub(crate) fn mkfifo(&self, rel: &str) {
            let status = std::process::Command::new("mkfifo")
                .arg(self.0.join(rel))
                .status()
                .expect("run mkfifo");
            assert!(status.success(), "mkfifo failed");
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Run `f` on a helper thread and fail the test if it does not finish in
    /// time, so a walk that blocks (e.g. on a FIFO) fails instead of hanging.
    pub(crate) fn finishes_within<T: Send + 'static>(
        secs: u64,
        f: impl FnOnce() -> T + Send + 'static,
    ) -> T {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
        rx.recv_timeout(Duration::from_secs(secs))
            .expect("walk did not finish (blocked on a special file or a symlink cycle?)")
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::test_support::TempDir;
    use super::*;

    fn kind(root: &Path, rel: &str) -> WalkEntryKind {
        let path = root.join(rel);
        let file_type = std::fs::symlink_metadata(&path).unwrap().file_type();
        walk_entry_kind(&canonical_walk_root(root), &path, file_type)
    }

    #[test]
    fn walk_entry_kind_bounds_symlinks_by_root_and_skips_special_files() {
        let outside = TempDir::new("walk-kind-outside");
        outside.write("secret.txt", "outside\n");
        let fx = TempDir::new("walk-kind");
        fx.write("src/main.rs", "fn main() {}\n");
        fx.symlink("main.rs", "src/alias.rs");
        fx.symlink("../src/main.rs", "src/up.rs");
        fx.symlink(outside.0.join("secret.txt"), "leak.rs");
        fx.symlink("missing.rs", "dangling.rs");
        fx.symlink(".", "loop");
        fx.mkfifo("pipe");

        assert_eq!(kind(&fx.0, "src"), WalkEntryKind::Dir);
        assert_eq!(kind(&fx.0, "src/main.rs"), WalkEntryKind::File);
        assert_eq!(kind(&fx.0, "src/alias.rs"), WalkEntryKind::File);
        assert_eq!(kind(&fx.0, "src/up.rs"), WalkEntryKind::File);
        assert_eq!(kind(&fx.0, "leak.rs"), WalkEntryKind::Skip);
        assert_eq!(kind(&fx.0, "dangling.rs"), WalkEntryKind::Skip);
        assert_eq!(kind(&fx.0, "loop"), WalkEntryKind::Skip);
        assert_eq!(kind(&fx.0, "pipe"), WalkEntryKind::Skip);
    }
}
