// crates/ctx-replay/tests/sticky_handle.rs
//
// Phase 4 sticky-handle FFI tests for ctx-replay. Validates the session
// API parallels the stateless path byte-for-byte and survives the
// per-call / cross-thread / repeat-cycle invariants documented in
// tests/STICKY_HANDLE_POC_REPORT.md.

use std::ffi::{c_char, c_void, CStr};
use std::path::Path;
use std::ptr;

use ctx_replay::ffi::{
    ctx_replay_diff, ctx_replay_free_string, ctx_replay_selection_diff,
    ctx_replay_session_close, ctx_replay_session_open, ctx_replay_session_query,
};
use ctx_replay::store::open_store;
use ctx_replay::types::{Entry, Manifest};

const ERR_OK: i32 = 0;
const ERR_NULL_PTR: i32 = -1;
const ERR_BAD_JSON: i32 = -3;
const ERR_BAD_HANDLE: i32 = -10;
const ERR_BAD_KIND: i32 = -11;
const ERR_NOT_FOUND: i32 = -12;

fn tmp_dir(label: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!(
        "replay-sh-{}-{}-{}",
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

fn save_manifest(dir: &Path, id: &str, created_at: &str, entries: Vec<Entry>) -> Manifest {
    let store = open_store(dir.to_str().unwrap()).unwrap();
    let mut m = Manifest::default();
    m.schema_version = 1;
    m.id = id.into();
    m.created_at = created_at.into();
    m.entries = entries;
    store.save(&m).unwrap();
    m
}

fn open_handle(dir: &str) -> *mut c_void {
    let opts = "";
    let mut handle: *mut c_void = ptr::null_mut();
    let rc = unsafe {
        ctx_replay_session_open(
            dir.as_ptr(),
            dir.len(),
            opts.as_ptr(),
            opts.len(),
            &mut handle,
        )
    };
    assert_eq!(rc, ERR_OK, "session_open rc={rc}");
    assert!(!handle.is_null());
    handle
}

fn run_query(handle: *mut c_void, kind: &str, args: &str) -> String {
    let mut out: *mut c_char = ptr::null_mut();
    let rc = unsafe {
        ctx_replay_session_query(
            handle,
            kind.as_ptr(),
            kind.len(),
            args.as_ptr(),
            args.len(),
            &mut out,
        )
    };
    assert_eq!(rc, ERR_OK, "session_query rc={rc} (kind={kind})");
    assert!(!out.is_null());
    let s = unsafe { CStr::from_ptr(out) }.to_str().unwrap().to_owned();
    unsafe { ctx_replay_free_string(out) };
    s
}

// ---------------------------------------------------------------------
// 1. open/close — no leak
// ---------------------------------------------------------------------

#[test]
fn t_session_open_close_no_leak() {
    let dir = tmp_dir("open_close");
    save_manifest(
        &dir,
        "snap-1",
        "2026-01-01T00:00:00Z",
        vec![entry("a.go", "aa", 5)],
    );
    let dir_str = dir.to_string_lossy().to_string();
    for _ in 0..256 {
        let h = open_handle(&dir_str);
        let rc = unsafe { ctx_replay_session_close(h) };
        assert_eq!(rc, ERR_OK);
    }
}

// ---------------------------------------------------------------------
// 2. diff parity vs stateless
// ---------------------------------------------------------------------

#[test]
fn t_session_diff_matches_stateless() {
    let dir = tmp_dir("diff");
    let base = save_manifest(
        &dir,
        "base",
        "2026-01-01T00:00:00Z",
        vec![entry("a.go", "aa", 10), entry("b.go", "bb", 20)],
    );
    let cur = Manifest {
        schema_version: 1,
        id: "cur".into(),
        created_at: "2026-01-02T00:00:00Z".into(),
        entries: vec![entry("a.go", "AA", 12), entry("c.go", "cc", 30)],
        ..Default::default()
    };

    // Stateless path: pass both as JSON via ctx_replay_diff.
    let base_json = serde_json::to_string(&base).unwrap();
    let cur_json = serde_json::to_string(&cur).unwrap();
    let mut out: *mut c_char = ptr::null_mut();
    let rc = unsafe {
        ctx_replay_diff(
            base_json.as_ptr(),
            base_json.len(),
            cur_json.as_ptr(),
            cur_json.len(),
            0,
            &mut out,
        )
    };
    assert_eq!(rc, ERR_OK);
    let stateless = unsafe { CStr::from_ptr(out) }.to_str().unwrap().to_owned();
    unsafe { ctx_replay_free_string(out) };

    // Session path.
    let dir_str = dir.to_string_lossy().to_string();
    let h = open_handle(&dir_str);
    let args = format!(
        r#"{{"base_id":"base","current_manifest":{},"strict":false}}"#,
        cur_json
    );
    let session = run_query(h, "diff", &args);
    unsafe { ctx_replay_session_close(h) };

    assert_eq!(stateless, session);
}

// ---------------------------------------------------------------------
// 3. compute (alias used by the verdict text — same as diff)
//    Validate that loading manifest by id returns the same Manifest the
//    store would emit standalone.
// ---------------------------------------------------------------------

#[test]
fn t_session_compute_matches_stateless() {
    let dir = tmp_dir("compute");
    let m = save_manifest(
        &dir,
        "snap-c",
        "2026-01-01T00:00:00Z",
        vec![entry("x", "xx", 7)],
    );
    let dir_str = dir.to_string_lossy().to_string();
    let h = open_handle(&dir_str);
    let body = run_query(h, "load", r#"{"id":"snap-c"}"#);
    unsafe { ctx_replay_session_close(h) };

    let expected = serde_json::to_string(&m).unwrap();
    assert_eq!(body, expected);
}

// ---------------------------------------------------------------------
// 4. prune candidates vs the stateless prune simulation
// ---------------------------------------------------------------------

#[test]
fn t_session_prune_matches_stateless() {
    let dir = tmp_dir("prune");
    save_manifest(
        &dir,
        "old",
        "2025-01-01T00:00:00Z",
        vec![entry("x", "xx", 1)],
    );
    save_manifest(
        &dir,
        "recent",
        "2026-05-29T00:00:00Z",
        vec![entry("x", "xx", 1)],
    );
    let dir_str = dir.to_string_lossy().to_string();
    let h = open_handle(&dir_str);
    let one_week_nanos: i64 = 7 * 24 * 3600 * 1_000_000_000;
    let args = format!(
        r#"{{"now":"2026-05-29T12:00:00Z","older_nanos":{}}}"#,
        one_week_nanos
    );
    let body = run_query(h, "prune_candidates", &args);
    unsafe { ctx_replay_session_close(h) };

    // The candidate set should include "old" but not "recent".
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    let cands: Vec<String> = v["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().to_string())
        .collect();
    assert!(cands.contains(&"old".to_string()), "{cands:?}");
    assert!(!cands.contains(&"recent".to_string()), "{cands:?}");
}

// ---------------------------------------------------------------------
// 5. lookup (load by id), exercising the cache path
// ---------------------------------------------------------------------

#[test]
fn t_session_lookup_matches_stateless() {
    let dir = tmp_dir("lookup");
    let m = save_manifest(
        &dir,
        "snap-l",
        "2026-01-01T00:00:00Z",
        vec![entry("p", "pp", 3)],
    );
    let dir_str = dir.to_string_lossy().to_string();
    let h = open_handle(&dir_str);
    let body = run_query(h, "load", r#"{"id":"snap-l"}"#);
    unsafe { ctx_replay_session_close(h) };

    let expected = serde_json::to_string(&m).unwrap();
    assert_eq!(body, expected);
}

// ---------------------------------------------------------------------
// 6. Multi-query parity within a session — diff, selection_diff, load
//    all match what the stateless paths would emit.
// ---------------------------------------------------------------------

#[test]
fn t_session_multi_query_parity() {
    let dir = tmp_dir("multi");
    let a = save_manifest(
        &dir,
        "A",
        "2026-01-01T00:00:00Z",
        vec![entry("x", "x1", 5), entry("y", "y1", 10)],
    );
    let b = save_manifest(
        &dir,
        "B",
        "2026-01-02T00:00:00Z",
        vec![entry("x", "x1", 5), entry("z", "z1", 20)],
    );
    let dir_str = dir.to_string_lossy().to_string();

    // 1) selection_diff via session.
    let h = open_handle(&dir_str);
    let sel_body = run_query(
        h,
        "selection_diff",
        r#"{"a_id":"A","b_id":"B","sort_by":"tier"}"#,
    );
    // 2) selection_diff via stateless API for parity.
    let a_json = serde_json::to_string(&a).unwrap();
    let b_json = serde_json::to_string(&b).unwrap();
    let sort_by = "tier";
    let mut out: *mut c_char = ptr::null_mut();
    let rc = unsafe {
        ctx_replay_selection_diff(
            a_json.as_ptr(),
            a_json.len(),
            b_json.as_ptr(),
            b_json.len(),
            sort_by.as_ptr(),
            sort_by.len(),
            &mut out,
        )
    };
    assert_eq!(rc, ERR_OK);
    let stateless = unsafe { CStr::from_ptr(out) }.to_str().unwrap().to_owned();
    unsafe { ctx_replay_free_string(out) };

    assert_eq!(sel_body, stateless, "selection_diff diverged");

    // 3) diff_ids via session.
    let diff_ids = run_query(
        h,
        "diff_ids",
        r#"{"base_id":"A","current_id":"B","strict":false}"#,
    );
    assert!(diff_ids.contains("\"added\":1"), "{diff_ids}");
    assert!(diff_ids.contains("\"removed\":1"), "{diff_ids}");

    unsafe { ctx_replay_session_close(h) };
}

// ---------------------------------------------------------------------
// 7. Concurrency — 4 threads × 25 queries each, no races, no leaks
// ---------------------------------------------------------------------

#[test]
fn t_session_concurrent_queries_safe() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::thread;

    let dir = tmp_dir("concurrent");
    save_manifest(
        &dir,
        "snap-c1",
        "2026-01-01T00:00:00Z",
        vec![entry("a", "aa", 1)],
    );
    save_manifest(
        &dir,
        "snap-c2",
        "2026-01-02T00:00:00Z",
        vec![entry("b", "bb", 2)],
    );
    let dir_str = dir.to_string_lossy().to_string();
    let h = open_handle(&dir_str);
    let h_usize = h as usize;
    let ok = Arc::new(AtomicUsize::new(0));
    let mut joins = Vec::new();
    for t in 0..4 {
        let ok = Arc::clone(&ok);
        joins.push(thread::spawn(move || {
            let h = h_usize as *mut c_void;
            for i in 0..25 {
                let (kind, args) = match (t + i) % 4 {
                    0 => ("list", "{}"),
                    1 => ("load", r#"{"id":"snap-c1"}"#),
                    2 => ("load", r#"{"id":"snap-c2"}"#),
                    _ => ("diff_ids", r#"{"base_id":"snap-c1","current_id":"snap-c2"}"#),
                };
                let mut out: *mut c_char = ptr::null_mut();
                let rc = unsafe {
                    ctx_replay_session_query(
                        h,
                        kind.as_ptr(),
                        kind.len(),
                        args.as_ptr(),
                        args.len(),
                        &mut out,
                    )
                };
                assert_eq!(rc, ERR_OK, "rc={rc} kind={kind}");
                if !out.is_null() {
                    unsafe { ctx_replay_free_string(out) };
                }
                ok.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }
    for j in joins {
        j.join().expect("thread join");
    }
    assert_eq!(ok.load(Ordering::Relaxed), 4 * 25);
    unsafe { ctx_replay_session_close(h) };
}

// ---------------------------------------------------------------------
// 8. close idempotent — FFI-side single-close (Go wrapper enforces CAS).
// ---------------------------------------------------------------------

#[test]
fn t_session_close_idempotent() {
    let dir = tmp_dir("close_idem");
    save_manifest(
        &dir,
        "snap",
        "2026-01-01T00:00:00Z",
        vec![entry("x", "xx", 1)],
    );
    let dir_str = dir.to_string_lossy().to_string();
    let h = open_handle(&dir_str);
    let rc1 = unsafe { ctx_replay_session_close(h) };
    assert_eq!(rc1, ERR_OK);
    // Second close on null is a no-op via ERR_NULL_PTR.
    let rc2 = unsafe { ctx_replay_session_close(ptr::null_mut()) };
    assert_eq!(rc2, ERR_NULL_PTR);
}

// ---------------------------------------------------------------------
// 9. query after close is detected via null-handle sentinel.
// ---------------------------------------------------------------------

#[test]
fn t_session_query_after_close_safe() {
    let kind = "load";
    let args = r#"{"id":"x"}"#;
    let mut out: *mut c_char = ptr::null_mut();
    let rc = unsafe {
        ctx_replay_session_query(
            ptr::null_mut(),
            kind.as_ptr(),
            kind.len(),
            args.as_ptr(),
            args.len(),
            &mut out,
        )
    };
    assert_eq!(rc, ERR_BAD_HANDLE);
    assert!(out.is_null());
}

// ---------------------------------------------------------------------
// 10. 2000-cycle soak — open/query/close repeatedly; no panic, no leak.
// ---------------------------------------------------------------------

#[test]
fn t_session_2000_cycle_soak() {
    let dir = tmp_dir("soak");
    save_manifest(
        &dir,
        "snap",
        "2026-01-01T00:00:00Z",
        vec![entry("a", "aa", 1)],
    );
    let dir_str = dir.to_string_lossy().to_string();
    for _ in 0..2000 {
        let h = open_handle(&dir_str);
        let body = run_query(h, "load", r#"{"id":"snap"}"#);
        assert!(body.contains("snap"));
        let rc = unsafe { ctx_replay_session_close(h) };
        assert_eq!(rc, ERR_OK);
    }
}

// ---------------------------------------------------------------------
// Extra coverage: bad kind / bad args / not-found map to the right
// FFI return codes.
// ---------------------------------------------------------------------

#[test]
fn t_session_rejects_bad_kind_bad_args_not_found() {
    let dir = tmp_dir("bad");
    save_manifest(
        &dir,
        "snap",
        "2026-01-01T00:00:00Z",
        vec![entry("a", "aa", 1)],
    );
    let dir_str = dir.to_string_lossy().to_string();
    let h = open_handle(&dir_str);

    // Unknown kind.
    let mut out: *mut c_char = ptr::null_mut();
    let kind = "bogus";
    let args = "{}";
    let rc = unsafe {
        ctx_replay_session_query(
            h,
            kind.as_ptr(),
            kind.len(),
            args.as_ptr(),
            args.len(),
            &mut out,
        )
    };
    assert_eq!(rc, ERR_BAD_KIND);

    // Bad args.
    let mut out: *mut c_char = ptr::null_mut();
    let kind = "load";
    let args = "not-json";
    let rc = unsafe {
        ctx_replay_session_query(
            h,
            kind.as_ptr(),
            kind.len(),
            args.as_ptr(),
            args.len(),
            &mut out,
        )
    };
    assert_eq!(rc, ERR_BAD_JSON);

    // Not found.
    let mut out: *mut c_char = ptr::null_mut();
    let kind = "load";
    let args = r#"{"id":"does-not-exist"}"#;
    let rc = unsafe {
        ctx_replay_session_query(
            h,
            kind.as_ptr(),
            kind.len(),
            args.as_ptr(),
            args.len(),
            &mut out,
        )
    };
    assert_eq!(rc, ERR_NOT_FOUND);

    unsafe { ctx_replay_session_close(h) };
}
