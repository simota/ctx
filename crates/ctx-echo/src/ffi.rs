// crates/ctx-echo/src/ffi.rs
//
// Stateless batch FFI for ctx-echo. Mirrors the conventions in
// crates/ctx-contract/src/ffi.rs and crates/ctx-scan/src/ffi.rs:
//
//   * All inputs are BORROWED for the call window; we never retain.
//   * Outputs are emitted as heap-owned CStrings via `into_raw`; the
//     caller MUST call `ctx_echo_free_string` exactly once on each
//     non-null out pointer.
//   * Every extern wraps its body in `catch_unwind` — on panic we
//     return -99 and leave out-params untouched.
//
// FUNCTION SURFACE
// ================
//   ctx_echo_evaluate(pack_path_ptr, pack_path_len,
//                     pack_body_ptr, pack_body_len,
//                     opts_json_ptr, opts_json_len,
//                     out_result_ptr) -> i32
//     Runs the BM25 evaluator over `pack_body` under `opts_json` and
//     writes a JSON-encoded EchoResult into *out_result_ptr.
//
//   ctx_echo_free_string(s) — drop a string previously returned.
//   ctx_echo_version() -> *const c_char — static banner.

use std::ffi::{c_char, c_int, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::slice;

use crate::evaluate::evaluate;
use crate::types::Options;

const MAX_INPUT_BYTES: usize = 100 * 1024 * 1024;

const ERR_OK: c_int = 0;
const ERR_NULL_PTR: c_int = -1;
const ERR_TOO_LARGE: c_int = -2;
const ERR_BAD_JSON: c_int = -3;
const ERR_SERIALIZE: c_int = -4;
const ERR_PANIC: c_int = -99;

static VERSION_C: once_cell::sync::Lazy<CString> =
    once_cell::sync::Lazy::new(|| CString::new("ctx-echo 0.1.0").expect("version cstr"));

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

fn decode_opts(bytes: &[u8]) -> Result<Options, c_int> {
    if bytes.is_empty() {
        return Ok(Options::default());
    }
    serde_json::from_slice(bytes).map_err(|_| ERR_BAD_JSON)
}

fn decode_utf8(bytes: &[u8]) -> Result<&str, c_int> {
    std::str::from_utf8(bytes).map_err(|_| ERR_BAD_JSON)
}

// ---------------------------------------------------------------------
// ctx_echo_evaluate
// ---------------------------------------------------------------------

/// Run the BM25 evaluator. On success writes a JSON-encoded EchoResult
/// into `*out_result_ptr`. Empty `opts_json` (len=0) is treated as
/// `Options::default()`.
///
/// # Safety
/// See module-level docs.
#[no_mangle]
pub unsafe extern "C" fn ctx_echo_evaluate(
    pack_path_ptr: *const u8,
    pack_path_len: usize,
    pack_body_ptr: *const u8,
    pack_body_len: usize,
    opts_json_ptr: *const u8,
    opts_json_len: usize,
    out_result_ptr: *mut *mut c_char,
) -> c_int {
    let result = catch_unwind(AssertUnwindSafe(|| {
        if out_result_ptr.is_null() {
            return ERR_NULL_PTR;
        }
        *out_result_ptr = ptr::null_mut();

        let pack_path_bytes = match slice_from_raw(pack_path_ptr, pack_path_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let pack_body_bytes = match slice_from_raw(pack_body_ptr, pack_body_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let opts_bytes = match slice_from_raw(opts_json_ptr, opts_json_len) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let pack_path = match decode_utf8(pack_path_bytes) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let pack_body = match decode_utf8(pack_body_bytes) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let opts = match decode_opts(opts_bytes) {
            Ok(o) => o,
            Err(e) => return e,
        };

        let res = evaluate(pack_path, pack_body, &opts);
        let json = match serde_json::to_string(&res) {
            Ok(s) => s,
            Err(_) => return ERR_SERIALIZE,
        };
        emit_cstring(json, out_result_ptr)
    }));
    result.unwrap_or(ERR_PANIC)
}

// ---------------------------------------------------------------------
// Free helper
// ---------------------------------------------------------------------

/// Free a string previously returned from one of the `ctx_echo_*`
/// functions via `out_*_ptr`. Safe to call on a null pointer (no-op).
///
/// # Safety
/// `s` must either be null or a pointer originally returned by this
/// crate's FFI. Calling on any other pointer is undefined behaviour.
#[no_mangle]
pub unsafe extern "C" fn ctx_echo_free_string(s: *mut c_char) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !s.is_null() {
            drop(CString::from_raw(s));
        }
    }));
}

// ---------------------------------------------------------------------
// ctx_echo_version
// ---------------------------------------------------------------------

/// Returns a pointer to a `'static` NUL-terminated C string carrying
/// the crate's version banner. The caller MUST NOT free it.
#[no_mangle]
pub extern "C" fn ctx_echo_version() -> *const c_char {
    VERSION_C.as_ptr()
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;

    fn cstr_into_string(p: *mut c_char) -> String {
        assert!(!p.is_null(), "expected non-null out-string");
        let s = unsafe { CStr::from_ptr(p) }
            .to_str()
            .expect("utf-8")
            .to_owned();
        unsafe { ctx_echo_free_string(p) };
        s
    }

    #[test]
    fn version_round_trips() {
        let p = ctx_echo_version();
        assert!(!p.is_null());
        let s = unsafe { CStr::from_ptr(p) }.to_str().unwrap();
        assert_eq!(s, "ctx-echo 0.1.0");
    }

    #[test]
    fn evaluate_happy_path() {
        let pack_path = "inline";
        let pack_body =
            "## File contents\n\n### foo/bar.go\n\n```go\nfunc BurstHandler() {}\n```\n";
        let opts = br#"{"goal":"burst handler","top":5}"#;
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe {
            ctx_echo_evaluate(
                pack_path.as_ptr(),
                pack_path.len(),
                pack_body.as_ptr(),
                pack_body.len(),
                opts.as_ptr(),
                opts.len(),
                &mut out,
            )
        };
        assert_eq!(rc, ERR_OK);
        let json = cstr_into_string(out);
        assert!(json.contains("\"pack_file\":\"inline\""), "json = {json}");
        assert!(json.contains("\"chunks_total\""), "json = {json}");
    }

    #[test]
    fn rejects_null_out_ptr() {
        let rc = unsafe {
            ctx_echo_evaluate(
                ptr::null(),
                0,
                ptr::null(),
                0,
                ptr::null(),
                0,
                ptr::null_mut(),
            )
        };
        assert_eq!(rc, ERR_NULL_PTR);
    }

    #[test]
    fn rejects_oversize_input() {
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe {
            ctx_echo_evaluate(
                1 as *const u8,
                MAX_INPUT_BYTES + 1,
                ptr::null(),
                0,
                ptr::null(),
                0,
                &mut out,
            )
        };
        assert_eq!(rc, ERR_TOO_LARGE);
        assert!(out.is_null());
    }

    #[test]
    fn bad_opts_json_returns_error() {
        let bad = b"{not json";
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe {
            ctx_echo_evaluate(
                b"".as_ptr(),
                0,
                b"".as_ptr(),
                0,
                bad.as_ptr(),
                bad.len(),
                &mut out,
            )
        };
        assert_eq!(rc, ERR_BAD_JSON);
    }

    #[test]
    fn empty_opts_uses_defaults() {
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe {
            ctx_echo_evaluate(
                b"x".as_ptr(),
                1,
                b"".as_ptr(),
                0,
                ptr::null(),
                0,
                &mut out,
            )
        };
        assert_eq!(rc, ERR_OK);
        let json = cstr_into_string(out);
        assert!(json.contains("\"chunks_total\":0"), "json = {json}");
    }

    #[test]
    fn free_string_on_null_is_safe() {
        unsafe { ctx_echo_free_string(ptr::null_mut()) };
    }
}
