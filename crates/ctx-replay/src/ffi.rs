// crates/ctx-replay/src/ffi.rs
//
// Phase 3 FFI surface for ctx-replay. Mirrors the conventions of
// ctx-relations/ctx-scan: every input is BORROWED, every output is
// heap-owned (caller frees), every extern wraps `catch_unwind`.
//
// FUNCTION SURFACE
// ================
//   ctx_replay_diff(base_json, base_len, cur_json, cur_len,
//                   strict, out_result_ptr) -> i32
//     Decodes two Manifest JSON blobs, runs Compute, returns the
//     DiffSummary JSON.
//
//   ctx_replay_selection_diff(a_json, a_len, b_json, b_len,
//                             sort_by_ptr, sort_by_len,
//                             out_result_ptr) -> i32
//     Decodes two Manifest JSON blobs, computes the SelectionSummary,
//     applies SortSelectionDiff, returns JSON.
//
//   ctx_replay_parse_duration(s_ptr, s_len, out_nanos) -> i32
//     Parses `replay.ParseDuration` syntax. Returns nanoseconds.
//
//   ctx_replay_free_string(s)
//   ctx_replay_version() -> *const c_char

use std::ffi::{c_char, c_int, c_void, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::slice;

use crate::diff::{compute, compute_selection_diff, sort_selection_diff, DiffOptions};
use crate::prune::parse_duration;
use crate::session::{QueryError, ReplaySession};
use crate::types::Manifest;

const MAX_INPUT_BYTES: usize = 100 * 1024 * 1024;

const ERR_OK: c_int = 0;
const ERR_NULL_PTR: c_int = -1;
const ERR_TOO_LARGE: c_int = -2;
const ERR_BAD_JSON: c_int = -3;
const ERR_SERIALIZE: c_int = -4;
const ERR_PARSE: c_int = -5;
const ERR_BAD_HANDLE: c_int = -10;
const ERR_BAD_KIND: c_int = -11;
const ERR_NOT_FOUND: c_int = -12;
const ERR_IO: c_int = -13;
const ERR_PANIC: c_int = -99;

static VERSION_C: once_cell::sync::Lazy<CString> =
    once_cell::sync::Lazy::new(|| CString::new("ctx-replay 0.1.0").expect("version cstr"));

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

/// Diff two manifests and return DiffSummary JSON.
///
/// # Safety
/// See module-level docs.
#[no_mangle]
pub unsafe extern "C" fn ctx_replay_diff(
    base_ptr: *const u8,
    base_len: usize,
    cur_ptr: *const u8,
    cur_len: usize,
    strict: c_int,
    out_result_ptr: *mut *mut c_char,
) -> c_int {
    let result = catch_unwind(AssertUnwindSafe(|| {
        if out_result_ptr.is_null() {
            return ERR_NULL_PTR;
        }
        *out_result_ptr = ptr::null_mut();
        let base_bytes = match slice_from_raw(base_ptr, base_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let cur_bytes = match slice_from_raw(cur_ptr, cur_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let base: Manifest = match serde_json::from_slice(base_bytes) {
            Ok(m) => m,
            Err(_) => return ERR_BAD_JSON,
        };
        let cur: Manifest = match serde_json::from_slice(cur_bytes) {
            Ok(m) => m,
            Err(_) => return ERR_BAD_JSON,
        };
        let summary = compute(
            &base,
            &cur,
            DiffOptions {
                strict: strict != 0,
            },
        );
        let json = match serde_json::to_string(&summary) {
            Ok(s) => s,
            Err(_) => return ERR_SERIALIZE,
        };
        emit_cstring(json, out_result_ptr)
    }));
    result.unwrap_or(ERR_PANIC)
}

/// Compute the selection diff and return SelectionSummary JSON.
///
/// # Safety
/// See module-level docs.
#[no_mangle]
pub unsafe extern "C" fn ctx_replay_selection_diff(
    a_ptr: *const u8,
    a_len: usize,
    b_ptr: *const u8,
    b_len: usize,
    sort_by_ptr: *const u8,
    sort_by_len: usize,
    out_result_ptr: *mut *mut c_char,
) -> c_int {
    let result = catch_unwind(AssertUnwindSafe(|| {
        if out_result_ptr.is_null() {
            return ERR_NULL_PTR;
        }
        *out_result_ptr = ptr::null_mut();
        let a_bytes = match slice_from_raw(a_ptr, a_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let b_bytes = match slice_from_raw(b_ptr, b_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let sort_bytes = match slice_from_raw(sort_by_ptr, sort_by_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let sort_by = decode_utf8(sort_bytes).unwrap_or("tier");
        let a: Manifest = match serde_json::from_slice(a_bytes) {
            Ok(m) => m,
            Err(_) => return ERR_BAD_JSON,
        };
        let b: Manifest = match serde_json::from_slice(b_bytes) {
            Ok(m) => m,
            Err(_) => return ERR_BAD_JSON,
        };
        let mut sel = compute_selection_diff(&a, &b);
        sort_selection_diff(&mut sel, sort_by);
        let json = match serde_json::to_string(&sel) {
            Ok(s) => s,
            Err(_) => return ERR_SERIALIZE,
        };
        emit_cstring(json, out_result_ptr)
    }));
    result.unwrap_or(ERR_PANIC)
}

/// Parses a replay duration string and writes nanoseconds to `out_nanos`.
///
/// # Safety
/// See module-level docs.
#[no_mangle]
pub unsafe extern "C" fn ctx_replay_parse_duration(
    s_ptr: *const u8,
    s_len: usize,
    out_nanos: *mut i64,
) -> c_int {
    let result = catch_unwind(AssertUnwindSafe(|| {
        if out_nanos.is_null() {
            return ERR_NULL_PTR;
        }
        let bytes = match slice_from_raw(s_ptr, s_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let s = match decode_utf8(bytes) {
            Ok(s) => s,
            Err(e) => return e,
        };
        match parse_duration(s) {
            Ok(n) => {
                *out_nanos = n;
                ERR_OK
            }
            Err(_) => ERR_PARSE,
        }
    }));
    result.unwrap_or(ERR_PANIC)
}

/// Free a string previously returned from one of the `ctx_replay_*`
/// functions. Safe to call on null (no-op).
///
/// # Safety
/// See module-level docs.
#[no_mangle]
pub unsafe extern "C" fn ctx_replay_free_string(s: *mut c_char) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !s.is_null() {
            drop(CString::from_raw(s));
        }
    }));
}

/// Returns a pointer to a `'static` NUL-terminated C string carrying
/// the crate's version banner. The caller MUST NOT free it.
#[no_mangle]
pub extern "C" fn ctx_replay_version() -> *const c_char {
    VERSION_C.as_ptr()
}

// =================================================================
// ADR-002 sticky-handle session API for ctx-replay.
//
// The session caches the snapshot directory's store handle + a lazy
// per-id Manifest map. The Go caller opens ONCE per logical scope
// (web handler init keyed by snapshot_dir, replay-pack pre-pass for
// its single base id) and routes N queries through the handle:
//
//   - "list"             — chronological manifests, cached
//   - "load"             — single manifest by id, cached
//   - "diff"             — base-id × current-manifest-json
//   - "diff_ids"         — base-id × current-id (both cached)
//   - "selection_diff"   — a-id × b-id (both cached) + sort
//   - "prune_candidates" — read-only stale-snapshot probe
//
// Queries hit pre-decoded state instead of re-reading the snapshot
// directory and re-marshaling the Manifest across cgo per call.
//
// FUNCTION SURFACE
// ================
//   ctx_replay_session_open(dir_ptr, dir_len, opts_ptr, opts_len,
//                           out_handle) -> i32
//   ctx_replay_session_query(handle, kind_ptr, kind_len,
//                            args_ptr, args_len, out_result_ptr) -> i32
//   ctx_replay_session_close(handle) -> i32
//
// `opts_json` is currently reserved for forward compatibility (e.g.
// "shared" flag toggles). Empty input is accepted.
// =================================================================

/// Open a replay session against `dir`. On success the handle is owned
/// by the caller and must be released via `ctx_replay_session_close`.
///
/// # Safety
/// `out_handle` must be a valid, writable pointer to `*mut c_void`.
#[no_mangle]
pub unsafe extern "C" fn ctx_replay_session_open(
    dir_ptr: *const u8,
    dir_len: usize,
    _opts_ptr: *const u8,
    opts_len: usize,
    out_handle: *mut *mut c_void,
) -> c_int {
    let r = catch_unwind(AssertUnwindSafe(|| {
        if out_handle.is_null() {
            return ERR_NULL_PTR;
        }
        *out_handle = ptr::null_mut();

        if opts_len > MAX_INPUT_BYTES {
            return ERR_TOO_LARGE;
        }
        let dir_bytes = match slice_from_raw(dir_ptr, dir_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let dir = match decode_utf8(dir_bytes) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let session = match ReplaySession::open(dir) {
            Ok(s) => s,
            Err(_) => return ERR_IO,
        };
        let boxed = Box::new(session);
        *out_handle = Box::into_raw(boxed) as *mut c_void;
        ERR_OK
    }));
    r.unwrap_or(ERR_PANIC)
}

/// Run a kind-tagged query against the cached snapshot session.
///
/// # Safety
/// `handle` must have been returned by a prior successful call to
/// `ctx_replay_session_open` and must not have been passed to
/// `ctx_replay_session_close`.
#[no_mangle]
pub unsafe extern "C" fn ctx_replay_session_query(
    handle: *mut c_void,
    kind_ptr: *const u8,
    kind_len: usize,
    args_ptr: *const u8,
    args_len: usize,
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
        let kind_bytes = match slice_from_raw(kind_ptr, kind_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let args_bytes = if args_len == 0 {
            &[][..]
        } else {
            match slice_from_raw(args_ptr, args_len) {
                Ok(s) => s,
                Err(e) => return e,
            }
        };
        let kind = match decode_utf8(kind_bytes) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let args = match decode_utf8(args_bytes) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let session = &*(handle as *const ReplaySession);
        match session.query(kind, args) {
            Ok(body) => emit_cstring(body, out_result_ptr),
            Err(QueryError::UnknownKind(_)) => ERR_BAD_KIND,
            Err(QueryError::BadArgs) => ERR_BAD_JSON,
            Err(QueryError::BadArgsDetail(_)) => ERR_BAD_JSON,
            Err(QueryError::NotFound(_)) => ERR_NOT_FOUND,
            Err(QueryError::Io) => ERR_IO,
            Err(QueryError::Serialize) => ERR_SERIALIZE,
        }
    }));
    r.unwrap_or(ERR_PANIC)
}

/// Close the session and free Rust-side memory.
///
/// # Safety
/// `handle` must either be null (returns ERR_NULL_PTR) or a pointer
/// returned by `ctx_replay_session_open` that has not previously been
/// passed to this function. The caller MUST enforce single-close.
#[no_mangle]
pub unsafe extern "C" fn ctx_replay_session_close(handle: *mut c_void) -> c_int {
    let r = catch_unwind(AssertUnwindSafe(|| {
        if handle.is_null() {
            return ERR_NULL_PTR;
        }
        drop(Box::from_raw(handle as *mut ReplaySession));
        ERR_OK
    }));
    r.unwrap_or(ERR_PANIC)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;

    fn cstr_into_string(p: *mut c_char) -> String {
        assert!(!p.is_null());
        let s = unsafe { CStr::from_ptr(p) }
            .to_str()
            .expect("utf-8")
            .to_owned();
        unsafe { ctx_replay_free_string(p) };
        s
    }

    #[test]
    fn version_round_trips() {
        let p = ctx_replay_version();
        assert!(!p.is_null());
        let s = unsafe { CStr::from_ptr(p) }.to_str().unwrap();
        assert_eq!(s, "ctx-replay 0.1.0");
    }

    #[test]
    fn parse_duration_via_ffi() {
        let s = "5m";
        let mut out: i64 = 0;
        let rc = unsafe {
            ctx_replay_parse_duration(s.as_ptr(), s.len(), &mut out as *mut i64)
        };
        assert_eq!(rc, ERR_OK);
        assert_eq!(out, 5 * 60 * 1_000_000_000);
    }

    #[test]
    fn diff_returns_json() {
        let base = r#"{"schema_version":1,"id":"a","created_at":"2026-01-01T00:00:00Z","ctx_version":"dev","budget":0,"used":0,"root":"","format":"","entries":[{"path":"x","sha256":"aa","tokens":10,"relevance":"High","score":0}]}"#;
        let cur = r#"{"schema_version":1,"id":"b","created_at":"2026-01-02T00:00:00Z","ctx_version":"dev","budget":0,"used":0,"root":"","format":"","entries":[{"path":"x","sha256":"bb","tokens":15,"relevance":"High","score":0}]}"#;
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe {
            ctx_replay_diff(
                base.as_ptr(),
                base.len(),
                cur.as_ptr(),
                cur.len(),
                0,
                &mut out,
            )
        };
        assert_eq!(rc, ERR_OK);
        let json = cstr_into_string(out);
        assert!(json.contains("\"modified\":1"), "{json}");
    }
}
