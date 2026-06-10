// crates/ctx-braid/src/ffi.rs
//
// Phase 4 Tier 2 #1 stateless FFI surface for ctx-braid.
//
// API SHAPE: BATCH stateless (same reasoning as ctx-heatmap). braid's
// pure-compute helpers (Load + Validate + Allocate + MergePaths +
// ShellQuote) each fire once per `ctx braid` invocation; no session
// corpus to amortise.
//
// FUNCTION SURFACE
// ================
//   ctx_braid_load_config(toml_ptr, toml_len, out_json)        -> i32
//   ctx_braid_validate(cfg_json_ptr, cfg_json_len, out_json)   -> i32
//   ctx_braid_allocate(cfg_ptr, cfg_len, budget, out_json)     -> i32
//   ctx_braid_merge_paths(sels_ptr, sels_len, out_json)        -> i32
//   ctx_braid_shell_quote(src_ptr, src_len, out_json)          -> i32
//   ctx_braid_strand_subcommand(src_ptr, src_len, out_cstr)    -> i32
//
//   ctx_braid_free_string(s)
//   ctx_braid_version() -> *const c_char
//
// All JSON shapes follow types.rs serde derives. Errors are encoded as
// integer return codes matching the ctx-heatmap convention.

use std::ffi::{c_char, c_int, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::slice;

use crate::allocate::allocate;
use crate::config::{load, validate, ConfigError};
use crate::merge::merge_paths;
use crate::policy::strand_subcommand;
use crate::shellquote::shell_split;
use crate::types::{Config, StrandSelection};

const MAX_INPUT_BYTES: usize = 256 * 1024 * 1024;

const ERR_OK: c_int = 0;
const ERR_NULL_PTR: c_int = -1;
const ERR_TOO_LARGE: c_int = -2;
const ERR_BAD_JSON: c_int = -3;
const ERR_SERIALIZE: c_int = -4;
const ERR_VALIDATION: c_int = -5;
const ERR_SHELL_SPLIT: c_int = -6;
const ERR_PANIC: c_int = -99;

static VERSION_C: once_cell::sync::Lazy<CString> =
    once_cell::sync::Lazy::new(|| CString::new("ctx-braid 0.1.0").expect("version cstr"));

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

/// Internal helper: emit a JSON envelope `{"ok": true, "value": <T>}` or
/// `{"ok": false, "error": "<message>"}`. Callers that want richer
/// structured error info should switch to the dedicated error fields,
/// but for the FFI surface the message-string approach matches what the
/// Go side already does in cli/braid.go (string-match on prefix).
fn emit_ok_value(
    value: impl serde::Serialize,
    out: *mut *mut c_char,
) -> c_int {
    let body = match serde_json::to_string(&value) {
        Ok(s) => s,
        Err(_) => return ERR_SERIALIZE,
    };
    emit_cstring(body, out)
}

fn emit_validation_error(err: &ConfigError, out: *mut *mut c_char) -> c_int {
    let body = serde_json::json!({"ok": false, "error": err.to_string()});
    match serde_json::to_string(&body) {
        Ok(s) => {
            if emit_cstring(s, out) == ERR_OK {
                ERR_VALIDATION
            } else {
                ERR_SERIALIZE
            }
        }
        Err(_) => ERR_SERIALIZE,
    }
}

/// # Safety
/// `toml_ptr` must be valid for `toml_len` bytes (zero length permitted
/// with NULL pointer). `out_result_ptr` must be a valid writable
/// pointer to a `*mut c_char`.
#[no_mangle]
pub unsafe extern "C" fn ctx_braid_load_config(
    toml_ptr: *const u8,
    toml_len: usize,
    out_result_ptr: *mut *mut c_char,
) -> c_int {
    let r = catch_unwind(AssertUnwindSafe(|| {
        if out_result_ptr.is_null() {
            return ERR_NULL_PTR;
        }
        *out_result_ptr = ptr::null_mut();
        let bytes = match slice_from_raw(toml_ptr, toml_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        match load(bytes) {
            Ok(cfg) => emit_ok_value(cfg, out_result_ptr),
            Err(e) => emit_validation_error(&e, out_result_ptr),
        }
    }));
    r.unwrap_or(ERR_PANIC)
}

/// # Safety
/// See module-level docs.
#[no_mangle]
pub unsafe extern "C" fn ctx_braid_validate(
    cfg_json_ptr: *const u8,
    cfg_json_len: usize,
    out_result_ptr: *mut *mut c_char,
) -> c_int {
    let r = catch_unwind(AssertUnwindSafe(|| {
        if out_result_ptr.is_null() {
            return ERR_NULL_PTR;
        }
        *out_result_ptr = ptr::null_mut();
        let bytes = match slice_from_raw(cfg_json_ptr, cfg_json_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let mut cfg: Config = match serde_json::from_slice(bytes) {
            Ok(v) => v,
            Err(_) => return ERR_BAD_JSON,
        };
        match validate(&mut cfg) {
            Ok(()) => emit_ok_value(cfg, out_result_ptr),
            Err(e) => emit_validation_error(&e, out_result_ptr),
        }
    }));
    r.unwrap_or(ERR_PANIC)
}

/// # Safety
/// See module-level docs.
#[no_mangle]
pub unsafe extern "C" fn ctx_braid_allocate(
    cfg_json_ptr: *const u8,
    cfg_json_len: usize,
    global_budget: i64,
    out_result_ptr: *mut *mut c_char,
) -> c_int {
    let r = catch_unwind(AssertUnwindSafe(|| {
        if out_result_ptr.is_null() {
            return ERR_NULL_PTR;
        }
        *out_result_ptr = ptr::null_mut();
        let bytes = match slice_from_raw(cfg_json_ptr, cfg_json_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let cfg: Config = match serde_json::from_slice(bytes) {
            Ok(v) => v,
            Err(_) => return ERR_BAD_JSON,
        };
        let out = allocate(&cfg, global_budget);
        let body = serde_json::json!({
            "allocations": out.allocations,
            "warning": out.warning,
        });
        emit_ok_value(body, out_result_ptr)
    }));
    r.unwrap_or(ERR_PANIC)
}

/// # Safety
/// See module-level docs.
#[no_mangle]
pub unsafe extern "C" fn ctx_braid_merge_paths(
    sels_json_ptr: *const u8,
    sels_json_len: usize,
    out_result_ptr: *mut *mut c_char,
) -> c_int {
    let r = catch_unwind(AssertUnwindSafe(|| {
        if out_result_ptr.is_null() {
            return ERR_NULL_PTR;
        }
        *out_result_ptr = ptr::null_mut();
        let bytes = match slice_from_raw(sels_json_ptr, sels_json_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let sels: Vec<StrandSelection> = match serde_json::from_slice(bytes) {
            Ok(v) => v,
            Err(_) => return ERR_BAD_JSON,
        };
        let merged = merge_paths(&sels);
        emit_ok_value(merged, out_result_ptr)
    }));
    r.unwrap_or(ERR_PANIC)
}

/// # Safety
/// See module-level docs.
#[no_mangle]
pub unsafe extern "C" fn ctx_braid_shell_quote(
    src_ptr: *const u8,
    src_len: usize,
    out_result_ptr: *mut *mut c_char,
) -> c_int {
    let r = catch_unwind(AssertUnwindSafe(|| {
        if out_result_ptr.is_null() {
            return ERR_NULL_PTR;
        }
        *out_result_ptr = ptr::null_mut();
        let bytes = match slice_from_raw(src_ptr, src_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let src = match std::str::from_utf8(bytes) {
            Ok(s) => s,
            Err(_) => return ERR_BAD_JSON,
        };
        match shell_split(src) {
            Ok(tokens) => emit_ok_value(tokens, out_result_ptr),
            Err(e) => {
                let body =
                    serde_json::json!({"ok": false, "error": format!("braid: {e}")});
                let s = match serde_json::to_string(&body) {
                    Ok(s) => s,
                    Err(_) => return ERR_SERIALIZE,
                };
                if emit_cstring(s, out_result_ptr) == ERR_OK {
                    ERR_SHELL_SPLIT
                } else {
                    ERR_SERIALIZE
                }
            }
        }
    }));
    r.unwrap_or(ERR_PANIC)
}

/// # Safety
/// See module-level docs.
#[no_mangle]
pub unsafe extern "C" fn ctx_braid_strand_subcommand(
    src_ptr: *const u8,
    src_len: usize,
    out_result_ptr: *mut *mut c_char,
) -> c_int {
    let r = catch_unwind(AssertUnwindSafe(|| {
        if out_result_ptr.is_null() {
            return ERR_NULL_PTR;
        }
        *out_result_ptr = ptr::null_mut();
        let bytes = match slice_from_raw(src_ptr, src_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let src = match std::str::from_utf8(bytes) {
            Ok(s) => s,
            Err(_) => return ERR_BAD_JSON,
        };
        let sub = strand_subcommand(src);
        emit_cstring(sub, out_result_ptr)
    }));
    r.unwrap_or(ERR_PANIC)
}

/// # Safety
/// `s` must either be null (no-op) or a pointer returned by a prior
/// successful FFI call.
#[no_mangle]
pub unsafe extern "C" fn ctx_braid_free_string(s: *mut c_char) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !s.is_null() {
            drop(CString::from_raw(s));
        }
    }));
}

/// Returns a pointer to a `'static` NUL-terminated version banner.
#[no_mangle]
pub extern "C" fn ctx_braid_version() -> *const c_char {
    VERSION_C.as_ptr()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;

    #[test]
    fn version_round_trips() {
        let p = ctx_braid_version();
        assert!(!p.is_null());
        let s = unsafe { CStr::from_ptr(p) }.to_str().unwrap();
        assert_eq!(s, "ctx-braid 0.1.0");
    }

    #[test]
    fn load_config_round_trip() {
        let toml = br#"schema_version = 1

[[strand]]
name = "a"
source = "where 'foo'"
share = 0.5
"#;
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe { ctx_braid_load_config(toml.as_ptr(), toml.len(), &mut out) };
        assert_eq!(rc, ERR_OK);
        let s = unsafe { CStr::from_ptr(out) }.to_str().unwrap().to_owned();
        unsafe { ctx_braid_free_string(out) };
        assert!(s.contains("\"Strands\""));
        assert!(s.contains("\"a\""));
    }

    #[test]
    fn load_config_validation_error() {
        let toml = br#"schema_version = 1

[[strand]]
name = "bogus"
source = "unknown --flag"
share = 0.5
"#;
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe { ctx_braid_load_config(toml.as_ptr(), toml.len(), &mut out) };
        assert_eq!(rc, ERR_VALIDATION);
        let s = unsafe { CStr::from_ptr(out) }.to_str().unwrap().to_owned();
        unsafe { ctx_braid_free_string(out) };
        assert!(s.contains("unsupported source"));
    }

    #[test]
    fn allocate_round_trip() {
        let cfg_json =
            br#"{"schema_version":1,"strands":[{"name":"a","source":"where 'x'","share":0.3,"policy":""},{"name":"b","source":"where 'y'","share":0.4,"policy":""}]}"#;
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe {
            ctx_braid_allocate(cfg_json.as_ptr(), cfg_json.len(), 1000, &mut out)
        };
        assert_eq!(rc, ERR_OK);
        let s = unsafe { CStr::from_ptr(out) }.to_str().unwrap().to_owned();
        unsafe { ctx_braid_free_string(out) };
        assert!(s.contains("\"Budget\":300"));
        assert!(s.contains("\"Budget\":400"));
    }

    #[test]
    fn merge_paths_round_trip() {
        let sels = br#"[
            {"Name":"a","Policy":"merge","Paths":["x.go","y.go"]},
            {"Name":"b","Policy":"prefer-newer","Paths":["y.go","z.go"]}
        ]"#;
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe { ctx_braid_merge_paths(sels.as_ptr(), sels.len(), &mut out) };
        assert_eq!(rc, ERR_OK);
        let s = unsafe { CStr::from_ptr(out) }.to_str().unwrap().to_owned();
        unsafe { ctx_braid_free_string(out) };
        assert!(s.contains("\"y.go\""));
        // First entry should be x.go from a; y.go evicted from a by prefer-newer.
        assert!(s.starts_with(r#"[{"path":"x.go","origin":"a"}"#));
    }

    #[test]
    fn shell_quote_round_trip() {
        let src = b"where 'multi word' --limit 5";
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe { ctx_braid_shell_quote(src.as_ptr(), src.len(), &mut out) };
        assert_eq!(rc, ERR_OK);
        let s = unsafe { CStr::from_ptr(out) }.to_str().unwrap().to_owned();
        unsafe { ctx_braid_free_string(out) };
        assert_eq!(s, r#"["where","multi word","--limit","5"]"#);
    }

    #[test]
    fn shell_quote_unclosed_error() {
        let src = b"where 'unclosed";
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe { ctx_braid_shell_quote(src.as_ptr(), src.len(), &mut out) };
        assert_eq!(rc, ERR_SHELL_SPLIT);
        let s = unsafe { CStr::from_ptr(out) }.to_str().unwrap().to_owned();
        unsafe { ctx_braid_free_string(out) };
        assert!(s.contains("unclosed single quote"));
    }

    #[test]
    fn strand_subcommand_round_trip() {
        let src = b"ctx focus Bar --hops 2";
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe {
            ctx_braid_strand_subcommand(src.as_ptr(), src.len(), &mut out)
        };
        assert_eq!(rc, ERR_OK);
        let s = unsafe { CStr::from_ptr(out) }.to_str().unwrap().to_owned();
        unsafe { ctx_braid_free_string(out) };
        assert_eq!(s, "focus");
    }

    #[test]
    fn null_out_pointer_rejected() {
        let rc = unsafe { ctx_braid_allocate(ptr::null(), 0, 0, ptr::null_mut()) };
        assert_eq!(rc, ERR_NULL_PTR);
    }

    #[test]
    fn bad_json_rejected() {
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe { ctx_braid_allocate(b"not-json".as_ptr(), 8, 0, &mut out) };
        assert_eq!(rc, ERR_BAD_JSON);
    }
}
