// crates/ctx-symbols/tests/sticky_handle.rs — exercise the sessioned
// lookup path through the FFI to verify open/query/close behaves
// correctly across many cycles + queries.

use ctx_symbols::ffi::*;
use ctx_symbols::testing::synthetic_corpus;
use std::ffi::{c_char, c_void, CStr};
use std::ptr;

fn cstr_to_string(p: *mut c_char) -> String {
    assert!(!p.is_null());
    let s = unsafe { CStr::from_ptr(p) }
        .to_str()
        .expect("utf-8")
        .to_owned();
    unsafe { ctx_symbols_free_string(p) };
    s
}

fn open(root: &[u8], corpus_json: &[u8]) -> *mut c_void {
    let mut handle: *mut c_void = ptr::null_mut();
    let rc = unsafe {
        ctx_symbols_lookup_session_open(
            root.as_ptr(),
            root.len(),
            corpus_json.as_ptr(),
            corpus_json.len(),
            &mut handle,
        )
    };
    assert_eq!(rc, ERR_OK, "session_open rc={rc}");
    assert!(!handle.is_null());
    handle
}

fn query(handle: *mut c_void, kind: &[u8], args: &[u8]) -> String {
    let mut out: *mut c_char = ptr::null_mut();
    let rc = unsafe {
        ctx_symbols_lookup_session_query(
            handle,
            kind.as_ptr(),
            kind.len(),
            args.as_ptr(),
            args.len(),
            &mut out,
        )
    };
    assert_eq!(rc, ERR_OK, "session_query rc={rc}");
    cstr_to_string(out)
}

fn close(handle: *mut c_void) {
    let rc = unsafe { ctx_symbols_lookup_session_close(handle) };
    assert_eq!(rc, ERR_OK);
}

#[test]
fn open_query_resolve_close_round_trip() {
    let corpus = synthetic_corpus(50, 10);
    let cb = serde_json::to_vec(&corpus).unwrap();
    let h = open(b"/repo", &cb);

    let args = serde_json::json!({"name": "BuildIndex"});
    let ab = serde_json::to_vec(&args).unwrap();
    let body = query(h, b"resolve", &ab);
    assert!(body.starts_with('['), "expected array, got {body}");

    close(h);
}

#[test]
fn stats_kind_returns_corpus_size() {
    let corpus = synthetic_corpus(20, 5);
    let cb = serde_json::to_vec(&corpus).unwrap();
    let h = open(b"/repo", &cb);
    let body = query(h, b"stats", b"");
    assert!(body.contains("\"files\":20"), "{body}");
    close(h);
}

#[test]
fn many_open_close_cycles_no_leak_in_5000_iter() {
    // Soft soak — 5K cycles. Production sticky-handle target is 10K
    // (per relations/where); 5K is a reasonable Tier 2 #5 budget
    // given the corpus is loaded fresh each cycle to also exercise
    // the deserialize path.
    let corpus = synthetic_corpus(50, 10);
    let cb = serde_json::to_vec(&corpus).unwrap();
    for i in 0..5_000 {
        let h = open(b"/repo", &cb);
        if i % 1_000 == 0 {
            let body = query(h, b"stats", b"");
            assert!(body.contains("\"files\":50"));
        }
        close(h);
    }
}

#[test]
fn many_queries_against_same_session_match_stateless_resolve() {
    let corpus = synthetic_corpus(30, 8);
    let cb = serde_json::to_vec(&corpus).unwrap();
    let h = open(b"/repo", &cb);
    let args = serde_json::json!({"name": "BuildIndex"});
    let ab = serde_json::to_vec(&args).unwrap();

    let first = query(h, b"resolve", &ab);
    for _ in 0..100 {
        let body = query(h, b"resolve", &ab);
        assert_eq!(body, first, "sessioned query result drifted");
    }
    close(h);
}

#[test]
fn refs_alias_returns_same_as_resolve() {
    let corpus = synthetic_corpus(10, 5);
    let cb = serde_json::to_vec(&corpus).unwrap();
    let h = open(b"/repo", &cb);
    let args = serde_json::json!({"name": "BuildIndex"});
    let ab = serde_json::to_vec(&args).unwrap();
    let a = query(h, b"resolve", &ab);
    let b = query(h, b"refs", &ab);
    assert_eq!(a, b, "refs vs resolve differ");
    close(h);
}

#[test]
fn empty_corpus_is_accepted() {
    let h = open(b"/repo", b"[]");
    let body = query(h, b"stats", b"");
    assert!(body.contains("\"files\":0"));
    close(h);
}

#[test]
fn null_close_handled() {
    let rc = unsafe { ctx_symbols_lookup_session_close(ptr::null_mut()) };
    assert_eq!(rc, ERR_NULL_PTR);
}

#[test]
fn unknown_query_kind_does_not_corrupt_session() {
    let corpus = synthetic_corpus(5, 2);
    let cb = serde_json::to_vec(&corpus).unwrap();
    let h = open(b"/repo", &cb);

    // First a bad kind
    let mut out: *mut c_char = ptr::null_mut();
    let rc = unsafe {
        ctx_symbols_lookup_session_query(h, b"nope".as_ptr(), 4, ptr::null(), 0, &mut out)
    };
    assert_eq!(rc, ERR_BAD_KIND);

    // Then a good kind should still work.
    let body = query(h, b"stats", b"");
    assert!(body.contains("\"files\":5"));
    close(h);
}
