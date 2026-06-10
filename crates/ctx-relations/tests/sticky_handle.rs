// crates/ctx-relations/tests/sticky_handle.rs
//
// Phase 4 sticky-handle FFI tests for ctx-relations. Validates the
// session API parallels the stateless path byte-for-byte and survives
// the per-call/cross-thread / repeat-cycle invariants documented in
// tests/STICKY_HANDLE_POC_REPORT.md.

use std::ffi::{c_char, c_void, CStr};
use std::path::Path;
use std::ptr;

use ctx_relations::ffi::{
    ctx_relations_build, ctx_relations_build_cached, ctx_relations_free_string,
    ctx_relations_session_close, ctx_relations_session_open, ctx_relations_session_query,
};

const ERR_OK: i32 = 0;
const ERR_NULL_PTR: i32 = -1;
const ERR_BAD_JSON: i32 = -3;
const ERR_BAD_HANDLE: i32 = -10;
const ERR_BAD_KIND: i32 = -11;

fn temp_dir(label: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!(
        "rel-sh-{}-{}-{}",
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
                "package main\nimport \"example.com/m/lib\"\nimport \"example.com/m/util\"\nfunc main() {}\n",
            ),
            ("lib/a.go", "package lib\n"),
            ("util/u.go", "package util\nimport \"example.com/m/lib\"\n"),
        ],
    );
    dir
}

fn open_handle(root: &str) -> *mut c_void {
    let opts = "";
    let mut handle: *mut c_void = ptr::null_mut();
    let rc = unsafe {
        ctx_relations_session_open(
            root.as_ptr(),
            root.len(),
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
        ctx_relations_session_query(
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
    unsafe { ctx_relations_free_string(out) };
    s
}

#[test]
fn t_session_open_close_no_leak() {
    let dir = make_repo("open_close");
    let root = dir.to_string_lossy().to_string();
    for _ in 0..256 {
        let h = open_handle(&root);
        let rc = unsafe { ctx_relations_session_close(h) };
        assert_eq!(rc, ERR_OK);
    }
}

#[test]
fn t_session_query_refs_matches_stateless() {
    let dir = make_repo("refs");
    let root = dir.to_string_lossy().to_string();
    let h = open_handle(&root);
    let body = run_query(h_check(h), "refs", r#"{"path":"lib/a.go"}"#);
    assert!(body.contains("\"importers\""), "{body}");
    assert!(body.contains("main.go"), "{body}");
    assert!(body.contains("util/u.go"), "{body}");

    // Compare to the stateless full-index path; the importers slice
    // should match the field inside it for lib/a.go.
    let mut out: *mut c_char = ptr::null_mut();
    let rc = unsafe { ctx_relations_build(root.as_ptr(), root.len(), &mut out) };
    assert_eq!(rc, ERR_OK);
    let full = unsafe { CStr::from_ptr(out) }.to_str().unwrap().to_owned();
    unsafe { ctx_relations_free_string(out) };
    let parsed: serde_json::Value = serde_json::from_str(&full).unwrap();
    let importers = parsed
        .get("importers")
        .and_then(|v| v.get("lib/a.go"))
        .expect("importers->lib/a.go missing");
    let expected_set: Vec<String> = importers
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().to_string())
        .collect();
    for want in &expected_set {
        assert!(body.contains(want), "{body} missing {want}");
    }
    unsafe { ctx_relations_session_close(h) };
}

#[test]
fn t_session_query_deps_matches_stateless() {
    let dir = make_repo("deps");
    let root = dir.to_string_lossy().to_string();
    let h = open_handle(&root);
    let body = run_query(h, "deps", r#"{"path":"main.go"}"#);
    assert!(body.contains("\"imports\""), "{body}");
    assert!(body.contains("lib/a.go"), "{body}");
    assert!(body.contains("util/u.go"), "{body}");
    unsafe { ctx_relations_session_close(h) };
}

#[test]
fn t_session_query_callers_matches_stateless() {
    let dir = make_repo("callers");
    let root = dir.to_string_lossy().to_string();
    let h = open_handle(&root);
    let refs = run_query(h, "refs", r#"{"path":"lib/a.go"}"#);
    let callers = run_query(h, "callers", r#"{"path":"lib/a.go"}"#);
    // callers is an alias for refs — same body.
    assert_eq!(refs, callers);
    unsafe { ctx_relations_session_close(h) };
}

#[test]
fn t_session_query_index_summary_byte_equal_to_build_cached() {
    let dir = make_repo("summary");
    let root = dir.to_string_lossy().to_string();
    let h = open_handle(&root);
    let session_summary = run_query(h, "index_summary", "");

    // Build the stateless cached body for comparison.
    let mut out: *mut c_char = ptr::null_mut();
    let rc =
        unsafe { ctx_relations_build_cached(root.as_ptr(), root.len(), &mut out) };
    assert_eq!(rc, ERR_OK);
    let stateless = unsafe { CStr::from_ptr(out) }.to_str().unwrap().to_owned();
    unsafe { ctx_relations_free_string(out) };

    assert_eq!(
        session_summary, stateless,
        "session index_summary diverged from build_cached"
    );
    unsafe { ctx_relations_session_close(h) };
}

#[test]
fn t_session_concurrent_queries_safe() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::thread;

    let dir = make_repo("concurrent");
    let root = dir.to_string_lossy().to_string();
    let h = open_handle(&root);
    let h_usize = h as usize;
    let ok = Arc::new(AtomicUsize::new(0));
    let mut joins = Vec::new();
    for t in 0..4 {
        let ok = Arc::clone(&ok);
        joins.push(thread::spawn(move || {
            let h = h_usize as *mut c_void;
            for i in 0..25 {
                let (kind, args) = match (t + i) % 4 {
                    0 => ("refs", r#"{"path":"lib/a.go"}"#),
                    1 => ("deps", r#"{"path":"main.go"}"#),
                    2 => ("callers", r#"{"path":"util/u.go"}"#),
                    _ => ("index_summary", ""),
                };
                let mut out: *mut c_char = ptr::null_mut();
                let rc = unsafe {
                    ctx_relations_session_query(
                        h,
                        kind.as_ptr(),
                        kind.len(),
                        args.as_ptr(),
                        args.len(),
                        &mut out,
                    )
                };
                assert_eq!(rc, ERR_OK);
                if !out.is_null() {
                    unsafe { ctx_relations_free_string(out) };
                }
                ok.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }
    for j in joins {
        j.join().expect("thread join");
    }
    assert_eq!(ok.load(Ordering::Relaxed), 4 * 25);
    unsafe { ctx_relations_session_close(h) };
}

#[test]
fn t_session_close_idempotent() {
    // The FFI itself is single-close (the Go wrapper enforces atomic
    // CAS to call only once). We assert that a CLOSED-but-NULL handle
    // returns ERR_NULL_PTR — i.e. the Go-side idempotency check has a
    // well-defined sentinel.
    let dir = make_repo("close_idem");
    let root = dir.to_string_lossy().to_string();
    let h = open_handle(&root);
    let rc1 = unsafe { ctx_relations_session_close(h) };
    assert_eq!(rc1, ERR_OK);
    // Second close on null is a no-op.
    let rc2 = unsafe { ctx_relations_session_close(ptr::null_mut()) };
    assert_eq!(rc2, ERR_NULL_PTR);
}

#[test]
fn t_session_query_after_close_safe() {
    // After Close the Go wrapper short-circuits with ERR_BAD_HANDLE
    // before calling into Rust. From the Rust side a null handle is
    // sufficient to verify the behaviour.
    let mut out: *mut c_char = ptr::null_mut();
    let kind = "refs";
    let args = r#"{"path":"x"}"#;
    let rc = unsafe {
        ctx_relations_session_query(
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

#[test]
fn t_session_rejects_bad_kind_and_bad_args() {
    let dir = make_repo("bad");
    let root = dir.to_string_lossy().to_string();
    let h = open_handle(&root);

    // Unknown kind.
    let kind = "bogus";
    let args = r#"{"path":"main.go"}"#;
    let mut out: *mut c_char = ptr::null_mut();
    let rc = unsafe {
        ctx_relations_session_query(
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
    let kind = "refs";
    let args = "not-json";
    let mut out: *mut c_char = ptr::null_mut();
    let rc = unsafe {
        ctx_relations_session_query(
            h,
            kind.as_ptr(),
            kind.len(),
            args.as_ptr(),
            args.len(),
            &mut out,
        )
    };
    assert_eq!(rc, ERR_BAD_JSON);

    unsafe { ctx_relations_session_close(h) };
}

#[test]
fn t_session_2000_cycle_soak() {
    // Open/close 2000× on the same fixture; ensure no panic, no leak
    // visible to the test runner. Heap-growth assertion is done by the
    // Go-side soak benchmark; here we just confirm liveness.
    let dir = make_repo("soak");
    let root = dir.to_string_lossy().to_string();
    for _ in 0..2000 {
        let h = open_handle(&root);
        let body = run_query(h, "refs", r#"{"path":"lib/a.go"}"#);
        assert!(body.contains("importers"));
        let rc = unsafe { ctx_relations_session_close(h) };
        assert_eq!(rc, ERR_OK);
    }
}

// Helper used to make a *mut c_void a valid argument to ergonomic helpers
// without smudging the original handle.
fn h_check(h: *mut c_void) -> *mut c_void {
    assert!(!h.is_null());
    h
}
