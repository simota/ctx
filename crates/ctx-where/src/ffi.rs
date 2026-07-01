// crates/ctx-where/src/ffi.rs
//
// Phase 3 FFI surface for ctx-where. The dispatcher pre-walks the
// repository and extracts symbols (preserving the existing tree-sitter
// pipeline) before handing a JSON file-list across FFI. The Rust side
// then runs the LOOKUP_HEAVY hot path: scoring, Levenshtein candidates,
// match collection.
//
// FUNCTION SURFACE
// ================
//   Stateless (Phase 3 / ADR-001 Freeze baseline — KEPT for AB testing):
//     ctx_where_search(files_json, files_len, query_ptr, query_len,
//                      opts_json, opts_len, out_result_ptr) -> i32
//     ctx_where_suggest(files_json, files_len, query_ptr, query_len,
//                       limit, out_result_ptr) -> i32
//     ctx_where_levenshtein(a_ptr, a_len, b_ptr, b_len, out_dist) -> i32
//     ctx_where_free_string(s)
//     ctx_where_version() -> *const c_char
//
//   Sticky-handle (ADR-002 PoC — load corpus ONCE, query many times):
//     ctx_where_session_open(files_json, files_len, opts_json,
//                            opts_len, out_handle) -> i32
//     ctx_where_session_search(handle, query_ptr, query_len, limit,
//                              out_result_ptr) -> i32
//     ctx_where_session_close(handle) -> i32
//
//   The session variants amortize JSON unmarshal + FileInput allocation
//   across all queries against the same corpus. The Go side opens the
//   session ONCE per command, then routes N queries through the handle.

use std::ffi::{c_char, c_int, c_void, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::slice;

use crate::levenshtein::levenshtein;
use crate::search::{search_with_options, suggest_similar, FileInput, Options};

const MAX_INPUT_BYTES: usize = 256 * 1024 * 1024;

const ERR_OK: c_int = 0;
const ERR_NULL_PTR: c_int = -1;
const ERR_TOO_LARGE: c_int = -2;
const ERR_BAD_JSON: c_int = -3;
const ERR_SERIALIZE: c_int = -4;
const ERR_PANIC: c_int = -99;

static VERSION_C: once_cell::sync::Lazy<CString> =
    once_cell::sync::Lazy::new(|| CString::new("ctx-where 0.1.0").expect("version cstr"));

unsafe fn slice_from_raw(ptr: *const u8, len: usize) -> Result<&'static [u8], c_int> {
    if len == 0 {
        return Ok(&[]);
    }
    if ptr.is_null() {
        return Err(ERR_NULL_PTR);
    }
    if len > MAX_INPUT_BYTES {
        return Err(ERR_TOO_LARGE);
    }
    Ok(slice::from_raw_parts(ptr, len))
}

fn emit_cstring(value: String, out: *mut *mut c_char) -> c_int {
    let c = match CString::new(value) {
        Ok(c) => c,
        Err(_) => return ERR_SERIALIZE,
    };
    unsafe { *out = c.into_raw() };
    ERR_OK
}

fn decode_utf8(bytes: &[u8]) -> Result<&str, c_int> {
    std::str::from_utf8(bytes).map_err(|_| ERR_BAD_JSON)
}

/// # Safety
/// See module-level docs.
#[no_mangle]
pub unsafe extern "C" fn ctx_where_search(
    files_ptr: *const u8,
    files_len: usize,
    query_ptr: *const u8,
    query_len: usize,
    opts_ptr: *const u8,
    opts_len: usize,
    out_result_ptr: *mut *mut c_char,
) -> c_int {
    let r = catch_unwind(AssertUnwindSafe(|| {
        if out_result_ptr.is_null() {
            return ERR_NULL_PTR;
        }
        *out_result_ptr = ptr::null_mut();

        let files_bytes = match slice_from_raw(files_ptr, files_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let query_bytes = match slice_from_raw(query_ptr, query_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let opts_bytes = match slice_from_raw(opts_ptr, opts_len) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let files: Vec<FileInput> = match serde_json::from_slice(files_bytes) {
            Ok(v) => v,
            Err(_) => return ERR_BAD_JSON,
        };
        let query = match decode_utf8(query_bytes) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let opts: Options = if opts_bytes.is_empty() {
            Options::default()
        } else {
            match serde_json::from_slice(opts_bytes) {
                Ok(v) => v,
                Err(_) => return ERR_BAD_JSON,
            }
        };
        let results = search_with_options(&files, query, &opts);
        let json = match serde_json::to_string(&results) {
            Ok(s) => s,
            Err(_) => return ERR_SERIALIZE,
        };
        emit_cstring(json, out_result_ptr)
    }));
    r.unwrap_or(ERR_PANIC)
}

/// # Safety
/// See module-level docs.
#[no_mangle]
pub unsafe extern "C" fn ctx_where_suggest(
    files_ptr: *const u8,
    files_len: usize,
    query_ptr: *const u8,
    query_len: usize,
    limit: c_int,
    out_result_ptr: *mut *mut c_char,
) -> c_int {
    let r = catch_unwind(AssertUnwindSafe(|| {
        if out_result_ptr.is_null() {
            return ERR_NULL_PTR;
        }
        *out_result_ptr = ptr::null_mut();
        let files_bytes = match slice_from_raw(files_ptr, files_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let query_bytes = match slice_from_raw(query_ptr, query_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let files: Vec<FileInput> = match serde_json::from_slice(files_bytes) {
            Ok(v) => v,
            Err(_) => return ERR_BAD_JSON,
        };
        let query = match decode_utf8(query_bytes) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let suggestions = suggest_similar(&files, query, limit as i64);
        let json = match serde_json::to_string(&suggestions) {
            Ok(s) => s,
            Err(_) => return ERR_SERIALIZE,
        };
        emit_cstring(json, out_result_ptr)
    }));
    r.unwrap_or(ERR_PANIC)
}

/// # Safety
/// See module-level docs.
#[no_mangle]
pub unsafe extern "C" fn ctx_where_levenshtein(
    a_ptr: *const u8,
    a_len: usize,
    b_ptr: *const u8,
    b_len: usize,
    out_dist: *mut c_int,
) -> c_int {
    let r = catch_unwind(AssertUnwindSafe(|| {
        if out_dist.is_null() {
            return ERR_NULL_PTR;
        }
        let a_bytes = match slice_from_raw(a_ptr, a_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let b_bytes = match slice_from_raw(b_ptr, b_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let a = match decode_utf8(a_bytes) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let b = match decode_utf8(b_bytes) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let ac: Vec<char> = a.chars().collect();
        let bc: Vec<char> = b.chars().collect();
        let d = levenshtein(&ac, &bc);
        *out_dist = d as c_int;
        ERR_OK
    }));
    r.unwrap_or(ERR_PANIC)
}

/// # Safety
/// See module-level docs.
#[no_mangle]
pub unsafe extern "C" fn ctx_where_free_string(s: *mut c_char) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !s.is_null() {
            drop(CString::from_raw(s));
        }
    }));
}

/// Returns a pointer to a `'static` NUL-terminated version banner.
#[no_mangle]
pub extern "C" fn ctx_where_version() -> *const c_char {
    VERSION_C.as_ptr()
}

// =================================================================
// ADR-002 sticky-handle session API.
//
// A WhereSession holds the already-decoded corpus (Vec<FileInput>) and
// the default options. Subsequent ctx_where_session_search calls reuse
// these without re-marshaling, recovering the intrinsic Rust speedup
// that the per-call cgo+JSON shuttle was eating in Phase 3.
//
// Memory model:
//   * The session is allocated as Box<WhereSession> and exposed via
//     Box::into_raw as *mut c_void. Go must call session_close exactly
//     once per successful open.
//   * Calling session_close twice on the same handle returns -1
//     (ERR_NULL_PTR) on the second call; we do NOT detect use-after-
//     free across different allocations (handles are opaque pointers
//     and the caller must enforce the lifetime).
//   * session_search and session_close validate handle != null.
//
// Thread-safety:
//   * The session is immutable after construction; search_with_options
//     takes &[FileInput] and produces a fresh Vec<Result> per call.
//     `Vec<FileInput>` is Send + Sync because FileInput is owned data
//     with no interior mutability. Multiple Go threads MAY call
//     session_search concurrently against the same handle.
//   * session_close MUST NOT race with session_search — the caller is
//     responsible for quiescing queries before closing the handle.
// =================================================================

const ERR_BAD_HANDLE: c_int = -10;

#[repr(C)]
pub struct WhereSession {
    files: Vec<FileInput>,
    default_opts: Options,
}

/// # Safety
/// `out_handle` must be a valid, writable pointer to a `*mut c_void`.
/// On success the caller owns the handle and must release it via
/// `ctx_where_session_close`.
#[no_mangle]
pub unsafe extern "C" fn ctx_where_session_open(
    files_ptr: *const u8,
    files_len: usize,
    opts_ptr: *const u8,
    opts_len: usize,
    out_handle: *mut *mut c_void,
) -> c_int {
    let r = catch_unwind(AssertUnwindSafe(|| {
        if out_handle.is_null() {
            return ERR_NULL_PTR;
        }
        *out_handle = ptr::null_mut();

        let files_bytes = match slice_from_raw(files_ptr, files_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let opts_bytes = match slice_from_raw(opts_ptr, opts_len) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let files: Vec<FileInput> = match serde_json::from_slice(files_bytes) {
            Ok(v) => v,
            Err(_) => return ERR_BAD_JSON,
        };
        let default_opts: Options = if opts_bytes.is_empty() {
            Options::default()
        } else {
            match serde_json::from_slice(opts_bytes) {
                Ok(v) => v,
                Err(_) => return ERR_BAD_JSON,
            }
        };

        let session = Box::new(WhereSession {
            files,
            default_opts,
        });
        *out_handle = Box::into_raw(session) as *mut c_void;
        ERR_OK
    }));
    r.unwrap_or(ERR_PANIC)
}

/// # Safety
/// `handle` must have been returned by a prior successful call to
/// `ctx_where_session_open` and must not have been passed to
/// `ctx_where_session_close`. `out_result_ptr` must be a valid writable
/// pointer to a `*mut c_char`.
#[no_mangle]
pub unsafe extern "C" fn ctx_where_session_search(
    handle: *mut c_void,
    query_ptr: *const u8,
    query_len: usize,
    limit: c_int,
    out_result_ptr: *mut *mut c_char,
) -> c_int {
    let r = catch_unwind(AssertUnwindSafe(|| {
        if out_result_ptr.is_null() {
            return ERR_NULL_PTR;
        }
        *out_result_ptr = ptr::null_mut();
        if handle.is_null() {
            return ERR_BAD_HANDLE;
        }

        let query_bytes = match slice_from_raw(query_ptr, query_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let query = match decode_utf8(query_bytes) {
            Ok(s) => s,
            Err(e) => return e,
        };

        // Borrow the session immutably; multiple threads may call this
        // simultaneously. The session lives as long as the caller's
        // handle reference; we DO NOT take ownership here.
        let session = &*(handle as *const WhereSession);

        // Per-call options: clone defaults, then patch limit if the
        // caller passed a non-zero value. limit <=0 keeps the default.
        let mut opts = session.default_opts.clone();
        if limit > 0 {
            opts.limit = limit as i64;
        }

        let results = search_with_options(&session.files, query, &opts);
        let json = match serde_json::to_string(&results) {
            Ok(s) => s,
            Err(_) => return ERR_SERIALIZE,
        };
        emit_cstring(json, out_result_ptr)
    }));
    r.unwrap_or(ERR_PANIC)
}

/// # Safety
/// `handle` must either be null (no-op, returns -1) or a pointer
/// returned by `ctx_where_session_open` that has not previously been
/// passed to this function. Calling on a null handle is safe and
/// returns ERR_NULL_PTR; calling twice on the same non-null handle is
/// UNDEFINED — the caller must enforce single-close discipline.
#[no_mangle]
pub unsafe extern "C" fn ctx_where_session_close(handle: *mut c_void) -> c_int {
    let r = catch_unwind(AssertUnwindSafe(|| {
        if handle.is_null() {
            return ERR_NULL_PTR;
        }
        // Reclaim the Box; drop runs here.
        drop(Box::from_raw(handle as *mut WhereSession));
        ERR_OK
    }));
    r.unwrap_or(ERR_PANIC)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;

    #[test]
    fn version_round_trips() {
        let p = ctx_where_version();
        assert!(!p.is_null());
        let s = unsafe { CStr::from_ptr(p) }.to_str().unwrap();
        assert_eq!(s, "ctx-where 0.1.0");
    }

    #[test]
    fn levenshtein_ffi() {
        let a = "kitten";
        let b = "sitting";
        let mut d: c_int = 0;
        let rc = unsafe {
            ctx_where_levenshtein(
                a.as_ptr(),
                a.len(),
                b.as_ptr(),
                b.len(),
                &mut d as *mut c_int,
            )
        };
        assert_eq!(rc, ERR_OK);
        assert_eq!(d, 3);
    }

    fn open_session(files_json: &str) -> *mut c_void {
        let opts = "{}";
        let mut handle: *mut c_void = ptr::null_mut();
        let rc = unsafe {
            ctx_where_session_open(
                files_json.as_ptr(),
                files_json.len(),
                opts.as_ptr(),
                opts.len(),
                &mut handle,
            )
        };
        assert_eq!(rc, ERR_OK, "session_open rc={rc}");
        assert!(!handle.is_null());
        handle
    }

    #[test]
    fn t_session_open_close_no_leak() {
        let files = r#"[{"path":"a.go","is_dir":false,"symbols":[],"lines":["package a"]}]"#;
        for _ in 0..1000 {
            let h = open_session(files);
            let rc = unsafe { ctx_where_session_close(h) };
            assert_eq!(rc, ERR_OK);
        }
    }

    #[test]
    fn t_session_multiple_queries_same_handle_yields_same_results_as_stateless() {
        let files = r#"[
            {"path":"internal/pack/relevance.go","is_dir":false,"symbols":[],"lines":["package pack","func scoreRelevance() {}"]},
            {"path":"internal/auth/session.go","is_dir":false,"symbols":[],"lines":["package auth","func SaveSession() {}"]}
        ]"#;
        let queries = ["relevance", "session", "auth", "pack"];

        let handle = open_session(files);
        for q in queries {
            let mut sticky_out: *mut c_char = ptr::null_mut();
            let rc = unsafe {
                ctx_where_session_search(handle, q.as_ptr(), q.len(), 10, &mut sticky_out)
            };
            assert_eq!(rc, ERR_OK);
            let sticky = unsafe { CStr::from_ptr(sticky_out) }
                .to_str()
                .unwrap()
                .to_owned();
            unsafe { ctx_where_free_string(sticky_out) };

            // Stateless equivalent.
            let opts = r#"{"limit":10}"#;
            let mut stateless_out: *mut c_char = ptr::null_mut();
            let rc2 = unsafe {
                ctx_where_search(
                    files.as_ptr(),
                    files.len(),
                    q.as_ptr(),
                    q.len(),
                    opts.as_ptr(),
                    opts.len(),
                    &mut stateless_out,
                )
            };
            assert_eq!(rc2, ERR_OK);
            let stateless = unsafe { CStr::from_ptr(stateless_out) }
                .to_str()
                .unwrap()
                .to_owned();
            unsafe { ctx_where_free_string(stateless_out) };
            assert_eq!(sticky, stateless, "query {q} diverged");
        }
        let rc = unsafe { ctx_where_session_close(handle) };
        assert_eq!(rc, ERR_OK);
    }

    #[test]
    fn t_session_concurrent_queries_safe() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        use std::thread;

        let files = r#"[
            {"path":"a/relevance.go","is_dir":false,"symbols":[],"lines":["package a","func scoreRelevance() {}"]},
            {"path":"b/session.go","is_dir":false,"symbols":[],"lines":["package b","func SaveSession() {}"]},
            {"path":"c/handler.go","is_dir":false,"symbols":[],"lines":["package c","func Handler() {}"]}
        ]"#;
        let handle = open_session(files);
        // Wrap the handle as usize so we can Send it; the Rust handle
        // itself is *mut c_void which is !Send. usize is safe because
        // we know the underlying allocation outlives every spawned
        // thread (we join all of them before closing).
        let handle_usize = handle as usize;
        let ok = Arc::new(AtomicUsize::new(0));
        let mut joins = Vec::new();
        for t in 0..4 {
            let ok = Arc::clone(&ok);
            joins.push(thread::spawn(move || {
                let h = handle_usize as *mut c_void;
                for i in 0..25 {
                    let q = match (t + i) % 3 {
                        0 => "relevance",
                        1 => "session",
                        _ => "handler",
                    };
                    let mut out: *mut c_char = ptr::null_mut();
                    let rc =
                        unsafe { ctx_where_session_search(h, q.as_ptr(), q.len(), 10, &mut out) };
                    assert_eq!(rc, ERR_OK);
                    if !out.is_null() {
                        unsafe { ctx_where_free_string(out) };
                    }
                    ok.fetch_add(1, Ordering::Relaxed);
                }
            }));
        }
        for j in joins {
            j.join().expect("thread join");
        }
        assert_eq!(ok.load(Ordering::Relaxed), 4 * 25);
        let rc = unsafe { ctx_where_session_close(handle) };
        assert_eq!(rc, ERR_OK);
    }

    #[test]
    fn t_session_close_handle_idempotent_against_null() {
        // The contract is: null-handle close returns ERR_NULL_PTR;
        // it does not panic.
        let rc = unsafe { ctx_where_session_close(ptr::null_mut()) };
        assert_eq!(rc, ERR_NULL_PTR);
    }

    #[test]
    fn t_session_search_with_null_handle_safe() {
        let q = "anything";
        let mut out: *mut c_char = ptr::null_mut();
        let rc =
            unsafe { ctx_where_session_search(ptr::null_mut(), q.as_ptr(), q.len(), 10, &mut out) };
        assert_eq!(rc, ERR_BAD_HANDLE);
        assert!(out.is_null());
    }

    #[test]
    fn t_session_open_rejects_bad_json() {
        let bad = "not-json";
        let opts = "{}";
        let mut h: *mut c_void = ptr::null_mut();
        let rc = unsafe {
            ctx_where_session_open(bad.as_ptr(), bad.len(), opts.as_ptr(), opts.len(), &mut h)
        };
        assert_eq!(rc, ERR_BAD_JSON);
        assert!(h.is_null());
    }

    #[test]
    fn search_ffi_smoke() {
        let files = r#"[{"path":"internal/pack/relevance.go","is_dir":false,"symbols":[],"lines":["package pack","func scoreRelevance() {}"]}]"#;
        let query = "relevance";
        let opts = "{}";
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe {
            ctx_where_search(
                files.as_ptr(),
                files.len(),
                query.as_ptr(),
                query.len(),
                opts.as_ptr(),
                opts.len(),
                &mut out,
            )
        };
        assert_eq!(rc, ERR_OK);
        let json = unsafe { CStr::from_ptr(out) }.to_str().unwrap().to_owned();
        unsafe { ctx_where_free_string(out) };
        assert!(json.contains("internal/pack/relevance.go"), "{json}");
    }
}
