// crates/ctx-relations/src/ffi.rs
//
// Phase 2 FFI surface for ctx-relations. Mirrors the conventions used
// in crates/ctx-scan/src/ffi.rs:
//
//   * All inputs are BORROWED for the call window; we never retain.
//   * Outputs are emitted as heap-owned CStrings via `into_raw`; the
//     caller MUST call `ctx_relations_free_string` exactly once on each
//     non-null out pointer.
//   * Every extern wraps its body in `catch_unwind` — on panic we
//     return -99 and leave out-params untouched.
//
// FUNCTION SURFACE
// ================
//   ctx_relations_build(root_ptr, root_len, out_result_ptr) -> i32
//     Walks `root`, builds the Index, returns serialized JSON.
//
//   ctx_relations_build_cached(root_ptr, root_len, out_result_ptr) -> i32
//     Same as above but memoises per absolute root path.
//
//   ctx_relations_invalidate_cache(root_ptr, root_len) -> i32
//     Drops any cached Index for `root`.
//
//   ctx_relations_free_string(s) — drop a string previously returned.
//   ctx_relations_version() -> *const c_char — static banner.

use std::ffi::{c_char, c_int, c_void, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::slice;

use crate::build;
use crate::cache;
use crate::session::{QueryError, RelationsSession};

const MAX_INPUT_BYTES: usize = 100 * 1024 * 1024;

const ERR_OK: c_int = 0;
const ERR_NULL_PTR: c_int = -1;
const ERR_TOO_LARGE: c_int = -2;
const ERR_BAD_JSON: c_int = -3;
const ERR_SERIALIZE: c_int = -4;
const ERR_IO: c_int = -5;
const ERR_BAD_HANDLE: c_int = -10;
const ERR_BAD_KIND: c_int = -11;
const ERR_PANIC: c_int = -99;

static VERSION_C: once_cell::sync::Lazy<CString> =
    once_cell::sync::Lazy::new(|| CString::new("ctx-relations 0.1.0").expect("version cstr"));

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

// ---------------------------------------------------------------------
// ctx_relations_build
// ---------------------------------------------------------------------

/// Build the relations Index for `root` and emit the JSON
/// serialization into `*out_result_ptr`.
///
/// # Safety
/// See module-level docs.
#[no_mangle]
pub unsafe extern "C" fn ctx_relations_build(
    root_ptr: *const u8,
    root_len: usize,
    out_result_ptr: *mut *mut c_char,
) -> c_int {
    let result = catch_unwind(AssertUnwindSafe(|| {
        if out_result_ptr.is_null() {
            return ERR_NULL_PTR;
        }
        *out_result_ptr = ptr::null_mut();

        let root_bytes = match slice_from_raw(root_ptr, root_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let root = match decode_utf8(root_bytes) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let idx = match build::build(root) {
            Ok(i) => i,
            Err(_) => return ERR_IO,
        };
        let json = match serde_json::to_string(&idx) {
            Ok(s) => s,
            Err(_) => return ERR_SERIALIZE,
        };
        emit_cstring(json, out_result_ptr)
    }));
    result.unwrap_or(ERR_PANIC)
}

// ---------------------------------------------------------------------
// ctx_relations_build_cached
// ---------------------------------------------------------------------

/// Build the relations Index for `root`, hitting the in-memory cache
/// if available. See `crate::cache::build_cached` for invalidation
/// semantics.
///
/// # Safety
/// See module-level docs.
#[no_mangle]
pub unsafe extern "C" fn ctx_relations_build_cached(
    root_ptr: *const u8,
    root_len: usize,
    out_result_ptr: *mut *mut c_char,
) -> c_int {
    let result = catch_unwind(AssertUnwindSafe(|| {
        if out_result_ptr.is_null() {
            return ERR_NULL_PTR;
        }
        *out_result_ptr = ptr::null_mut();

        let root_bytes = match slice_from_raw(root_ptr, root_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let root = match decode_utf8(root_bytes) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let idx = match cache::build_cached(root) {
            Ok(i) => i,
            Err(_) => return ERR_IO,
        };
        let json = match serde_json::to_string(&idx) {
            Ok(s) => s,
            Err(_) => return ERR_SERIALIZE,
        };
        emit_cstring(json, out_result_ptr)
    }));
    result.unwrap_or(ERR_PANIC)
}

// ---------------------------------------------------------------------
// ctx_relations_invalidate_cache
// ---------------------------------------------------------------------

/// Drop any cached Index for `root`.
///
/// # Safety
/// See module-level docs.
#[no_mangle]
pub unsafe extern "C" fn ctx_relations_invalidate_cache(
    root_ptr: *const u8,
    root_len: usize,
) -> c_int {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let root_bytes = match slice_from_raw(root_ptr, root_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let root = match decode_utf8(root_bytes) {
            Ok(s) => s,
            Err(e) => return e,
        };
        cache::invalidate_cache(root);
        ERR_OK
    }));
    result.unwrap_or(ERR_PANIC)
}

// ---------------------------------------------------------------------
// free / version
// ---------------------------------------------------------------------

/// Free a string previously returned from one of the
/// `ctx_relations_*` functions via `out_result_ptr`. Safe to call on a
/// null pointer (no-op).
///
/// # Safety
/// `s` must either be null or a pointer originally returned by this
/// crate's FFI. Calling on any other pointer is undefined behaviour.
#[no_mangle]
pub unsafe extern "C" fn ctx_relations_free_string(s: *mut c_char) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !s.is_null() {
            drop(CString::from_raw(s));
        }
    }));
}

/// Returns a pointer to a `'static` NUL-terminated C string carrying
/// the crate's version banner. The caller MUST NOT free it.
#[no_mangle]
pub extern "C" fn ctx_relations_version() -> *const c_char {
    VERSION_C.as_ptr()
}

// =================================================================
// ADR-002 sticky-handle session API.
//
// The session caches the already-built Index on the Rust side. The Go
// caller opens ONCE per logical scope (web handler init, browse session
// startup) and then routes N queries through the handle, amortising
// away the walk + parse + JSON-marshal-the-whole-Index cost the
// stateless build_cached path pays per call.
//
// FUNCTION SURFACE
// ================
//   ctx_relations_session_open(root_ptr, root_len, opts_ptr, opts_len,
//                              out_handle) -> i32
//   ctx_relations_session_query(handle, kind_ptr, kind_len,
//                               args_ptr, args_len, out_result_ptr) -> i32
//   ctx_relations_session_close(handle) -> i32
//
// `opts_json` is currently reserved for forward compatibility (e.g.
// engine flags). Empty input is accepted.
// =================================================================

/// # Safety
/// `out_handle` must be a valid, writable pointer to a `*mut c_void`.
/// On success the caller owns the handle and must release it via
/// `ctx_relations_session_close`.
#[no_mangle]
pub unsafe extern "C" fn ctx_relations_session_open(
    root_ptr: *const u8,
    root_len: usize,
    _opts_ptr: *const u8,
    opts_len: usize,
    out_handle: *mut *mut c_void,
) -> c_int {
    let r = catch_unwind(AssertUnwindSafe(|| {
        if out_handle.is_null() {
            return ERR_NULL_PTR;
        }
        *out_handle = ptr::null_mut();

        // opts is optional — we only enforce the size limit so a caller
        // can't smuggle a giant buffer through.
        if opts_len > MAX_INPUT_BYTES {
            return ERR_TOO_LARGE;
        }
        let root_bytes = match slice_from_raw(root_ptr, root_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let root = match decode_utf8(root_bytes) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let session = match RelationsSession::open(root) {
            Ok(s) => s,
            Err(_) => return ERR_IO,
        };
        let boxed = Box::new(session);
        *out_handle = Box::into_raw(boxed) as *mut c_void;
        ERR_OK
    }));
    r.unwrap_or(ERR_PANIC)
}

/// # Safety
/// `handle` must have been returned by a prior successful call to
/// `ctx_relations_session_open` and must not have been passed to
/// `ctx_relations_session_close`.
#[no_mangle]
pub unsafe extern "C" fn ctx_relations_session_query(
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
        // args may legitimately be empty (e.g. index_summary)
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
        let session = &*(handle as *const RelationsSession);
        match session.query(kind, args) {
            Ok(body) => emit_cstring(body, out_result_ptr),
            Err(QueryError::UnknownKind(_)) => ERR_BAD_KIND,
            Err(QueryError::BadArgs) => ERR_BAD_JSON,
            Err(QueryError::Serialize) => ERR_SERIALIZE,
        }
    }));
    r.unwrap_or(ERR_PANIC)
}

/// # Safety
/// `handle` must either be null (returns ERR_NULL_PTR) or a pointer
/// returned by `ctx_relations_session_open` that has not previously been
/// passed to this function. The caller MUST enforce single-close.
#[no_mangle]
pub unsafe extern "C" fn ctx_relations_session_close(handle: *mut c_void) -> c_int {
    let r = catch_unwind(AssertUnwindSafe(|| {
        if handle.is_null() {
            return ERR_NULL_PTR;
        }
        drop(Box::from_raw(handle as *mut RelationsSession));
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
        unsafe { ctx_relations_free_string(p) };
        s
    }

    #[test]
    fn version_round_trips() {
        let p = ctx_relations_version();
        assert!(!p.is_null());
        let s = unsafe { CStr::from_ptr(p) }.to_str().unwrap();
        assert_eq!(s, "ctx-relations 0.1.0");
    }

    #[test]
    fn build_empty_dir_returns_empty_index() {
        let dir = std::env::temp_dir().join(format!(
            "rel-ffi-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let root = dir.to_string_lossy().to_string();
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe {
            ctx_relations_build(root.as_ptr(), root.len(), &mut out)
        };
        assert_eq!(rc, ERR_OK);
        let json = cstr_into_string(out);
        // BTreeMaps emit `{}` for empty maps.
        assert!(json.contains("\"module_path\":\"\""), "{json}");
        assert!(json.contains("\"imports\":{}"), "{json}");
    }

    #[test]
    fn rejects_null_out_ptr() {
        let rc = unsafe { ctx_relations_build(ptr::null(), 0, ptr::null_mut()) };
        assert_eq!(rc, ERR_NULL_PTR);
    }

    #[test]
    fn rejects_oversize_input() {
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe {
            ctx_relations_build(1 as *const u8, MAX_INPUT_BYTES + 1, &mut out)
        };
        assert_eq!(rc, ERR_TOO_LARGE);
        assert!(out.is_null());
    }

    #[test]
    fn invalidate_cache_handles_missing_root() {
        let bogus = "/tmp/does/not/exist/abcdefg";
        let rc = unsafe {
            ctx_relations_invalidate_cache(bogus.as_ptr(), bogus.len())
        };
        // Invalid path resolves to ERR_OK (canonicalize fails → no-op
        // per the Go semantics).
        assert_eq!(rc, ERR_OK);
    }

    #[test]
    fn free_string_on_null_is_safe() {
        unsafe { ctx_relations_free_string(ptr::null_mut()) };
    }
}
