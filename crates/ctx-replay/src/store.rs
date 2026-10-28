// crates/ctx-replay/src/store.rs
//
// Port of internal/replay/store.go — directory-backed manifest store.

use std::fs;
use std::path::{Path, PathBuf};

use crate::types::Manifest;

/// Error variants mirroring `replay.ErrSnapshot*`.
#[derive(Debug)]
pub enum StoreError {
    Exists(String),
    NotFound(String),
    InvalidId(String),
    Io(std::io::Error),
    Json(serde_json::Error),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Exists(id) => write!(f, "replay: snapshot already exists: {id}"),
            StoreError::NotFound(id) => write!(f, "replay: snapshot not found: {id}"),
            StoreError::InvalidId(s) => write!(f, "replay: invalid snapshot id: {s}"),
            StoreError::Io(e) => write!(f, "replay: io error: {e}"),
            StoreError::Json(e) => write!(f, "replay: json: {e}"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        StoreError::Io(e)
    }
}

impl From<serde_json::Error> for StoreError {
    fn from(e: serde_json::Error) -> Self {
        StoreError::Json(e)
    }
}

/// ResolveOptions mirrors `replay.ResolveOptions`.
#[derive(Debug, Clone, Default)]
pub struct ResolveOptions {
    pub shared: bool,
    pub root: String,
}

/// Resolve picks a store directory using the same precedence as Go's
/// `replay.Resolve`:
///  1. ResolveOptions.shared → <root>/.ctx/replay/
///  2. $XDG_STATE_HOME/ctx/replay/
///  3. $HOME/.local/state/ctx/replay/  (if .local/state exists)
///  4. $HOME/.ctx/replay/
pub fn resolve(opts: ResolveOptions) -> std::io::Result<String> {
    if opts.shared {
        let root = if opts.root.is_empty() {
            "."
        } else {
            &opts.root
        };
        let abs = fs::canonicalize(root).unwrap_or_else(|_| PathBuf::from(root));
        let p = abs.join(".ctx").join("replay");
        // The shared store lives in the repository, so its directories may
        // come from a clone. A committed `.ctx` or `.ctx/replay` symlink
        // would send every snapshot write (and read) wherever it points.
        for dir in [abs.join(".ctx"), p.clone()] {
            if fs::symlink_metadata(&dir).is_ok_and(|meta| meta.file_type().is_symlink()) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!(
                        "replay: refusing symlinked shared store directory {}",
                        dir.display()
                    ),
                ));
            }
        }
        return Ok(p.to_string_lossy().into_owned());
    }
    if let Ok(xdg) = std::env::var("XDG_STATE_HOME") {
        let xdg = xdg.trim();
        if !xdg.is_empty() {
            let p = Path::new(xdg).join("ctx").join("replay");
            return Ok(p.to_string_lossy().into_owned());
        }
    }
    let home = home_dir().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "replay: cannot resolve store directory",
        )
    })?;
    let state_dir = Path::new(&home).join(".local").join("state");
    if state_dir.is_dir() {
        let p = state_dir.join("ctx").join("replay");
        return Ok(p.to_string_lossy().into_owned());
    }
    let p = Path::new(&home).join(".ctx").join("replay");
    Ok(p.to_string_lossy().into_owned())
}

fn home_dir() -> Option<String> {
    std::env::var("HOME").ok()
}

/// Mirrors `replay.Store`.
pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn path(&self, id: &str) -> Result<PathBuf, StoreError> {
        validate_id(id)?;
        Ok(self.dir.join(format!("{id}.json")))
    }

    pub fn save(&self, m: &Manifest) -> Result<(), StoreError> {
        validate_id(&m.id)?;
        let path = self.dir.join(format!("{}.json", m.id));
        // `symlink_metadata` so a dangling `<id>.json` link also counts as
        // taken instead of being replaced.
        match fs::symlink_metadata(&path) {
            Ok(_) => return Err(StoreError::Exists(m.id.clone())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(StoreError::Io(e)),
        }
        let data = serde_json::to_vec_pretty(m)?;
        // A fixed `<id>.tmp` name could be a planted symlink that `fs::write`
        // follows, overwriting a file outside the store. Use a fresh name
        // and `create_new` (O_EXCL), which never follows or reuses an entry.
        let tmp = self.dir.join(format!(
            ".{}.{}.{}.tmp",
            m.id,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let written = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .and_then(|mut file| std::io::Write::write_all(&mut file, &data));
        if let Err(e) = written.and_then(|()| fs::rename(&tmp, &path)) {
            let _ = fs::remove_file(&tmp);
            return Err(StoreError::Io(e));
        }
        Ok(())
    }

    pub fn load(&self, id: &str) -> Result<Manifest, StoreError> {
        validate_id(id)?;
        let path = self.dir.join(format!("{id}.json"));
        // Only regular files are snapshots: reading a FIFO or a link to
        // `/dev/stdin` would block, and a link may point outside the store.
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_file() => {}
            Ok(_) => {
                return Err(StoreError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("{} is not a regular file", path.display()),
                )));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(StoreError::NotFound(id.to_string()));
            }
            Err(e) => return Err(StoreError::Io(e)),
        }
        let data = fs::read(&path)?;
        Ok(serde_json::from_slice(&data)?)
    }

    pub fn list(&self) -> Result<Vec<Manifest>, StoreError> {
        let entries = match fs::read_dir(&self.dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(StoreError::Io(e)),
        };
        let mut out: Vec<Manifest> = Vec::new();
        for entry in entries.flatten() {
            let ft = match entry.file_type() {
                Ok(t) => t,
                Err(_) => continue,
            };
            // `DirEntry::file_type` does not follow symlinks.
            if !ft.is_file() {
                continue;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.ends_with(".json") {
                continue;
            }
            let id = &name[..name.len() - 5];
            match self.load(id) {
                Ok(m) => out.push(m),
                Err(_) => continue,
            }
        }
        out.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(out)
    }

    pub fn delete(&self, id: &str) -> Result<(), StoreError> {
        validate_id(id)?;
        let path = self.dir.join(format!("{id}.json"));
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(StoreError::NotFound(id.to_string()))
            }
            Err(e) => Err(StoreError::Io(e)),
        }
    }
}

/// Mirrors `replay.OpenStore`.
pub fn open_store(dir: &str) -> Result<Store, StoreError> {
    if dir.is_empty() {
        return Err(StoreError::InvalidId("empty store directory".into()));
    }
    fs::create_dir_all(dir)?;
    Ok(Store {
        dir: PathBuf::from(dir),
    })
}

fn validate_id(id: &str) -> Result<(), StoreError> {
    if id.is_empty() {
        return Err(StoreError::InvalidId("empty id".into()));
    }
    for c in id.chars() {
        let ok = ('a'..='z').contains(&c)
            || ('A'..='Z').contains(&c)
            || ('0'..='9').contains(&c)
            || c == '-'
            || c == '_'
            || c == '.';
        if !ok {
            return Err(StoreError::InvalidId(format!(
                "{id:?} contains disallowed character {c:?}"
            )));
        }
    }
    if id == "." || id == ".." || id.starts_with('.') {
        return Err(StoreError::InvalidId(id.to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "ctx-replay-store-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn save_load_round_trip() {
        let dir = tmp_dir("rt");
        let store = open_store(&dir.to_string_lossy()).unwrap();
        let mut m = Manifest::default();
        m.id = "abc".into();
        m.created_at = "2026-01-01T00:00:00Z".into();
        store.save(&m).unwrap();
        let got = store.load("abc").unwrap();
        assert_eq!(got.id, "abc");
    }

    #[test]
    fn save_duplicate_returns_exists() {
        let dir = tmp_dir("dup");
        let store = open_store(&dir.to_string_lossy()).unwrap();
        let mut m = Manifest::default();
        m.id = "a".into();
        m.created_at = "2026-01-01T00:00:00Z".into();
        store.save(&m).unwrap();
        let err = store.save(&m).unwrap_err();
        assert!(matches!(err, StoreError::Exists(_)));
    }

    #[test]
    fn list_returns_chronological_order() {
        let dir = tmp_dir("list");
        let store = open_store(&dir.to_string_lossy()).unwrap();
        let mut a = Manifest::default();
        a.id = "a".into();
        a.created_at = "2026-01-02T00:00:00Z".into();
        let mut b = Manifest::default();
        b.id = "b".into();
        b.created_at = "2026-01-01T00:00:00Z".into();
        store.save(&a).unwrap();
        store.save(&b).unwrap();
        let got = store.list().unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].id, "b");
        assert_eq!(got[1].id, "a");
    }

    #[cfg(unix)]
    #[test]
    fn save_does_not_follow_a_planted_tmp_symlink() {
        let dir = tmp_dir("tmplink");
        let victim = dir.join("victim.txt");
        std::fs::write(&victim, "keep me").unwrap();
        let store_dir = dir.join("store");
        std::fs::create_dir_all(&store_dir).unwrap();
        std::os::unix::fs::symlink(&victim, store_dir.join("baseline.tmp")).unwrap();
        let store = open_store(&store_dir.to_string_lossy()).unwrap();
        let m = Manifest {
            id: "baseline".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            ..Default::default()
        };
        store.save(&m).unwrap();
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "keep me");
        let saved = store_dir.join("baseline.json");
        assert!(std::fs::symlink_metadata(&saved)
            .unwrap()
            .file_type()
            .is_file());
        assert_eq!(store.load("baseline").unwrap().id, "baseline");
    }

    #[cfg(unix)]
    #[test]
    fn save_treats_a_dangling_json_symlink_as_existing() {
        let dir = tmp_dir("danglink");
        let store = open_store(&dir.to_string_lossy()).unwrap();
        std::os::unix::fs::symlink(dir.join("missing"), dir.join("s.json")).unwrap();
        let m = Manifest {
            id: "s".into(),
            ..Default::default()
        };
        assert!(matches!(store.save(&m).unwrap_err(), StoreError::Exists(_)));
    }

    #[cfg(unix)]
    #[test]
    fn list_and_load_skip_fifos_and_symlinks() {
        let dir = tmp_dir("fifo");
        let store = open_store(&dir.to_string_lossy()).unwrap();
        let m = Manifest {
            id: "real".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            ..Default::default()
        };
        store.save(&m).unwrap();
        let fifo = dir.join("pipe.json");
        let status = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap();
        assert!(status.success());
        std::os::unix::fs::symlink(dir.join("real.json"), dir.join("alias.json")).unwrap();
        let ids: Vec<String> = store.list().unwrap().into_iter().map(|m| m.id).collect();
        assert_eq!(ids, vec!["real".to_string()]);
        assert!(matches!(store.load("pipe").unwrap_err(), StoreError::Io(_)));
        assert!(matches!(
            store.load("alias").unwrap_err(),
            StoreError::Io(_)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn resolve_shared_rejects_a_symlinked_store_directory() {
        let dir = tmp_dir("sharedlink");
        let outside = dir.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        let repo = dir.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        std::os::unix::fs::symlink(&outside, repo.join(".ctx")).unwrap();
        let err = resolve(ResolveOptions {
            shared: true,
            root: repo.to_string_lossy().into_owned(),
        })
        .unwrap_err();
        assert!(err.to_string().contains("symlinked"), "{err}");

        std::fs::remove_file(repo.join(".ctx")).unwrap();
        std::fs::create_dir_all(repo.join(".ctx")).unwrap();
        std::os::unix::fs::symlink(&outside, repo.join(".ctx/replay")).unwrap();
        assert!(resolve(ResolveOptions {
            shared: true,
            root: repo.to_string_lossy().into_owned(),
        })
        .is_err());
    }

    #[test]
    fn validate_id_rejects_dotfiles() {
        assert!(validate_id(".hidden").is_err());
        assert!(validate_id("..").is_err());
        assert!(validate_id(".").is_err());
        assert!(validate_id("").is_err());
        assert!(validate_id("a/b").is_err());
        assert!(validate_id("ok-id_1.2").is_ok());
    }
}
