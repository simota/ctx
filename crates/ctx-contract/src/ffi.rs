// crates/ctx-contract/src/ffi.rs
//
// T-26: stable C ABI for ctx-contract. The Go CLI (T-27) consumes this
// crate via cgo through `crates/ctx-contract/include/ctx_contract.h`,
// which is generated from this file by `build.rs` + `cbindgen`.
//
// MEMORY OWNERSHIP PROTOCOL
// =========================
//   Inputs (caller -> Rust):
//     - All `*const u8 + usize len` and `*const c_char + usize len`
//       buffers are BORROWED for the duration of the call. The caller
//       retains ownership and may free them immediately upon return.
//     - Rust never retains pointers across calls. The crate is
//       stateless (Phase 1 SPECTER finding: no globals, all functions
//       pure modulo the worktree filesystem reads in `verify`).
//
//   Outputs (Rust -> caller):
//     - `*mut *mut c_char` out-params (JSON / contract / refs strings):
//       Rust allocates via `CString::into_raw`. The caller takes
//       ownership and MUST call `ctx_contract_free_string` exactly once
//       on the resulting pointer (when non-null).
//     - `*mut *mut u8` + `*mut usize` (raw byte buffer; strip output):
//       Rust allocates a `Vec<u8>`, leaks it via
//       `Vec::into_raw_parts`-equivalent dance, returns (ptr, len).
//       The caller MUST call `ctx_contract_free_buffer(ptr, len)` once.
//       The buffer is NOT null-terminated — `len` is authoritative.
//     - `ctx_contract_version` returns a pointer to a `'static` CStr.
//       The caller MUST NOT free it.
//
//   Error codes (all functions returning i32 use the same enum):
//     0    success
//    -1    null pointer in required argument
//    -2    input length exceeds MAX_INPUT_BYTES (100 MiB) — defensive
//          cap to avoid runaway allocations from corrupt caller state
//    -3    input bytes were not valid JSON / UTF-8 where required
//    -4    internal serialization failure (should not happen — would
//          indicate a bug in serde_json or this shim)
//   -99    Rust panicked. The panic is caught by `catch_unwind` and
//          no allocation is leaked; out-params are left untouched.
//
// THREAD SAFETY
// =============
//   Every public function here is reentrant and safe to call
//   concurrently from any thread. The crate has no global mutable
//   state; the few `once_cell::Lazy<Regex>` statics are read-only.
//
// PANIC SAFETY
// ============
//   Every extern function wraps its body in `std::panic::catch_unwind`.
//   On panic, we return -99 and leave all out-params untouched (the
//   caller's zero-initialised pointers remain null, so freeing them is
//   a no-op).

use std::ffi::{c_char, c_int, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::slice;

use crate::embed::{parse_from_pack, strip_contract_block};
use crate::parse_refs::extract_references;
use crate::types::{Contract, VerifyOptions};
use crate::verify::verify;

/// Defensive upper bound on any single input buffer. Anything larger
/// than 100 MiB is treated as a caller bug — packs and responses in
/// practice run well below 10 MiB.
const MAX_INPUT_BYTES: usize = 100 * 1024 * 1024;

/// Static version string returned by `ctx_contract_version`.
/// `Lazy` so we only construct the `CString` once.
static VERSION_C: once_cell::sync::Lazy<CString> =
    once_cell::sync::Lazy::new(|| CString::new("ctx-contract 0.1.0").expect("version cstr"));

// ---------------------------------------------------------------------
// Error codes (kept in sync with the doc-comment at the top of file).
// ---------------------------------------------------------------------
const ERR_OK: c_int = 0;
const ERR_NULL_PTR: c_int = -1;
const ERR_TOO_LARGE: c_int = -2;
const ERR_BAD_JSON: c_int = -3;
const ERR_SERIALIZE: c_int = -4;
const ERR_PANIC: c_int = -99;

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

/// Validate `(ptr, len)` and turn it into a `&[u8]`. Accepts a null
/// pointer when `len == 0` (callers may pass an empty input that way).
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

/// Allocate a heap-resident `CString` from a Rust `String` and hand the
/// raw pointer to the caller via `out`. Returns the appropriate error
/// code if the value contains an interior NUL.
fn emit_cstring(value: String, out: *mut *mut c_char) -> c_int {
    let c = match CString::new(value) {
        Ok(c) => c,
        Err(_) => return ERR_SERIALIZE,
    };
    unsafe { *out = c.into_raw() };
    ERR_OK
}

// ---------------------------------------------------------------------
// ctx_contract_verify
// ---------------------------------------------------------------------

/// Verify `response` against `contract_json` under `opts_json`. On
/// success writes a JSON-encoded `Result` into `*out_result_ptr`.
///
/// `opts_json` may be a null pointer + len=0 to use defaults.
///
/// # Safety
/// All input pointers must either be null (len 0) or point to a valid
/// initialised buffer of at least the indicated length. `out_result_ptr`
/// must point to writable storage for one `*mut c_char`.
#[no_mangle]
pub unsafe extern "C" fn ctx_contract_verify(
    contract_json_ptr: *const u8,
    contract_json_len: usize,
    response_ptr: *const u8,
    response_len: usize,
    opts_json_ptr: *const u8,
    opts_json_len: usize,
    out_result_ptr: *mut *mut c_char,
) -> c_int {
    let result = catch_unwind(AssertUnwindSafe(|| {
        if out_result_ptr.is_null() {
            return ERR_NULL_PTR;
        }
        *out_result_ptr = ptr::null_mut();

        let contract_bytes = match slice_from_raw(contract_json_ptr, contract_json_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let response_bytes = match slice_from_raw(response_ptr, response_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let opts_bytes = match slice_from_raw(opts_json_ptr, opts_json_len) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let contract: Contract = match serde_json::from_slice(contract_bytes) {
            Ok(c) => c,
            Err(_) => return ERR_BAD_JSON,
        };

        // Options JSON shape mirrors the Rust `VerifyOptions` field names
        // (snake_case). Empty input → default. We use a small POD shadow
        // struct because `VerifyOptions` doesn't implement Deserialize
        // directly (it lives in src/types.rs which we're told not to
        // touch). The struct here intentionally matches that JSON shape
        // 1:1 so this stays a faithful transport.
        let opts: VerifyOptions = if opts_bytes.is_empty() {
            VerifyOptions::default()
        } else {
            #[derive(serde::Deserialize, Default)]
            struct OptsWire {
                #[serde(default)]
                strict: bool,
                #[serde(default)]
                no_symbols: bool,
                #[serde(default)]
                worktree_root: String,
            }
            match serde_json::from_slice::<OptsWire>(opts_bytes) {
                Ok(w) => VerifyOptions {
                    strict: w.strict,
                    no_symbols: w.no_symbols,
                    worktree_root: w.worktree_root,
                },
                Err(_) => return ERR_BAD_JSON,
            }
        };

        let res = verify(&contract, response_bytes, &opts);
        let json = match serde_json::to_string(&res) {
            Ok(s) => s,
            Err(_) => return ERR_SERIALIZE,
        };
        emit_cstring(json, out_result_ptr)
    }));
    result.unwrap_or(ERR_PANIC)
}

// ---------------------------------------------------------------------
// ctx_contract_extract_references
// ---------------------------------------------------------------------

/// Extract every reference from `response`. Writes a JSON array
/// (possibly empty) into `*out_refs_ptr`.
///
/// # Safety
/// See module-level docs.
#[no_mangle]
pub unsafe extern "C" fn ctx_contract_extract_references(
    response_ptr: *const u8,
    response_len: usize,
    out_refs_ptr: *mut *mut c_char,
) -> c_int {
    let result = catch_unwind(AssertUnwindSafe(|| {
        if out_refs_ptr.is_null() {
            return ERR_NULL_PTR;
        }
        *out_refs_ptr = ptr::null_mut();

        let bytes = match slice_from_raw(response_ptr, response_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let refs = extract_references(bytes);
        let json = match serde_json::to_string(&refs) {
            Ok(s) => s,
            Err(_) => return ERR_SERIALIZE,
        };
        emit_cstring(json, out_refs_ptr)
    }));
    result.unwrap_or(ERR_PANIC)
}

// ---------------------------------------------------------------------
// ctx_contract_parse_from_pack
// ---------------------------------------------------------------------

/// Search `pack` for an embedded contract block. On success writes the
/// contract JSON into `*out_contract_ptr` and sets `*out_found = 1`. If
/// no contract is embedded, `*out_found = 0`, `*out_contract_ptr` is
/// null, and the function still returns `ERR_OK`.
///
/// # Safety
/// See module-level docs.
#[no_mangle]
pub unsafe extern "C" fn ctx_contract_parse_from_pack(
    pack_ptr: *const u8,
    pack_len: usize,
    out_contract_ptr: *mut *mut c_char,
    out_found: *mut c_int,
) -> c_int {
    let result = catch_unwind(AssertUnwindSafe(|| {
        if out_contract_ptr.is_null() || out_found.is_null() {
            return ERR_NULL_PTR;
        }
        *out_contract_ptr = ptr::null_mut();
        *out_found = 0;

        let bytes = match slice_from_raw(pack_ptr, pack_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        match parse_from_pack(bytes) {
            None => ERR_OK,
            Some(c) => {
                let json = match serde_json::to_string(&c) {
                    Ok(s) => s,
                    Err(_) => return ERR_SERIALIZE,
                };
                let rc = emit_cstring(json, out_contract_ptr);
                if rc == ERR_OK {
                    *out_found = 1;
                }
                rc
            }
        }
    }));
    result.unwrap_or(ERR_PANIC)
}

// ---------------------------------------------------------------------
// ctx_contract_strip_block
// ---------------------------------------------------------------------

/// Strip the embedded contract block from `pack`. Writes a raw byte
/// buffer (NOT null-terminated) into `*out_stripped_ptr` with the byte
/// count in `*out_len`. When `*out_len == 0`, `*out_stripped_ptr` is
/// null and no free is required.
///
/// # Safety
/// See module-level docs.
#[no_mangle]
pub unsafe extern "C" fn ctx_contract_strip_block(
    pack_ptr: *const u8,
    pack_len: usize,
    out_stripped_ptr: *mut *mut u8,
    out_len: *mut usize,
) -> c_int {
    let result = catch_unwind(AssertUnwindSafe(|| {
        if out_stripped_ptr.is_null() || out_len.is_null() {
            return ERR_NULL_PTR;
        }
        *out_stripped_ptr = ptr::null_mut();
        *out_len = 0;

        let bytes = match slice_from_raw(pack_ptr, pack_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let stripped = strip_contract_block(bytes);
        if stripped.is_empty() {
            return ERR_OK;
        }
        // Convert Vec<u8> → raw (ptr, len). Drop excess capacity first
        // so the caller's free can pass the same `len` that we set, and
        // the dealloc layout in `ctx_contract_free_buffer` matches.
        let mut boxed = stripped.into_boxed_slice();
        let ptr = boxed.as_mut_ptr();
        let len = boxed.len();
        std::mem::forget(boxed);
        *out_stripped_ptr = ptr;
        *out_len = len;
        ERR_OK
    }));
    result.unwrap_or(ERR_PANIC)
}

// ---------------------------------------------------------------------
// Free helpers
// ---------------------------------------------------------------------

/// Free a string previously returned from one of the `ctx_contract_*`
/// functions via `out_*_ptr`. Safe to call on a null pointer (no-op).
///
/// # Safety
/// `s` must either be null or a pointer originally returned by this
/// crate's FFI. Calling on any other pointer is undefined behaviour.
#[no_mangle]
pub unsafe extern "C" fn ctx_contract_free_string(s: *mut c_char) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !s.is_null() {
            // Reclaim ownership and drop. `from_raw` is the matching
            // half of `CString::into_raw`.
            drop(CString::from_raw(s));
        }
    }));
}

/// Free a byte buffer previously returned by `ctx_contract_strip_block`.
/// Safe to call when `buf` is null AND `len == 0`. Calling with
/// mismatched `len` is undefined behaviour.
///
/// # Safety
/// `(buf, len)` must be the exact pair returned by a previous
/// `ctx_contract_strip_block` call.
#[no_mangle]
pub unsafe extern "C" fn ctx_contract_free_buffer(buf: *mut u8, len: usize) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if buf.is_null() || len == 0 {
            return;
        }
        // Recreate the `Box<[u8]>` we forgot and drop it.
        let slice = slice::from_raw_parts_mut(buf, len);
        drop(Box::from_raw(slice as *mut [u8]));
    }));
}

// ---------------------------------------------------------------------
// ctx_contract_version
// ---------------------------------------------------------------------

/// Returns a pointer to a `'static` NUL-terminated C string carrying
/// the crate's version banner. The pointer is valid for the lifetime
/// of the loaded library; the caller MUST NOT free it.
#[no_mangle]
pub extern "C" fn ctx_contract_version() -> *const c_char {
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
        unsafe { ctx_contract_free_string(p) };
        s
    }

    #[test]
    fn version_round_trips() {
        let p = ctx_contract_version();
        assert!(!p.is_null());
        let s = unsafe { CStr::from_ptr(p) }.to_str().unwrap();
        assert_eq!(s, "ctx-contract 0.1.0");
    }

    #[test]
    fn verify_happy_path() {
        let contract = br#"{
            "schema_version": 1,
            "created": "2026-05-29T00:00:00Z",
            "files": [
                {"path": "a.go", "line_start": 1, "line_end": 10, "sha256": "abc", "symbols": ["A"]}
            ]
        }"#;
        let response = b"see a.go for details";
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe {
            ctx_contract_verify(
                contract.as_ptr(),
                contract.len(),
                response.as_ptr(),
                response.len(),
                ptr::null(),
                0,
                &mut out,
            )
        };
        assert_eq!(rc, ERR_OK);
        let json = cstr_into_string(out);
        assert!(json.contains("\"exit_code\":0"), "json = {json}");
        assert!(json.contains("\"references_found\":1"), "json = {json}");
    }

    #[test]
    fn verify_rejects_null_out_ptr() {
        let contract = b"{}";
        let response = b"";
        let rc = unsafe {
            ctx_contract_verify(
                contract.as_ptr(),
                contract.len(),
                response.as_ptr(),
                response.len(),
                ptr::null(),
                0,
                ptr::null_mut(),
            )
        };
        assert_eq!(rc, ERR_NULL_PTR);
    }

    #[test]
    fn verify_rejects_oversize_input() {
        let mut out: *mut c_char = ptr::null_mut();
        // Pass a bogus pointer with an oversized len — the size check
        // must trip before we dereference anything.
        let rc = unsafe {
            ctx_contract_verify(
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
    fn verify_bad_json_returns_error() {
        let mut out: *mut c_char = ptr::null_mut();
        let bad = b"not json";
        let rc = unsafe {
            ctx_contract_verify(
                bad.as_ptr(),
                bad.len(),
                b"".as_ptr(),
                0,
                ptr::null(),
                0,
                &mut out,
            )
        };
        assert_eq!(rc, ERR_BAD_JSON);
    }

    #[test]
    fn extract_references_returns_json_array() {
        let response = b"look at foo.go and `Bar`";
        let mut out: *mut c_char = ptr::null_mut();
        let rc =
            unsafe { ctx_contract_extract_references(response.as_ptr(), response.len(), &mut out) };
        assert_eq!(rc, ERR_OK);
        let json = cstr_into_string(out);
        assert!(json.starts_with('['));
        assert!(json.contains("\"foo.go\""));
    }

    #[test]
    fn extract_references_empty_input() {
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe { ctx_contract_extract_references(ptr::null(), 0, &mut out) };
        assert_eq!(rc, ERR_OK);
        let json = cstr_into_string(out);
        assert_eq!(json, "[]");
    }

    #[test]
    fn parse_from_pack_finds_embedded_contract() {
        let pack =
            b"prelude\n<!-- ctx:contract v1\n{\"schema_version\":1,\"created\":\"x\",\"files\":[]}\n-->\nepilogue";
        let mut out: *mut c_char = ptr::null_mut();
        let mut found: c_int = 0;
        let rc = unsafe {
            ctx_contract_parse_from_pack(pack.as_ptr(), pack.len(), &mut out, &mut found)
        };
        assert_eq!(rc, ERR_OK);
        assert_eq!(found, 1);
        let json = cstr_into_string(out);
        assert!(json.contains("\"schema_version\":1"));
    }

    #[test]
    fn parse_from_pack_no_block_sets_found_zero() {
        let pack = b"plain pack body with no contract";
        let mut out: *mut c_char = ptr::null_mut();
        let mut found: c_int = 7;
        let rc = unsafe {
            ctx_contract_parse_from_pack(pack.as_ptr(), pack.len(), &mut out, &mut found)
        };
        assert_eq!(rc, ERR_OK);
        assert_eq!(found, 0);
        assert!(out.is_null());
    }

    #[test]
    fn strip_block_returns_buffer_and_frees() {
        let pack =
            b"prelude\n<!-- ctx:contract v1\n{\"schema_version\":1,\"created\":\"x\",\"files\":[]}\n-->\nepilogue";
        let mut buf: *mut u8 = ptr::null_mut();
        let mut len: usize = 0;
        let rc =
            unsafe { ctx_contract_strip_block(pack.as_ptr(), pack.len(), &mut buf, &mut len) };
        assert_eq!(rc, ERR_OK);
        assert!(!buf.is_null());
        let slice = unsafe { slice::from_raw_parts(buf, len) };
        let s = std::str::from_utf8(slice).unwrap();
        assert!(s.contains("prelude"));
        assert!(s.contains("epilogue"));
        assert!(!s.contains("ctx:contract"));
        unsafe { ctx_contract_free_buffer(buf, len) };
    }

    #[test]
    fn strip_block_no_op_returns_zero_len() {
        // strip on input with no block returns the body verbatim. If
        // the body is non-empty we still allocate; only an empty
        // stripped result skips allocation.
        let pack = b"";
        let mut buf: *mut u8 = ptr::null_mut();
        let mut len: usize = 0;
        let rc =
            unsafe { ctx_contract_strip_block(pack.as_ptr(), pack.len(), &mut buf, &mut len) };
        assert_eq!(rc, ERR_OK);
        assert_eq!(len, 0);
        assert!(buf.is_null());
        // Calling free on the null/0 pair must be a no-op (not UB).
        unsafe { ctx_contract_free_buffer(buf, len) };
    }

    #[test]
    fn free_string_on_null_is_safe() {
        unsafe { ctx_contract_free_string(ptr::null_mut()) };
    }
}
