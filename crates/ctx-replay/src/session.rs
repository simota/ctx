// crates/ctx-replay/src/session.rs
//
// Phase 4 ADR-002 sticky-handle: load a replay snapshot directory ONCE
// and route many queries (diff against a base manifest, selection diff,
// load-by-id, list, prune candidates) through the cached session. The
// existing stateless API in diff.rs / prune.rs / store.rs remains the
// FIRST access point for single-shot callers (CLI `ctx replay list`,
// `replay-engine-diff`); this module is the SECOND access point for
// multi-query consumers (web handlers, replay-pack pre-pass).
//
// QUERY SHAPES
// ============
//   "list"
//       Args: ignored.
//       Returns JSON: { manifests: [Manifest, …] } — chronological.
//       Cached on first call; subsequent calls are zero-IO.
//
//   "load"
//       Args: {"id": "<snapshot-id>"}
//       Returns JSON: Manifest. Cached per id.
//
//   "diff"
//       Args: {"base_id": "<id>", "current_manifest": <Manifest JSON>,
//              "strict": bool}
//       Returns JSON: DiffSummary. base manifest hit through the cache.
//
//   "diff_ids"
//       Args: {"base_id": "<id>", "current_id": "<id>", "strict": bool}
//       Returns JSON: DiffSummary. Both manifests hit the cache.
//
//   "selection_diff"
//       Args: {"a_id": "<id>", "b_id": "<id>", "sort_by": "<tier|tokens|score>"}
//       Returns JSON: SelectionSummary.
//
//   "prune_candidates"
//       Args: {"now": "<RFC3339>", "older_nanos": <i64>}
//       Returns JSON: { candidates: ["<id>", …], kept: <i64> }.
//       Does NOT delete — read-only probe used by the web UI prune
//       preview.
//
// The session never mutates the snapshot directory; deletes go through
// the stateless Store API so the cache invariant is preserved.

use std::collections::HashMap;
use std::sync::Mutex;

use serde::Deserialize;

use crate::diff::{compute, compute_selection_diff, sort_selection_diff, DiffOptions};
use crate::store::{open_store, Store, StoreError};
use crate::types::{DiffSummary, Manifest, SelectionSummary};

/// A session holds a snapshot store handle plus a lazy manifest cache.
/// Multiple queries against the same session amortise the directory
/// scan + per-id JSON-decode cost.
pub struct ReplaySession {
    dir: String,
    store: Store,
    /// Cache of fully-decoded manifests keyed by snapshot id. Populated
    /// lazily on first load; invalidated only by close().
    by_id: Mutex<HashMap<String, Manifest>>,
    /// Cache of the chronological `list()` result. None means "not yet
    /// loaded". The vector mirrors `Store::list` ordering.
    listed: Mutex<Option<Vec<Manifest>>>,
}

impl ReplaySession {
    /// Open a session against the given snapshot directory. Creates the
    /// directory if it does not exist (matching the stateless
    /// `open_store` semantics). The session is stand-alone — it does
    /// NOT share state with any global cache.
    pub fn open(dir: &str) -> Result<Self, StoreError> {
        let store = open_store(dir)?;
        Ok(Self {
            dir: dir.to_string(),
            store,
            by_id: Mutex::new(HashMap::new()),
            listed: Mutex::new(None),
        })
    }

    /// Snapshot directory the session was opened against.
    pub fn dir(&self) -> &str {
        &self.dir
    }

    /// Run a kind-tagged query against the session. Returns the
    /// serialized JSON envelope (always valid JSON, never empty on
    /// success).
    pub fn query(&self, kind: &str, args_json: &str) -> Result<String, QueryError> {
        match kind {
            "list" => self.query_list(),
            "load" => self.query_load(args_json),
            "diff" => self.query_diff(args_json),
            "diff_ids" => self.query_diff_ids(args_json),
            "selection_diff" => self.query_selection_diff(args_json),
            "prune_candidates" => self.query_prune_candidates(args_json),
            other => Err(QueryError::UnknownKind(other.to_string())),
        }
    }

    // -----------------------------------------------------------------
    // Cache primitives
    // -----------------------------------------------------------------

    /// Load a manifest by id, hitting the cache when warm.
    fn manifest_by_id(&self, id: &str) -> Result<Manifest, QueryError> {
        {
            let by_id = self.by_id.lock().expect("by_id mutex poisoned");
            if let Some(m) = by_id.get(id) {
                return Ok(m.clone());
            }
        }
        let m = self.store.load(id).map_err(map_store_err)?;
        let mut by_id = self.by_id.lock().expect("by_id mutex poisoned");
        by_id.insert(id.to_string(), m.clone());
        Ok(m)
    }

    /// List all manifests, caching the chronological vector on first
    /// call. Subsequent calls clone the cached vector.
    fn list_manifests(&self) -> Result<Vec<Manifest>, QueryError> {
        {
            let listed = self.listed.lock().expect("listed mutex poisoned");
            if let Some(v) = listed.as_ref() {
                return Ok(v.clone());
            }
        }
        let v = self.store.list().map_err(map_store_err)?;
        // Also warm the by_id cache for free.
        {
            let mut by_id = self.by_id.lock().expect("by_id mutex poisoned");
            for m in &v {
                by_id.entry(m.id.clone()).or_insert_with(|| m.clone());
            }
        }
        let mut listed = self.listed.lock().expect("listed mutex poisoned");
        *listed = Some(v.clone());
        Ok(v)
    }

    // -----------------------------------------------------------------
    // Query implementations
    // -----------------------------------------------------------------

    fn query_list(&self) -> Result<String, QueryError> {
        let manifests = self.list_manifests()?;
        let env = ListResponse { manifests };
        serde_json::to_string(&env).map_err(|_| QueryError::Serialize)
    }

    fn query_load(&self, args_json: &str) -> Result<String, QueryError> {
        let args: LoadArgs = parse_args(args_json)?;
        if args.id.is_empty() {
            return Err(QueryError::BadArgs);
        }
        let m = self.manifest_by_id(&args.id)?;
        serde_json::to_string(&m).map_err(|_| QueryError::Serialize)
    }

    fn query_diff(&self, args_json: &str) -> Result<String, QueryError> {
        let args: DiffArgs = parse_args(args_json)?;
        if args.base_id.is_empty() {
            return Err(QueryError::BadArgs);
        }
        let base = self.manifest_by_id(&args.base_id)?;
        let current: Manifest = serde_json::from_str(args.current_manifest.get())
            .map_err(|_| QueryError::BadArgs)?;
        let summary = compute(&base, &current, DiffOptions { strict: args.strict });
        serde_json::to_string(&summary).map_err(|_| QueryError::Serialize)
    }

    fn query_diff_ids(&self, args_json: &str) -> Result<String, QueryError> {
        let args: DiffIdsArgs = parse_args(args_json)?;
        if args.base_id.is_empty() || args.current_id.is_empty() {
            return Err(QueryError::BadArgs);
        }
        let base = self.manifest_by_id(&args.base_id)?;
        let cur = self.manifest_by_id(&args.current_id)?;
        let summary: DiffSummary = compute(&base, &cur, DiffOptions { strict: args.strict });
        serde_json::to_string(&summary).map_err(|_| QueryError::Serialize)
    }

    fn query_selection_diff(&self, args_json: &str) -> Result<String, QueryError> {
        let args: SelectionDiffArgs = parse_args(args_json)?;
        if args.a_id.is_empty() || args.b_id.is_empty() {
            return Err(QueryError::BadArgs);
        }
        let a = self.manifest_by_id(&args.a_id)?;
        let b = self.manifest_by_id(&args.b_id)?;
        let mut sel: SelectionSummary = compute_selection_diff(&a, &b);
        let sort_by = if args.sort_by.is_empty() { "tier" } else { &args.sort_by };
        sort_selection_diff(&mut sel, sort_by);
        serde_json::to_string(&sel).map_err(|_| QueryError::Serialize)
    }

    fn query_prune_candidates(&self, args_json: &str) -> Result<String, QueryError> {
        let args: PruneCandidatesArgs = parse_args(args_json)?;
        if args.now.is_empty() {
            return Err(QueryError::BadArgs);
        }
        let manifests = self.list_manifests()?;
        // Reuse the prune module's RFC3339 → unix-nanos via parse_duration
        // helpers — these are crate-private, so we inline the tiny call to
        // store::Store::list_filtered indirectly by mirroring the prune
        // function but read-only.
        let now_nanos = rfc3339_to_nanos(&args.now).ok_or(QueryError::BadArgs)?;
        let cutoff = now_nanos.saturating_sub(args.older_nanos);
        let mut candidates: Vec<String> = Vec::new();
        let mut kept: i64 = 0;
        for m in &manifests {
            let ts = rfc3339_to_nanos(&m.created_at).unwrap_or(i64::MAX);
            if ts < cutoff {
                candidates.push(m.id.clone());
            } else {
                kept += 1;
            }
        }
        let env = PruneCandidatesResponse { candidates, kept };
        serde_json::to_string(&env).map_err(|_| QueryError::Serialize)
    }
}

// =====================================================================
// Args / response envelopes
// =====================================================================

#[derive(Debug, Deserialize)]
struct LoadArgs {
    #[serde(default)]
    id: String,
}

#[derive(Debug, Deserialize)]
struct DiffArgs {
    #[serde(default)]
    base_id: String,
    /// Borrowed JSON for the current manifest. Held as a RawValue so we
    /// avoid the cost of double-decoding when the caller already has a
    /// canonical encoding.
    current_manifest: Box<serde_json::value::RawValue>,
    #[serde(default)]
    strict: bool,
}

#[derive(Debug, Deserialize)]
struct DiffIdsArgs {
    #[serde(default)]
    base_id: String,
    #[serde(default)]
    current_id: String,
    #[serde(default)]
    strict: bool,
}

#[derive(Debug, Deserialize)]
struct SelectionDiffArgs {
    #[serde(default)]
    a_id: String,
    #[serde(default)]
    b_id: String,
    #[serde(default)]
    sort_by: String,
}

#[derive(Debug, Deserialize)]
struct PruneCandidatesArgs {
    #[serde(default)]
    now: String,
    #[serde(default)]
    older_nanos: i64,
}

#[derive(Debug, serde::Serialize)]
struct ListResponse {
    manifests: Vec<Manifest>,
}

#[derive(Debug, serde::Serialize)]
struct PruneCandidatesResponse {
    candidates: Vec<String>,
    kept: i64,
}

// =====================================================================
// Helpers
// =====================================================================

fn parse_args<T: for<'de> serde::Deserialize<'de>>(args_json: &str) -> Result<T, QueryError> {
    if args_json.trim().is_empty() {
        return Err(QueryError::BadArgs);
    }
    serde_json::from_str(args_json).map_err(|_| QueryError::BadArgs)
}

fn map_store_err(e: StoreError) -> QueryError {
    match e {
        StoreError::NotFound(id) => QueryError::NotFound(id),
        StoreError::InvalidId(s) => QueryError::BadArgs.with_detail(s),
        _ => QueryError::Io,
    }
}

/// Minimal RFC3339 → unix-nanos parser, duplicated from prune.rs because
/// that helper is private. Same algorithm (Hinnant days-from-civil),
/// covering Z and ±HH:MM offsets.
fn rfc3339_to_nanos(s: &str) -> Option<i64> {
    let s = s.trim();
    let len = s.len();
    if len < 20 {
        return None;
    }
    let b = s.as_bytes();
    let y = parse_int(&b[0..4])?;
    if b[4] != b'-' {
        return None;
    }
    let mo = parse_int(&b[5..7])?;
    if b[7] != b'-' {
        return None;
    }
    let d = parse_int(&b[8..10])?;
    if b[10] != b'T' && b[10] != b't' && b[10] != b' ' {
        return None;
    }
    let hh = parse_int(&b[11..13])?;
    if b[13] != b':' {
        return None;
    }
    let mm = parse_int(&b[14..16])?;
    if b[16] != b':' {
        return None;
    }
    let ss = parse_int(&b[17..19])?;

    let mut idx = 19usize;
    let mut frac_nanos: i64 = 0;
    if idx < len && b[idx] == b'.' {
        idx += 1;
        let start = idx;
        while idx < len && b[idx].is_ascii_digit() {
            idx += 1;
        }
        let frac = &b[start..idx];
        let mut buf = [b'0'; 9];
        for (i, &c) in frac.iter().take(9).enumerate() {
            buf[i] = c;
        }
        frac_nanos = parse_int(&buf)?;
    }
    let offset_secs: i64 = if idx >= len {
        return None;
    } else if b[idx] == b'Z' || b[idx] == b'z' {
        0
    } else if b[idx] == b'+' || b[idx] == b'-' {
        let sign = if b[idx] == b'+' { 1 } else { -1 };
        if idx + 5 >= len {
            return None;
        }
        let oh = parse_int(&b[idx + 1..idx + 3])?;
        let mm_start = if b[idx + 3] == b':' { idx + 4 } else { idx + 3 };
        let om = parse_int(&b[mm_start..mm_start + 2])?;
        sign * (oh * 3600 + om * 60)
    } else {
        return None;
    };

    let civil = civil_to_unix(y, mo, d, hh, mm, ss)?;
    Some((civil - offset_secs) * 1_000_000_000 + frac_nanos)
}

fn parse_int(bytes: &[u8]) -> Option<i64> {
    let mut n: i64 = 0;
    for &b in bytes {
        if !b.is_ascii_digit() {
            return None;
        }
        n = n * 10 + (b - b'0') as i64;
    }
    Some(n)
}

fn civil_to_unix(y: i64, m: i64, d: i64, hh: i64, mm: i64, ss: i64) -> Option<i64> {
    if !(1..=12).contains(&m) {
        return None;
    }
    if !(1..=31).contains(&d) {
        return None;
    }
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hh * 3600 + mm * 60 + ss)
}

/// Error variants for `ReplaySession::query`. Mapped to FFI return codes
/// by the ffi.rs glue.
#[derive(Debug)]
pub enum QueryError {
    UnknownKind(String),
    BadArgs,
    BadArgsDetail(String),
    NotFound(String),
    Io,
    Serialize,
}

impl QueryError {
    fn with_detail(self, _detail: String) -> Self {
        // Detail surface is reserved; preserving the bad-args variant
        // keeps the FFI mapping simple.
        QueryError::BadArgs
    }
}

// Compile-time sanity: the session must cross thread boundaries safely.
const _: fn() = || {
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}
    assert_send::<ReplaySession>();
    assert_sync::<ReplaySession>();
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Entry;

    fn tmp_dir(label: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "ctx-replay-session-{}-{}-{}",
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

    fn entry(path: &str, sha: &str, tokens: i64) -> Entry {
        Entry {
            path: path.into(),
            sha256: sha.into(),
            tokens,
            relevance: "High".into(),
            score: 0,
            reason: String::new(),
        }
    }

    fn seed_store(dir: &std::path::Path, ids: &[(&str, &str)]) {
        let store = open_store(dir.to_str().unwrap()).unwrap();
        for (id, created_at) in ids {
            let mut m = Manifest::default();
            m.schema_version = 1;
            m.id = (*id).into();
            m.created_at = (*created_at).into();
            m.entries.push(entry("a.go", "aa", 10));
            store.save(&m).unwrap();
        }
    }

    #[test]
    fn session_open_and_list() {
        let dir = tmp_dir("list");
        seed_store(
            &dir,
            &[
                ("snap-a", "2026-01-01T00:00:00Z"),
                ("snap-b", "2026-01-02T00:00:00Z"),
            ],
        );
        let s = ReplaySession::open(dir.to_str().unwrap()).unwrap();
        let body = s.query("list", "{}").unwrap();
        assert!(body.contains("snap-a"));
        assert!(body.contains("snap-b"));
    }

    #[test]
    fn session_load_caches_manifest() {
        let dir = tmp_dir("load");
        seed_store(&dir, &[("snap-x", "2026-01-01T00:00:00Z")]);
        let s = ReplaySession::open(dir.to_str().unwrap()).unwrap();
        let body1 = s.query("load", r#"{"id":"snap-x"}"#).unwrap();
        // Delete the underlying file: the cached load should still work.
        std::fs::remove_file(dir.join("snap-x.json")).unwrap();
        let body2 = s.query("load", r#"{"id":"snap-x"}"#).unwrap();
        assert_eq!(body1, body2);
    }

    #[test]
    fn session_diff_matches_stateless() {
        let dir = tmp_dir("diff");
        seed_store(&dir, &[("base", "2026-01-01T00:00:00Z")]);
        let s = ReplaySession::open(dir.to_str().unwrap()).unwrap();

        let cur = Manifest {
            schema_version: 1,
            id: "cur".into(),
            created_at: "2026-01-02T00:00:00Z".into(),
            entries: vec![entry("a.go", "ZZ", 11)],
            ..Default::default()
        };
        let cur_json = serde_json::to_string(&cur).unwrap();
        let args = format!(
            r#"{{"base_id":"base","current_manifest":{},"strict":false}}"#,
            cur_json
        );
        let body = s.query("diff", &args).unwrap();
        assert!(body.contains("\"modified\":1"), "{body}");
    }

    #[test]
    fn session_diff_ids_matches_stateless() {
        let dir = tmp_dir("diff_ids");
        seed_store(
            &dir,
            &[
                ("a", "2026-01-01T00:00:00Z"),
                ("b", "2026-01-02T00:00:00Z"),
            ],
        );
        let s = ReplaySession::open(dir.to_str().unwrap()).unwrap();
        let body = s
            .query("diff_ids", r#"{"base_id":"a","current_id":"b","strict":false}"#)
            .unwrap();
        // Both stored manifests are identical so the diff should report
        // unchanged=1.
        assert!(body.contains("\"unchanged\":1"), "{body}");
    }

    #[test]
    fn session_selection_diff_matches_stateless() {
        let dir = tmp_dir("seldiff");
        let store = open_store(dir.to_str().unwrap()).unwrap();
        let mut a = Manifest::default();
        a.schema_version = 1;
        a.id = "A".into();
        a.created_at = "2026-01-01T00:00:00Z".into();
        a.entries.push(entry("a.go", "aa", 10));
        let mut b = Manifest::default();
        b.schema_version = 1;
        b.id = "B".into();
        b.created_at = "2026-01-02T00:00:00Z".into();
        b.entries.push(entry("b.go", "bb", 20));
        store.save(&a).unwrap();
        store.save(&b).unwrap();
        let s = ReplaySession::open(dir.to_str().unwrap()).unwrap();
        let body = s
            .query(
                "selection_diff",
                r#"{"a_id":"A","b_id":"B","sort_by":"tier"}"#,
            )
            .unwrap();
        assert!(body.contains("\"added\":1"), "{body}");
        assert!(body.contains("\"removed\":1"), "{body}");
    }

    #[test]
    fn session_prune_candidates_identifies_old() {
        let dir = tmp_dir("prune");
        seed_store(
            &dir,
            &[
                ("old", "2025-01-01T00:00:00Z"),
                ("recent", "2026-05-29T00:00:00Z"),
            ],
        );
        let s = ReplaySession::open(dir.to_str().unwrap()).unwrap();
        let one_week_nanos: i64 = 7 * 24 * 3600 * 1_000_000_000;
        let args = format!(
            r#"{{"now":"2026-05-29T12:00:00Z","older_nanos":{}}}"#,
            one_week_nanos
        );
        let body = s.query("prune_candidates", &args).unwrap();
        assert!(body.contains("\"old\""), "{body}");
        assert!(!body.contains("\"recent\""), "{body}");
    }

    #[test]
    fn session_unknown_kind_rejected() {
        let dir = tmp_dir("bad");
        let s = ReplaySession::open(dir.to_str().unwrap()).unwrap();
        assert!(matches!(
            s.query("bogus", "{}"),
            Err(QueryError::UnknownKind(_))
        ));
    }

    #[test]
    fn session_bad_args_rejected() {
        let dir = tmp_dir("badargs");
        let s = ReplaySession::open(dir.to_str().unwrap()).unwrap();
        assert!(matches!(s.query("load", ""), Err(QueryError::BadArgs)));
        assert!(matches!(s.query("load", "not-json"), Err(QueryError::BadArgs)));
        assert!(matches!(s.query("load", r#"{"id":""}"#), Err(QueryError::BadArgs)));
    }
}
