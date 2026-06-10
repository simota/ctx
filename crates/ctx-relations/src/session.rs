// crates/ctx-relations/src/session.rs
//
// Phase 4 ADR-002 sticky-handle: load the import-graph Index ONCE per
// root, then route many lookups through the cached session. The existing
// Phase 2 build_cached path is retained for callers that only need a
// one-shot Index serialization; this module is the SECOND access point
// for multi-query consumers (web handlers, browse TUI keystrokes).
//
// QUERY SHAPES
// ============
//   "refs"          — given {"path": "<file>"}, return the list of
//                     importers of that file (who depends on it).
//   "deps"          — given {"path": "<file>"}, return the list of
//                     imports of that file (what it depends on).
//   "callers"       — alias for "refs" framed in caller terminology.
//                     Same result shape; kept separate so the FFI surface
//                     reads naturally from a Go caller's POV.
//   "index_summary" — serialize the full Index (byte-equal to
//                     build_cached output). Used by callers that already
//                     held an in-memory snapshot.
//
// The query/result envelopes are tiny JSON blobs so the per-call
// marshal cost stays measured-in-microseconds — the whole point of the
// sticky handle.

use serde::Deserialize;

use crate::build;
use crate::types::Index;

/// A session holds a single-root Index in memory. Multiple queries
/// against the same session amortise away the walk + parse cost.
pub struct RelationsSession {
    root: String,
    index: Index,
}

impl RelationsSession {
    /// Build the Index for `root` and return a session. The session is
    /// stand-alone — it does NOT participate in the per-root Mutex cache
    /// in cache.rs. (The cgo bridge owns session lifetime explicitly.)
    pub fn open(root: &str) -> std::io::Result<Self> {
        let idx = build::build(root)?;
        Ok(Self {
            root: root.to_string(),
            index: idx,
        })
    }

    /// Borrow the underlying Index — used by tests for byte-equality
    /// checks against the stateless path.
    pub fn index(&self) -> &Index {
        &self.index
    }

    /// Root the session was opened against.
    pub fn root(&self) -> &str {
        &self.root
    }

    /// Run a kind-tagged query against the cached Index. Returns the
    /// serialized result envelope (always valid JSON, never empty).
    pub fn query(&self, kind: &str, args_json: &str) -> Result<String, QueryError> {
        match kind {
            "refs" | "callers" => self.query_refs(args_json),
            "deps" => self.query_deps(args_json),
            "edges" => self.query_edges(args_json),
            "index_summary" => self.query_index_summary(),
            other => Err(QueryError::UnknownKind(other.to_string())),
        }
    }

    fn query_edges(&self, args_json: &str) -> Result<String, QueryError> {
        let args: PathArgs = parse_path_args(args_json)?;
        let edges = self.index.edges(&args.path);
        let env = EdgesResponse {
            path: args.path,
            module_path: self.index.module_path.clone(),
            imports: edges.imports,
            importers: edges.importers,
        };
        serde_json::to_string(&env).map_err(|_| QueryError::Serialize)
    }

    fn query_refs(&self, args_json: &str) -> Result<String, QueryError> {
        let args: PathArgs = parse_path_args(args_json)?;
        let edges = self.index.edges(&args.path);
        let env = RefsResponse {
            path: args.path,
            importers: edges.importers,
        };
        serde_json::to_string(&env).map_err(|_| QueryError::Serialize)
    }

    fn query_deps(&self, args_json: &str) -> Result<String, QueryError> {
        let args: PathArgs = parse_path_args(args_json)?;
        let edges = self.index.edges(&args.path);
        let env = DepsResponse {
            path: args.path,
            imports: edges.imports,
        };
        serde_json::to_string(&env).map_err(|_| QueryError::Serialize)
    }

    fn query_index_summary(&self) -> Result<String, QueryError> {
        // BYTE-EQUAL to ctx_relations_build / ctx_relations_build_cached
        // output. Tests rely on this.
        serde_json::to_string(&self.index).map_err(|_| QueryError::Serialize)
    }
}

#[derive(Debug, Deserialize)]
struct PathArgs {
    #[serde(default)]
    path: String,
}

fn parse_path_args(args_json: &str) -> Result<PathArgs, QueryError> {
    if args_json.trim().is_empty() {
        return Err(QueryError::BadArgs);
    }
    let parsed: PathArgs = serde_json::from_str(args_json).map_err(|_| QueryError::BadArgs)?;
    if parsed.path.is_empty() {
        return Err(QueryError::BadArgs);
    }
    Ok(parsed)
}

#[derive(Debug, serde::Serialize)]
struct RefsResponse {
    path: String,
    importers: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
struct DepsResponse {
    path: String,
    imports: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
struct EdgesResponse {
    path: String,
    module_path: String,
    imports: Vec<String>,
    importers: Vec<String>,
}

/// Error variants for `RelationsSession::query`. Mapped to FFI return
/// codes by the ffi.rs glue.
#[derive(Debug)]
pub enum QueryError {
    UnknownKind(String),
    BadArgs,
    Serialize,
}

// Compile-time sanity: the session must cross thread boundaries safely
// for the concurrent-queries test and for SetFinalizer-style usage on
// the Go side.
const _: fn() = || {
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}
    assert_send::<RelationsSession>();
    assert_sync::<RelationsSession>();
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "rel-session-{}-{}-{}",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn write_tree(dir: &Path, files: &[(&str, &str)]) {
        for (rel, content) in files {
            let p = dir.join(rel);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&p, content).unwrap();
        }
    }

    fn make_repo(label: &str) -> std::path::PathBuf {
        let dir = temp_dir(label);
        write_tree(
            &dir,
            &[
                ("go.mod", "module example.com/m\n"),
                (
                    "main.go",
                    "package main\nimport \"example.com/m/lib\"\nfunc main() {}\n",
                ),
                ("lib/a.go", "package lib\n"),
            ],
        );
        dir
    }

    #[test]
    fn session_open_returns_built_index() {
        let dir = make_repo("open");
        let s = RelationsSession::open(&dir.to_string_lossy()).unwrap();
        assert_eq!(s.index().module_path, "example.com/m");
    }

    #[test]
    fn query_refs_returns_importers() {
        let dir = make_repo("refs");
        let s = RelationsSession::open(&dir.to_string_lossy()).unwrap();
        let body = s
            .query("refs", r#"{"path":"lib/a.go"}"#)
            .expect("refs query");
        assert!(body.contains("\"importers\""));
        assert!(body.contains("main.go"), "{body}");
    }

    #[test]
    fn query_deps_returns_imports() {
        let dir = make_repo("deps");
        let s = RelationsSession::open(&dir.to_string_lossy()).unwrap();
        let body = s
            .query("deps", r#"{"path":"main.go"}"#)
            .expect("deps query");
        assert!(body.contains("\"imports\""));
        assert!(body.contains("lib/a.go"), "{body}");
    }

    #[test]
    fn query_callers_is_alias_for_refs() {
        let dir = make_repo("callers");
        let s = RelationsSession::open(&dir.to_string_lossy()).unwrap();
        let refs = s.query("refs", r#"{"path":"lib/a.go"}"#).unwrap();
        let cls = s.query("callers", r#"{"path":"lib/a.go"}"#).unwrap();
        assert_eq!(refs, cls);
    }

    #[test]
    fn query_index_summary_matches_serialized_build() {
        let dir = make_repo("summary");
        let root = dir.to_string_lossy().to_string();
        let s = RelationsSession::open(&root).unwrap();
        let body = s.query("index_summary", "").unwrap();
        let direct = serde_json::to_string(&build::build(&root).unwrap()).unwrap();
        assert_eq!(body, direct);
    }

    #[test]
    fn query_rejects_bad_args() {
        let dir = make_repo("badargs");
        let s = RelationsSession::open(&dir.to_string_lossy()).unwrap();
        assert!(matches!(
            s.query("refs", "not-json"),
            Err(QueryError::BadArgs)
        ));
        assert!(matches!(
            s.query("refs", r#"{}"#),
            Err(QueryError::BadArgs)
        ));
    }

    #[test]
    fn query_rejects_unknown_kind() {
        let dir = make_repo("badkind");
        let s = RelationsSession::open(&dir.to_string_lossy()).unwrap();
        assert!(matches!(
            s.query("bogus", r#"{"path":"main.go"}"#),
            Err(QueryError::UnknownKind(_))
        ));
    }
}
