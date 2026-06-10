// crates/ctx-symbols/src/ffi.rs
//
// Phase 4 Tier 2 #5 mixed-shape FFI surface for ctx-symbols.
//
// API SHAPE: mixed stateless + sessioned (cf. ctx-pack's split).
//   - apionly: stateless render (caller hands lines + ranges + edits)
//   - lookup:  sessioned open(root, corpus_json) → query(args_json)
//     × N → close. Stateless `lookup_resolve` also exposed for one-shot
//     callers and parity tests.
//
// FUNCTION SURFACE
// ================
//   ctx_symbols_apionly_render(req_ptr, req_len, out_json)             -> i32
//   ctx_symbols_lookup_resolve(corpus_ptr, corpus_len,
//                              args_ptr, args_len, out_json)           -> i32
//   ctx_symbols_lookup_session_open(root_ptr, root_len,
//                                   corpus_ptr, corpus_len,
//                                   out_handle)                        -> i32
//   ctx_symbols_lookup_session_query(handle, kind_ptr, kind_len,
//                                    args_ptr, args_len, out_json)     -> i32
//   ctx_symbols_lookup_session_close(handle)                           -> i32
//
//   ctx_symbols_free_string(s)
//   ctx_symbols_version() -> *const c_char
//
// All JSON shapes follow types.rs serde derives.

use std::ffi::{c_char, c_int, c_void, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::slice;

use crate::apionly::render_api;
use crate::lookup::resolve;
use crate::lookup::session::LookupSession;
use crate::types::{APIRenderRequest, FileSymbols, LookupArgs};

const MAX_INPUT_BYTES: usize = 256 * 1024 * 1024;

pub const ERR_OK: c_int = 0;
pub const ERR_NULL_PTR: c_int = -1;
pub const ERR_TOO_LARGE: c_int = -2;
pub const ERR_BAD_JSON: c_int = -3;
pub const ERR_SERIALIZE: c_int = -4;
pub const ERR_BAD_HANDLE: c_int = -5;
pub const ERR_BAD_KIND: c_int = -6;
pub const ERR_PANIC: c_int = -99;

static VERSION_C: once_cell::sync::Lazy<CString> =
    once_cell::sync::Lazy::new(|| CString::new("ctx-symbols 0.1.0").expect("version cstr"));

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

fn decode_utf8(b: &[u8]) -> Result<&str, c_int> {
    std::str::from_utf8(b).map_err(|_| ERR_BAD_JSON)
}

fn emit_cstring(value: String, out: *mut *mut c_char) -> c_int {
    let c = match CString::new(value) {
        Ok(c) => c,
        Err(_) => return ERR_SERIALIZE,
    };
    unsafe { *out = c.into_raw() };
    ERR_OK
}

fn emit_ok_value(value: impl serde::Serialize, out: *mut *mut c_char) -> c_int {
    let body = match serde_json::to_string(&value) {
        Ok(s) => s,
        Err(_) => return ERR_SERIALIZE,
    };
    emit_cstring(body, out)
}

// =========================================================================
// apionly stateless render
// =========================================================================

/// # Safety
/// `req_ptr` must be valid for `req_len` bytes. `out_result_ptr` must be
/// a valid, writable pointer to `*mut c_char`. On success the caller
/// owns the returned C string and MUST free via
/// `ctx_symbols_free_string`.
#[no_mangle]
pub unsafe extern "C" fn ctx_symbols_apionly_render(
    req_ptr: *const u8,
    req_len: usize,
    out_result_ptr: *mut *mut c_char,
) -> c_int {
    let r = catch_unwind(AssertUnwindSafe(|| {
        if out_result_ptr.is_null() {
            return ERR_NULL_PTR;
        }
        *out_result_ptr = ptr::null_mut();
        let bytes = match slice_from_raw(req_ptr, req_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let req: APIRenderRequest = match serde_json::from_slice(bytes) {
            Ok(v) => v,
            Err(_) => return ERR_BAD_JSON,
        };
        let rendered = render_api(&req);
        // Wrap as `{"rendered": "..."}` so the JSON envelope is
        // self-describing and matches the convention used by braid /
        // pack stateless FFI.
        let env = serde_json::json!({ "rendered": rendered });
        emit_ok_value(env, out_result_ptr)
    }));
    r.unwrap_or(ERR_PANIC)
}

// =========================================================================
// lookup stateless resolve
// =========================================================================

/// # Safety
/// See `ctx_symbols_apionly_render` for the slice/out-ptr contract.
#[no_mangle]
pub unsafe extern "C" fn ctx_symbols_lookup_resolve(
    corpus_ptr: *const u8,
    corpus_len: usize,
    args_ptr: *const u8,
    args_len: usize,
    out_result_ptr: *mut *mut c_char,
) -> c_int {
    let r = catch_unwind(AssertUnwindSafe(|| {
        if out_result_ptr.is_null() {
            return ERR_NULL_PTR;
        }
        *out_result_ptr = ptr::null_mut();
        let corpus_bytes = match slice_from_raw(corpus_ptr, corpus_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let args_bytes = match slice_from_raw(args_ptr, args_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let corpus: Vec<FileSymbols> = match serde_json::from_slice(corpus_bytes) {
            Ok(v) => v,
            Err(_) => return ERR_BAD_JSON,
        };
        let args: LookupArgs = match serde_json::from_slice(args_bytes) {
            Ok(v) => v,
            Err(_) => return ERR_BAD_JSON,
        };
        let hits = resolve(&corpus, &args);
        emit_ok_value(hits, out_result_ptr)
    }));
    r.unwrap_or(ERR_PANIC)
}

// =========================================================================
// lookup sessioned
// =========================================================================

/// # Safety
/// `out_handle` must be a valid, writable pointer to `*mut c_void`.
/// On success the caller owns the handle and must release via
/// `ctx_symbols_lookup_session_close`.
#[no_mangle]
pub unsafe extern "C" fn ctx_symbols_lookup_session_open(
    root_ptr: *const u8,
    root_len: usize,
    corpus_ptr: *const u8,
    corpus_len: usize,
    out_handle: *mut *mut c_void,
) -> c_int {
    let r = catch_unwind(AssertUnwindSafe(|| {
        if out_handle.is_null() {
            return ERR_NULL_PTR;
        }
        *out_handle = ptr::null_mut();
        let root_bytes = match slice_from_raw(root_ptr, root_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let corpus_bytes = match slice_from_raw(corpus_ptr, corpus_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let root = match decode_utf8(root_bytes) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let corpus: Vec<FileSymbols> = if corpus_bytes.is_empty() {
            Vec::new()
        } else {
            match serde_json::from_slice(corpus_bytes) {
                Ok(v) => v,
                Err(_) => return ERR_BAD_JSON,
            }
        };
        let session = LookupSession::open(root, corpus);
        let boxed = Box::new(session);
        *out_handle = Box::into_raw(boxed) as *mut c_void;
        ERR_OK
    }));
    r.unwrap_or(ERR_PANIC)
}

/// # Safety
/// `handle` must be a session pointer returned by `_session_open` and
/// not yet closed.
#[no_mangle]
pub unsafe extern "C" fn ctx_symbols_lookup_session_query(
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
        let args: LookupArgs = if args_bytes.is_empty() {
            LookupArgs::default()
        } else {
            match serde_json::from_slice(args_bytes) {
                Ok(v) => v,
                Err(_) => return ERR_BAD_JSON,
            }
        };
        let session = &*(handle as *const LookupSession);
        match kind {
            "resolve" => {
                let hits = session.resolve(&args);
                emit_ok_value(hits, out_result_ptr)
            }
            "find_references" | "refs" => {
                let hits = session.find_references(&args);
                emit_ok_value(hits, out_result_ptr)
            }
            "stats" => {
                let env = serde_json::json!({
                    "root": session.root(),
                    "files": session.corpus_len(),
                    "symbols": session.total_symbols(),
                });
                emit_ok_value(env, out_result_ptr)
            }
            _ => ERR_BAD_KIND,
        }
    }));
    r.unwrap_or(ERR_PANIC)
}

/// # Safety
/// `handle` must either be null (returns ERR_NULL_PTR) or a pointer
/// returned by `_session_open` not yet passed to this function.
#[no_mangle]
pub unsafe extern "C" fn ctx_symbols_lookup_session_close(handle: *mut c_void) -> c_int {
    let r = catch_unwind(AssertUnwindSafe(|| {
        if handle.is_null() {
            return ERR_NULL_PTR;
        }
        drop(Box::from_raw(handle as *mut LookupSession));
        ERR_OK
    }));
    r.unwrap_or(ERR_PANIC)
}

// =========================================================================
// free / version
// =========================================================================

/// # Safety
/// `s` must either be null (no-op) or a pointer returned by a prior
/// successful FFI call.
#[no_mangle]
pub unsafe extern "C" fn ctx_symbols_free_string(s: *mut c_char) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !s.is_null() {
            drop(CString::from_raw(s));
        }
    }));
}

/// Returns a pointer to a `'static` NUL-terminated version banner.
#[no_mangle]
pub extern "C" fn ctx_symbols_version() -> *const c_char {
    VERSION_C.as_ptr()
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
        unsafe { ctx_symbols_free_string(p) };
        s
    }

    #[test]
    fn version_round_trips() {
        let p = ctx_symbols_version();
        assert!(!p.is_null());
        let s = unsafe { CStr::from_ptr(p) }.to_str().unwrap();
        assert_eq!(s, "ctx-symbols 0.1.0");
    }

    #[test]
    fn apionly_render_round_trip() {
        let req = serde_json::json!({
            "lines": ["package x", "", "func F() {}"],
            "ranges": [{"start": 0, "end": 0}]
        });
        let body = serde_json::to_vec(&req).unwrap();
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe {
            ctx_symbols_apionly_render(body.as_ptr(), body.len(), &mut out)
        };
        assert_eq!(rc, ERR_OK);
        let s = cstr_into_string(out);
        assert!(s.contains("\"rendered\":\"package x\\n\""), "{s}");
    }

    #[test]
    fn lookup_resolve_stateless_round_trip() {
        let corpus = serde_json::json!([
            {"Path": "a.go", "Symbols": [
                {"Name": "F", "Kind": "function", "Line": 1}
            ]}
        ]);
        let args = serde_json::json!({"name": "F"});
        let cb = serde_json::to_vec(&corpus).unwrap();
        let ab = serde_json::to_vec(&args).unwrap();
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe {
            ctx_symbols_lookup_resolve(
                cb.as_ptr(),
                cb.len(),
                ab.as_ptr(),
                ab.len(),
                &mut out,
            )
        };
        assert_eq!(rc, ERR_OK);
        let s = cstr_into_string(out);
        assert!(s.contains("\"Path\":\"a.go\""), "{s}");
        assert!(s.contains("\"SymbolName\":\"F\""), "{s}");
    }

    #[test]
    fn lookup_session_open_query_close_round_trip() {
        let corpus = serde_json::json!([
            {"Path": "internal/web/handlers.go", "Symbols": [
                {"Name": "BuildIndex", "Kind": "function", "Line": 200}
            ]},
            {"Path": "internal/pack/pack.go", "Symbols": [
                {"Name": "BuildIndex", "Kind": "function", "Line": 50}
            ]}
        ]);
        let cb = serde_json::to_vec(&corpus).unwrap();
        let root = b"/repo";

        let mut handle: *mut c_void = ptr::null_mut();
        let rc = unsafe {
            ctx_symbols_lookup_session_open(
                root.as_ptr(),
                root.len(),
                cb.as_ptr(),
                cb.len(),
                &mut handle,
            )
        };
        assert_eq!(rc, ERR_OK);
        assert!(!handle.is_null());

        // Query #1: stats
        let kind = b"stats";
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe {
            ctx_symbols_lookup_session_query(
                handle,
                kind.as_ptr(),
                kind.len(),
                ptr::null(),
                0,
                &mut out,
            )
        };
        assert_eq!(rc, ERR_OK);
        let s = cstr_into_string(out);
        assert!(s.contains("\"files\":2"), "{s}");
        assert!(s.contains("\"symbols\":2"), "{s}");

        // Query #2: resolve with from=
        let kind = b"resolve";
        let args = serde_json::json!({
            "name": "BuildIndex",
            "from": "internal/pack/diff.go"
        });
        let ab = serde_json::to_vec(&args).unwrap();
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe {
            ctx_symbols_lookup_session_query(
                handle,
                kind.as_ptr(),
                kind.len(),
                ab.as_ptr(),
                ab.len(),
                &mut out,
            )
        };
        assert_eq!(rc, ERR_OK);
        let s = cstr_into_string(out);
        // pack should rank first (same-directory match wins)
        assert!(
            s.starts_with("[{\"Path\":\"internal/pack/pack.go\""),
            "{s}"
        );

        let rc = unsafe { ctx_symbols_lookup_session_close(handle) };
        assert_eq!(rc, ERR_OK);
    }

    #[test]
    fn lookup_session_unknown_kind_returns_bad_kind() {
        let mut handle: *mut c_void = ptr::null_mut();
        let cb = b"[]";
        let rc = unsafe {
            ctx_symbols_lookup_session_open(
                b"/r".as_ptr(),
                2,
                cb.as_ptr(),
                cb.len(),
                &mut handle,
            )
        };
        assert_eq!(rc, ERR_OK);

        let kind = b"who_knows";
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe {
            ctx_symbols_lookup_session_query(
                handle,
                kind.as_ptr(),
                kind.len(),
                ptr::null(),
                0,
                &mut out,
            )
        };
        assert_eq!(rc, ERR_BAD_KIND);
        unsafe { ctx_symbols_lookup_session_close(handle) };
    }

    #[test]
    fn null_handle_returns_bad_handle() {
        let kind = b"resolve";
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe {
            ctx_symbols_lookup_session_query(
                ptr::null_mut(),
                kind.as_ptr(),
                kind.len(),
                ptr::null(),
                0,
                &mut out,
            )
        };
        assert_eq!(rc, ERR_BAD_HANDLE);
    }

    #[test]
    fn bad_json_rejected_in_resolve() {
        let mut out: *mut c_char = ptr::null_mut();
        let bad = b"not-json";
        let rc = unsafe {
            ctx_symbols_lookup_resolve(
                bad.as_ptr(),
                bad.len(),
                bad.as_ptr(),
                bad.len(),
                &mut out,
            )
        };
        assert_eq!(rc, ERR_BAD_JSON);
    }

    #[test]
    fn double_close_after_open_no_crash() {
        let mut handle: *mut c_void = ptr::null_mut();
        let cb = b"[]";
        let rc = unsafe {
            ctx_symbols_lookup_session_open(
                b"/r".as_ptr(),
                2,
                cb.as_ptr(),
                cb.len(),
                &mut handle,
            )
        };
        assert_eq!(rc, ERR_OK);
        let rc = unsafe { ctx_symbols_lookup_session_close(handle) };
        assert_eq!(rc, ERR_OK);
        // Calling close with the same pointer would be a UAF — the Go
        // bridge guards against this with atomic.Uint32. Here we
        // simply verify NULL is handled defensively.
        let rc = unsafe { ctx_symbols_lookup_session_close(ptr::null_mut()) };
        assert_eq!(rc, ERR_NULL_PTR);
    }
}
