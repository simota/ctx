//! Saved file-selection recipes ("mixes").
//!
//! A mix records *which files* to hand to an LLM, in which order, together
//! with the goal and token budget they were chosen for. It is the reusable
//! half of `ctx pack`: build a selection once (from `ctx where`, the
//! `ctx browse` pinned board, or by hand), then re-pack it at any later
//! commit with `ctx pack --from-mix <id>` or the MCP `ctx_pack` `mix`
//! argument.
//!
//! The on-disk format is the one the Go implementation wrote, one
//! `<id>.mix.json` per recipe (2-space indented JSON):
//!
//! ```json
//! {
//!   "schema_version": 1,
//!   "id": "auth-review",
//!   "name": "Auth review",
//!   "goal": "review session expiry",
//!   "created": "2026-01-01T10:00:00Z",
//!   "files": ["src/session.rs", "src/login.rs"],
//!   "budget": {"limit": 30000}
//! }
//! ```
//!
//! `updated` is an optional extension written when a recipe is edited;
//! readers that predate it ignore it.
//!
//! Store location (first match wins), shared by every front end:
//!  1. `$XDG_STATE_HOME/ctx/mixes/`
//!  2. `$HOME/.local/state/ctx/mixes/` (when `~/.local/state` exists)
//!  3. `$HOME/.ctx/mixes/`

use std::collections::hash_map::RandomState;
use std::collections::HashSet;
use std::fmt;
use std::hash::{BuildHasher, Hasher};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: i64 = 1;
pub const MAX_NAME_LEN: usize = 200;
pub const MAX_GOAL_LEN: usize = 2000;
pub const MAX_FILES: usize = 5000;
pub const MAX_PATH_LEN: usize = 4096;
pub const MAX_BUDGET: i64 = 10_000_000;
const MAX_SLUG_LEN: usize = 48;

/// Optional plan name + token limit.
/// Always serialized (even when empty) to keep the Go `"budget":{}` shape.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct Budget {
    #[serde(default, skip_serializing_if = "str::is_empty")]
    pub plan: String,
    #[serde(default, skip_serializing_if = "is_zero_i64")]
    pub limit: i64,
}

fn is_zero_i64(v: &i64) -> bool {
    *v == 0
}

/// One saved recipe. Field order matches the Go struct for byte-identical
/// JSON output.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Mix {
    pub schema_version: i64,
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "str::is_empty")]
    pub goal: String,
    /// RFC3339 timestamp, kept as an opaque string to round-trip faithfully.
    pub created: String,
    /// RFC3339 timestamp of the last edit; absent on recipes never edited.
    #[serde(default, skip_serializing_if = "str::is_empty")]
    pub updated: String,
    pub files: Vec<String>,
    #[serde(default)]
    pub budget: Budget,
}

/// What a caller supplies to create a recipe; the store assigns `id`
/// (unless `id` is set) and `created`.
#[derive(Debug, Clone, Default)]
pub struct Draft {
    /// Requested id. Empty derives one from `name`.
    pub id: String,
    pub name: String,
    pub goal: String,
    pub files: Vec<String>,
    pub budget: i64,
}

#[derive(Debug)]
pub enum MixError {
    InvalidId(String),
    Invalid(String),
    NotFound(String),
    Ambiguous(String, Vec<String>),
    Exists(String),
    Store(String),
    Io(io::Error),
    Json(serde_json::Error),
}

impl MixError {
    /// Stable machine-readable code (used by the web API error envelope).
    pub fn code(&self) -> &'static str {
        match self {
            MixError::InvalidId(_) => "invalid_id",
            MixError::Invalid(_) => "bad_request",
            MixError::NotFound(_) => "not_found",
            MixError::Ambiguous(..) => "ambiguous",
            MixError::Exists(_) => "conflict",
            MixError::Store(_) | MixError::Io(_) | MixError::Json(_) => "mix_store",
        }
    }
}

impl fmt::Display for MixError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MixError::InvalidId(s) => write!(f, "mix: invalid recipe id: {s}"),
            MixError::Invalid(s) => write!(f, "mix: {s}"),
            MixError::NotFound(id) => write!(f, "mix: recipe not found: {id}"),
            MixError::Ambiguous(name, ids) => write!(
                f,
                "mix: {name:?} names several recipes ({}); use the id",
                ids.join(", ")
            ),
            MixError::Exists(id) => write!(f, "mix: recipe already exists: {id}"),
            MixError::Store(s) => write!(f, "mix: {s}"),
            MixError::Io(e) => write!(f, "mix: io error: {e}"),
            MixError::Json(e) => write!(f, "mix: json: {e}"),
        }
    }
}

impl std::error::Error for MixError {}

impl From<io::Error> for MixError {
    fn from(e: io::Error) -> Self {
        MixError::Io(e)
    }
}

// ---------------------------------------------------------------------------
// Validation / normalisation
// ---------------------------------------------------------------------------

/// Ids are file names in the store: `[A-Za-z0-9._-]+`, not starting with `.`.
pub fn validate_id(id: &str) -> Result<(), MixError> {
    if id.is_empty() {
        return Err(MixError::InvalidId("empty id".to_string()));
    }
    for c in id.chars() {
        match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => {}
            _ => {
                return Err(MixError::InvalidId(format!(
                    "{id:?} contains disallowed character {c:?}"
                )));
            }
        }
    }
    if id.starts_with('.') || id.len() > 128 {
        return Err(MixError::InvalidId(format!("{id:?}")));
    }
    Ok(())
}

/// Normalise one repository-relative path: `\` → `/`, drop `.` and empty
/// components. Absolute paths and `..` components are rejected — a recipe
/// must never name a file outside the repository it is packed from.
pub fn normalize_path(raw: &str) -> Result<String, MixError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(MixError::Invalid("empty file path".to_string()));
    }
    if trimmed.len() > MAX_PATH_LEN {
        return Err(MixError::Invalid(format!(
            "file path longer than {MAX_PATH_LEN} bytes"
        )));
    }
    if trimmed.contains('\0') {
        return Err(MixError::Invalid(format!(
            "file path {trimmed:?} contains NUL"
        )));
    }
    let unified = trimmed.replace('\\', "/");
    let bytes = unified.as_bytes();
    let drive = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    if unified.starts_with('/') || drive {
        return Err(MixError::Invalid(format!(
            "file path {trimmed:?} must be relative to the repository root"
        )));
    }
    let mut parts = Vec::new();
    for part in unified.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                return Err(MixError::Invalid(format!(
                    "file path {trimmed:?} must not contain '..'"
                )))
            }
            other => parts.push(other),
        }
    }
    if parts.is_empty() {
        return Err(MixError::Invalid(format!(
            "file path {trimmed:?} names the repository root, not a file"
        )));
    }
    Ok(parts.join("/"))
}

/// Normalise every path and drop duplicates, keeping first-seen order (the
/// order is the pack order).
pub fn normalize_files<S: AsRef<str>>(files: &[S]) -> Result<Vec<String>, MixError> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for raw in files {
        let path = normalize_path(raw.as_ref())?;
        if seen.insert(path.clone()) {
            out.push(path);
        }
    }
    if out.len() > MAX_FILES {
        return Err(MixError::Invalid(format!(
            "a recipe holds at most {MAX_FILES} files"
        )));
    }
    Ok(out)
}

fn validate_fields(name: &str, goal: &str, budget: i64) -> Result<(), MixError> {
    if name.trim().is_empty() {
        return Err(MixError::Invalid("name is required".to_string()));
    }
    if name.len() > MAX_NAME_LEN {
        return Err(MixError::Invalid(format!(
            "name longer than {MAX_NAME_LEN} bytes"
        )));
    }
    if goal.len() > MAX_GOAL_LEN {
        return Err(MixError::Invalid(format!(
            "goal longer than {MAX_GOAL_LEN} bytes"
        )));
    }
    if !(0..=MAX_BUDGET).contains(&budget) {
        return Err(MixError::Invalid(format!(
            "budget must be between 0 and {MAX_BUDGET}"
        )));
    }
    Ok(())
}

/// A readable id derived from a display name: lowercase ASCII letters and
/// digits, every other run of characters collapsed to `-`. Empty when the
/// name has no ASCII alphanumerics (e.g. a Japanese-only name).
pub fn slug_from_name(name: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            if dash && !out.is_empty() {
                out.push('-');
            }
            dash = false;
            out.push(c.to_ascii_lowercase());
            if out.len() >= MAX_SLUG_LEN {
                break;
            }
        } else {
            dash = true;
        }
    }
    out
}

/// A random 16-hex-digit id (the Go store's id shape). Not a secret: it only
/// needs to be unlikely to collide.
pub fn generate_id() -> String {
    let mut hasher = RandomState::new().build_hasher();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    hasher.write_u128(nanos);
    hasher.write_u32(std::process::id());
    format!("{:016x}", hasher.finish())
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

/// Resolve the default store directory (see the module docs).
pub fn resolve_dir() -> Result<PathBuf, MixError> {
    if let Ok(xdg) = std::env::var("XDG_STATE_HOME") {
        let xdg = xdg.trim();
        if !xdg.is_empty() {
            return Ok(Path::new(xdg).join("ctx").join("mixes"));
        }
    }
    let home = std::env::var("HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| MixError::Store("cannot resolve store directory".to_string()))?;
    let state_dir = Path::new(&home).join(".local").join("state");
    if state_dir.is_dir() {
        return Ok(state_dir.join("ctx").join("mixes"));
    }
    Ok(Path::new(&home).join(".ctx").join("mixes"))
}

#[derive(Debug, Clone)]
pub struct Store {
    dir: PathBuf,
}

impl Store {
    /// The store at the default location (not created until the first save).
    pub fn open_default() -> Result<Store, MixError> {
        Ok(Store {
            dir: resolve_dir()?,
        })
    }

    pub fn at(dir: impl Into<PathBuf>) -> Store {
        Store { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn path_for(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.mix.json"))
    }

    /// True when a recipe file (or anything else, e.g. a dangling link) holds
    /// the name `id` would be saved under.
    pub fn exists(&self, id: &str) -> bool {
        std::fs::symlink_metadata(self.path_for(id)).is_ok()
    }

    /// Load by exact id. Only a regular file is read: a FIFO or a link to a
    /// file elsewhere planted in the store is reported as not found.
    pub fn load(&self, id: &str) -> Result<Mix, MixError> {
        validate_id(id)?;
        let path = self.path_for(id);
        match std::fs::symlink_metadata(&path) {
            Ok(meta) if meta.is_file() => {}
            Ok(_) => return Err(MixError::NotFound(id.to_string())),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err(MixError::NotFound(id.to_string()))
            }
            Err(e) => return Err(MixError::Io(e)),
        }
        let data = std::fs::read(&path)?;
        serde_json::from_slice(&data).map_err(MixError::Json)
    }

    /// Every readable recipe, newest first (RFC3339 sorts chronologically),
    /// ties broken by id. Unreadable entries are skipped.
    pub fn list(&self) -> Result<Vec<Mix>, MixError> {
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(MixError::Io(e)),
        };
        let mut out = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let Some(id) = name.strip_suffix(".mix.json") else {
                continue;
            };
            if let Ok(mix) = self.load(id) {
                out.push(mix);
            }
        }
        out.sort_by(|a, b| b.created.cmp(&a.created).then_with(|| a.id.cmp(&b.id)));
        Ok(out)
    }

    /// Resolve a user reference: an exact id, else a recipe whose name
    /// matches case-insensitively (an error when several do).
    pub fn find(&self, reference: &str) -> Result<Mix, MixError> {
        let reference = reference.trim();
        if validate_id(reference).is_ok() {
            match self.load(reference) {
                Ok(mix) => return Ok(mix),
                Err(MixError::NotFound(_)) => {}
                Err(e) => return Err(e),
            }
        }
        let mut matches: Vec<Mix> = self
            .list()?
            .into_iter()
            .filter(|mix| mix.name.trim().eq_ignore_ascii_case(reference))
            .collect();
        match matches.len() {
            0 => Err(MixError::NotFound(reference.to_string())),
            1 => Ok(matches.remove(0)),
            _ => Err(MixError::Ambiguous(
                reference.to_string(),
                matches.into_iter().map(|mix| mix.id).collect(),
            )),
        }
    }

    /// Create a recipe. An explicit `draft.id` must be free; a derived id
    /// (from the name, or random) gets a `-2`, `-3`, … suffix when taken.
    pub fn create(&self, draft: Draft) -> Result<Mix, MixError> {
        validate_fields(&draft.name, &draft.goal, draft.budget)?;
        let files = normalize_files(&draft.files)?;
        if files.is_empty() {
            return Err(MixError::Invalid(
                "a recipe needs at least one file".to_string(),
            ));
        }
        let id = if draft.id.is_empty() {
            self.free_id(&slug_from_name(&draft.name))
        } else {
            validate_id(&draft.id)?;
            if self.exists(&draft.id) {
                return Err(MixError::Exists(draft.id));
            }
            draft.id
        };
        let mix = Mix {
            schema_version: SCHEMA_VERSION,
            id,
            name: draft.name.trim().to_string(),
            goal: draft.goal.trim().to_string(),
            created: ctx_pack::assemble::current_rfc3339_utc(),
            updated: String::new(),
            files,
            budget: Budget {
                plan: String::new(),
                limit: draft.budget,
            },
        };
        self.write(&mix)?;
        Ok(mix)
    }

    fn free_id(&self, base: &str) -> String {
        if base.is_empty() {
            loop {
                let id = generate_id();
                if !self.exists(&id) {
                    return id;
                }
            }
        }
        if !self.exists(base) {
            return base.to_string();
        }
        (2..)
            .map(|n| format!("{base}-{n}"))
            .find(|id| !self.exists(id))
            .expect("an unused suffix exists")
    }

    /// Replace an existing recipe (validated, files normalised, `updated`
    /// stamped). The id must already exist.
    pub fn update(&self, mut mix: Mix) -> Result<Mix, MixError> {
        validate_id(&mix.id)?;
        validate_fields(&mix.name, &mix.goal, mix.budget.limit)?;
        mix.files = normalize_files(&mix.files)?;
        if mix.files.is_empty() {
            return Err(MixError::Invalid(
                "a recipe needs at least one file".to_string(),
            ));
        }
        // Must exist as a regular recipe file (load checks that).
        self.load(&mix.id)?;
        mix.schema_version = SCHEMA_VERSION;
        mix.updated = ctx_pack::assemble::current_rfc3339_utc();
        self.write(&mix)?;
        Ok(mix)
    }

    /// Delete by exact id.
    pub fn delete(&self, id: &str) -> Result<(), MixError> {
        validate_id(id)?;
        let path = self.path_for(id);
        match std::fs::symlink_metadata(&path) {
            Ok(meta) if meta.is_file() || meta.file_type().is_symlink() => {
                std::fs::remove_file(&path)?;
                Ok(())
            }
            Ok(_) => Err(MixError::NotFound(id.to_string())),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                Err(MixError::NotFound(id.to_string()))
            }
            Err(e) => Err(MixError::Io(e)),
        }
    }

    /// Write through a freshly created temporary file and rename it into
    /// place, so a reader never sees a half-written recipe and a planted
    /// `<id>.tmp` link cannot redirect the write.
    fn write(&self, mix: &Mix) -> Result<(), MixError> {
        std::fs::create_dir_all(&self.dir)?;
        let mut body = serde_json::to_vec_pretty(mix).map_err(MixError::Json)?;
        body.push(b'\n');
        let tmp = loop {
            let candidate = self.dir.join(format!(".{}.{}.tmp", mix.id, generate_id()));
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&candidate)
            {
                Ok(mut file) => {
                    if let Err(e) = file.write_all(&body).and_then(|_| file.sync_all()) {
                        let _ = std::fs::remove_file(&candidate);
                        return Err(MixError::Io(e));
                    }
                    break candidate;
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(MixError::Io(e)),
            }
        };
        if let Err(e) = std::fs::rename(&tmp, self.path_for(&mix.id)) {
            let _ = std::fs::remove_file(&tmp);
            return Err(MixError::Io(e));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_relative_paths_and_rejects_escapes() {
        assert_eq!(normalize_path("./src//main.rs").unwrap(), "src/main.rs");
        assert_eq!(normalize_path("src\\lib.rs").unwrap(), "src/lib.rs");
        assert!(normalize_path("/etc/passwd").is_err());
        assert!(normalize_path("C:/x").is_err());
        assert!(normalize_path("src/../../x").is_err());
        assert!(normalize_path(".").is_err());
        assert!(normalize_path("  ").is_err());
    }

    #[test]
    fn dedupes_files_keeping_first_order() {
        let files = normalize_files(&["b.rs", "./a.rs", "b.rs", "a.rs"]).unwrap();
        assert_eq!(files, vec!["b.rs", "a.rs"]);
    }

    #[test]
    fn slugs_names() {
        assert_eq!(slug_from_name("Auth  Review (v2)"), "auth-review-v2");
        assert_eq!(slug_from_name("認証まわり"), "");
        assert_eq!(slug_from_name("--x--"), "x");
    }

    #[test]
    fn ids_reject_traversal() {
        assert!(validate_id("../x").is_err());
        assert!(validate_id(".hidden").is_err());
        assert!(validate_id("a/b").is_err());
        assert!(validate_id("auth-review_2.1").is_ok());
    }

    #[test]
    fn generated_ids_are_hex() {
        let id = generate_id();
        assert_eq!(id.len(), 16);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
