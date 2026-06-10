// crates/ctx-focus/src/ffi.rs
//
// Phase 4 FFI surface for ctx-focus. The dispatcher pre-walks the
// repository and extracts symbols (preserving the existing tree-sitter
// pipeline) before handing a JSON file-list across FFI. The Rust side
// then runs the LOOKUP_HEAVY hot path: ResolveAnchor + Expand.
//
// FUNCTION SURFACE
// ================
//   Sticky-handle (ADR-002 — PRIMARY; load corpus ONCE, query many):
//     ctx_focus_session_open(files_json, files_len, opts_json,
//                            opts_len, out_handle) -> i32
//     ctx_focus_session_resolve(handle, anchor_ptr, anchor_len,
//                               out_result_ptr) -> i32
//     ctx_focus_session_expand(handle, anchor_ptr, anchor_len, hops,
//                              out_result_ptr) -> i32
//     ctx_focus_session_pack(handle, anchor_ptr, anchor_len, hops,
//                            out_result_ptr) -> i32
//     ctx_focus_session_close(handle) -> i32
//
//   Stateless (SECONDARY — for callers that don't need a session):
//     ctx_focus_pack(files_json, files_len, anchor_ptr, anchor_len,
//                    hops, out_result_ptr) -> i32
//
//   Plumbing:
//     ctx_focus_free_string(s)
//     ctx_focus_version() -> *const c_char
//
// JSON SHAPES
// ===========
//   * Resolve success: serialized Anchor struct (uppercase tags match Go)
//   * Resolve ambiguity: {"ambiguous": true, "anchor": "...", "candidates": [...]}
//   * Resolve not-found: {"error": "anchor not found", "anchor": "..."}
//   * Expand: serialized [FileInfo]
//   * Pack: serialized PackResult OR the same error envelope as resolve.

use std::ffi::{c_char, c_int, c_void, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::slice;

use crate::expand::expand;
use crate::pack::pack;
use crate::resolve::resolve_anchor;
use crate::types::{ExpandOptions, FileInput};

const MAX_INPUT_BYTES: usize = 256 * 1024 * 1024;

const ERR_OK: c_int = 0;
const ERR_NULL_PTR: c_int = -1;
const ERR_TOO_LARGE: c_int = -2;
const ERR_BAD_JSON: c_int = -3;
const ERR_SERIALIZE: c_int = -4;
const ERR_BAD_HANDLE: c_int = -10;
const ERR_PANIC: c_int = -99;

static VERSION_C: once_cell::sync::Lazy<CString> =
    once_cell::sync::Lazy::new(|| CString::new("ctx-focus 0.1.0").expect("version cstr"));

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

/// Wrap a resolve_anchor outcome into the agreed JSON envelope.
fn resolve_envelope(anchor: &str, files: &[FileInput]) -> String {
    match resolve_anchor(files, anchor) {
        Ok(a) => serde_json::to_string(&a).unwrap_or_else(|_| String::from("{}")),
        Err(err) => {
            if err.candidates.is_empty() {
                let body = serde_json::json!({
                    "error": "anchor not found",
                    "anchor": err.anchor,
                });
                body.to_string()
            } else {
                let body = serde_json::json!({
                    "ambiguous": true,
                    "anchor": err.anchor,
                    "candidates": err.candidates,
                });
                body.to_string()
            }
        }
    }
}

fn expand_envelope(anchor: &str, hops: i64, files: &[FileInput]) -> String {
    match resolve_anchor(files, anchor) {
        Ok(a) => {
            let r = expand(files, &a, &ExpandOptions { hops });
            serde_json::to_string(&r).unwrap_or_else(|_| String::from("[]"))
        }
        Err(err) => {
            if err.candidates.is_empty() {
                serde_json::json!({"error": "anchor not found", "anchor": err.anchor})
                    .to_string()
            } else {
                serde_json::json!({
                    "ambiguous": true,
                    "anchor": err.anchor,
                    "candidates": err.candidates,
                })
                .to_string()
            }
        }
    }
}

fn pack_envelope(anchor: &str, hops: i64, files: &[FileInput]) -> String {
    match pack(files, anchor, &ExpandOptions { hops }) {
        Ok(r) => serde_json::to_string(&r).unwrap_or_else(|_| String::from("{}")),
        Err(err) => {
            if err.candidates.is_empty() {
                serde_json::json!({"error": "anchor not found", "anchor": err.anchor})
                    .to_string()
            } else {
                serde_json::json!({
                    "ambiguous": true,
                    "anchor": err.anchor,
                    "candidates": err.candidates,
                })
                .to_string()
            }
        }
    }
}

// =================================================================
// Stateless secondary API. Documented as the SLOW path: callers should
// prefer the session API for any workload that touches the same corpus
// more than once.
// =================================================================

/// # Safety
/// See module-level docs.
#[no_mangle]
pub unsafe extern "C" fn ctx_focus_pack(
    files_ptr: *const u8,
    files_len: usize,
    anchor_ptr: *const u8,
    anchor_len: usize,
    hops: c_int,
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
        let anchor_bytes = match slice_from_raw(anchor_ptr, anchor_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let files: Vec<FileInput> = match serde_json::from_slice(files_bytes) {
            Ok(v) => v,
            Err(_) => return ERR_BAD_JSON,
        };
        let anchor = match decode_utf8(anchor_bytes) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let body = pack_envelope(anchor, hops as i64, &files);
        emit_cstring(body, out_result_ptr)
    }));
    r.unwrap_or(ERR_PANIC)
}

/// # Safety
/// See module-level docs.
#[no_mangle]
pub unsafe extern "C" fn ctx_focus_free_string(s: *mut c_char) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !s.is_null() {
            drop(CString::from_raw(s));
        }
    }));
}

/// Returns a pointer to a `'static` NUL-terminated version banner.
#[no_mangle]
pub extern "C" fn ctx_focus_version() -> *const c_char {
    VERSION_C.as_ptr()
}

// =================================================================
// ADR-002 sticky-handle session API.
//
// FocusSession holds the already-decoded corpus (Vec<FileInput>) so
// subsequent resolve / expand / pack calls amortise away the JSON
// unmarshal cost. The Go side opens the session ONCE per command, then
// routes N queries through the handle.
// =================================================================

#[repr(C)]
pub struct FocusSession {
    files: Vec<FileInput>,
    default_hops: i64,
}

/// # Safety
/// `out_handle` must be a valid, writable pointer to a `*mut c_void`.
/// On success the caller owns the handle and must release it via
/// `ctx_focus_session_close`.
#[no_mangle]
pub unsafe extern "C" fn ctx_focus_session_open(
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
        let default_opts: ExpandOptions = if opts_bytes.is_empty() {
            ExpandOptions::default()
        } else {
            match serde_json::from_slice(opts_bytes) {
                Ok(v) => v,
                Err(_) => return ERR_BAD_JSON,
            }
        };

        let session = Box::new(FocusSession {
            files,
            default_hops: default_opts.hops,
        });
        *out_handle = Box::into_raw(session) as *mut c_void;
        ERR_OK
    }));
    r.unwrap_or(ERR_PANIC)
}

/// # Safety
/// `handle` must have been returned by a prior successful call to
/// `ctx_focus_session_open` and must not have been passed to
/// `ctx_focus_session_close`.
#[no_mangle]
pub unsafe extern "C" fn ctx_focus_session_resolve(
    handle: *mut c_void,
    anchor_ptr: *const u8,
    anchor_len: usize,
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
        let anchor_bytes = match slice_from_raw(anchor_ptr, anchor_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let anchor = match decode_utf8(anchor_bytes) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let session = &*(handle as *const FocusSession);
        let body = resolve_envelope(anchor, &session.files);
        emit_cstring(body, out_result_ptr)
    }));
    r.unwrap_or(ERR_PANIC)
}

/// # Safety
/// `handle` must have been returned by a prior successful call to
/// `ctx_focus_session_open` and must not have been passed to
/// `ctx_focus_session_close`.
#[no_mangle]
pub unsafe extern "C" fn ctx_focus_session_expand(
    handle: *mut c_void,
    anchor_ptr: *const u8,
    anchor_len: usize,
    hops: c_int,
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
        let anchor_bytes = match slice_from_raw(anchor_ptr, anchor_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let anchor = match decode_utf8(anchor_bytes) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let session = &*(handle as *const FocusSession);
        let effective_hops = if hops > 0 { hops as i64 } else { session.default_hops };
        let body = expand_envelope(anchor, effective_hops, &session.files);
        emit_cstring(body, out_result_ptr)
    }));
    r.unwrap_or(ERR_PANIC)
}

/// # Safety
/// See ctx_focus_session_expand.
#[no_mangle]
pub unsafe extern "C" fn ctx_focus_session_pack(
    handle: *mut c_void,
    anchor_ptr: *const u8,
    anchor_len: usize,
    hops: c_int,
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
        let anchor_bytes = match slice_from_raw(anchor_ptr, anchor_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let anchor = match decode_utf8(anchor_bytes) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let session = &*(handle as *const FocusSession);
        let effective_hops = if hops > 0 { hops as i64 } else { session.default_hops };
        let body = pack_envelope(anchor, effective_hops, &session.files);
        emit_cstring(body, out_result_ptr)
    }));
    r.unwrap_or(ERR_PANIC)
}

/// # Safety
/// `handle` must either be null (returns ERR_NULL_PTR) or a pointer
/// returned by `ctx_focus_session_open` that has not previously been
/// passed to this function. The caller MUST enforce single-close.
#[no_mangle]
pub unsafe extern "C" fn ctx_focus_session_close(handle: *mut c_void) -> c_int {
    let r = catch_unwind(AssertUnwindSafe(|| {
        if handle.is_null() {
            return ERR_NULL_PTR;
        }
        drop(Box::from_raw(handle as *mut FocusSession));
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
        let p = ctx_focus_version();
        assert!(!p.is_null());
        let s = unsafe { CStr::from_ptr(p) }.to_str().unwrap();
        assert_eq!(s, "ctx-focus 0.1.0");
    }

    fn open_session(files_json: &str) -> *mut c_void {
        let opts = "{}";
        let mut handle: *mut c_void = ptr::null_mut();
        let rc = unsafe {
            ctx_focus_session_open(
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

    fn fixture_json() -> &'static str {
        r#"[
            {"path":"internal/pack/pack.go","is_dir":false,
             "symbols":[{"name":"Pack","kind":"function","line":2}],
             "lines":["package pack","func Pack() {}"]},
            {"path":"internal/pack/helper.go","is_dir":false,
             "symbols":[{"name":"helper","kind":"function","line":2}],
             "lines":["package pack","func helper() {}"]},
            {"path":"internal/render/render.go","is_dir":false,
             "symbols":[{"name":"RenderPack","kind":"function","line":2}],
             "lines":["package render","// uses Pack"]}
        ]"#
    }

    #[test]
    fn t_session_open_close_no_leak_1000() {
        for _ in 0..1000 {
            let h = open_session(fixture_json());
            let rc = unsafe { ctx_focus_session_close(h) };
            assert_eq!(rc, ERR_OK);
        }
    }

    #[test]
    fn t_session_resolve_returns_anchor() {
        let handle = open_session(fixture_json());
        let q = "Pack";
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe {
            ctx_focus_session_resolve(handle, q.as_ptr(), q.len(), &mut out)
        };
        assert_eq!(rc, ERR_OK);
        let s = unsafe { CStr::from_ptr(out) }.to_str().unwrap().to_owned();
        unsafe { ctx_focus_free_string(out) };
        assert!(s.contains("\"OriginPath\":\"internal/pack/pack.go\""), "{s}");
        let rc = unsafe { ctx_focus_session_close(handle) };
        assert_eq!(rc, ERR_OK);
    }

    #[test]
    fn t_session_expand_returns_files() {
        let handle = open_session(fixture_json());
        let q = "Pack";
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe {
            ctx_focus_session_expand(handle, q.as_ptr(), q.len(), 1, &mut out)
        };
        assert_eq!(rc, ERR_OK);
        let s = unsafe { CStr::from_ptr(out) }.to_str().unwrap().to_owned();
        unsafe { ctx_focus_free_string(out) };
        assert!(s.contains("internal/pack/pack.go"), "{s}");
        assert!(s.contains("anchor-origin"), "{s}");
        unsafe { ctx_focus_session_close(handle) };
    }

    #[test]
    fn t_session_pack_returns_anchor_and_files() {
        let handle = open_session(fixture_json());
        let q = "Pack";
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe {
            ctx_focus_session_pack(handle, q.as_ptr(), q.len(), 1, &mut out)
        };
        assert_eq!(rc, ERR_OK);
        let s = unsafe { CStr::from_ptr(out) }.to_str().unwrap().to_owned();
        unsafe { ctx_focus_free_string(out) };
        assert!(s.contains("\"anchor\""));
        assert!(s.contains("\"files\""));
        unsafe { ctx_focus_session_close(handle) };
    }

    #[test]
    fn t_session_concurrent_queries_safe() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        use std::thread;

        let handle = open_session(fixture_json());
        let handle_usize = handle as usize;
        let ok = Arc::new(AtomicUsize::new(0));
        let mut joins = Vec::new();
        for t in 0..4 {
            let ok = Arc::clone(&ok);
            joins.push(thread::spawn(move || {
                let h = handle_usize as *mut c_void;
                for i in 0..25 {
                    let q = match (t + i) % 3 {
                        0 => "Pack",
                        1 => "helper",
                        _ => "RenderPack",
                    };
                    let mut out: *mut c_char = ptr::null_mut();
                    let rc = unsafe {
                        ctx_focus_session_resolve(h, q.as_ptr(), q.len(), &mut out)
                    };
                    assert_eq!(rc, ERR_OK);
                    if !out.is_null() {
                        unsafe { ctx_focus_free_string(out) };
                    }
                    ok.fetch_add(1, Ordering::Relaxed);
                }
            }));
        }
        for j in joins {
            j.join().expect("thread join");
        }
        assert_eq!(ok.load(Ordering::Relaxed), 4 * 25);
        unsafe { ctx_focus_session_close(handle) };
    }

    #[test]
    fn t_session_close_null_handle_is_safe() {
        let rc = unsafe { ctx_focus_session_close(ptr::null_mut()) };
        assert_eq!(rc, ERR_NULL_PTR);
    }

    #[test]
    fn t_session_open_rejects_bad_json() {
        let bad = "not-json";
        let opts = "{}";
        let mut h: *mut c_void = ptr::null_mut();
        let rc = unsafe {
            ctx_focus_session_open(
                bad.as_ptr(),
                bad.len(),
                opts.as_ptr(),
                opts.len(),
                &mut h,
            )
        };
        assert_eq!(rc, ERR_BAD_JSON);
        assert!(h.is_null());
    }

    #[test]
    fn t_session_expand_resolves_ambiguous_envelope() {
        // Two symbols named "Foo" → ambiguous envelope, not a panic.
        let files = r#"[
            {"path":"a/a.go","is_dir":false,"symbols":[{"name":"Foo","kind":"function","line":1}],"lines":[]},
            {"path":"b/b.go","is_dir":false,"symbols":[{"name":"Foo","kind":"function","line":1}],"lines":[]}
        ]"#;
        let handle = open_session(files);
        let q = "Foo";
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe {
            ctx_focus_session_expand(handle, q.as_ptr(), q.len(), 1, &mut out)
        };
        assert_eq!(rc, ERR_OK);
        let s = unsafe { CStr::from_ptr(out) }.to_str().unwrap().to_owned();
        unsafe { ctx_focus_free_string(out) };
        assert!(s.contains("ambiguous"), "{s}");
        unsafe { ctx_focus_session_close(handle) };
    }

    #[test]
    fn t_stateless_pack_smoke() {
        let files = fixture_json();
        let q = "Pack";
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe {
            ctx_focus_pack(
                files.as_ptr(),
                files.len(),
                q.as_ptr(),
                q.len(),
                1,
                &mut out,
            )
        };
        assert_eq!(rc, ERR_OK);
        let s = unsafe { CStr::from_ptr(out) }.to_str().unwrap().to_owned();
        unsafe { ctx_focus_free_string(out) };
        assert!(s.contains("\"anchor\""));
    }
}
