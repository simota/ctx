// crates/ctx-pack/src/ffi.rs
//
// Phase 4 Tier 2 #2 FFI surface for ctx-pack. The pack module is
// scope-split:
//
//   * relevance  → SESSIONED. The pack planner scores hundreds-to-
//                  thousands of files against the SAME goal/budget
//                  per invocation. Goal-keyword extraction + the
//                  alias table fire once on session_open; subsequent
//                  session_score calls reuse the precomputed
//                  RelevanceContext.
//   * diff       → STATELESS. Fired once per `ctx pack diff`.
//   * redact     → STATELESS. Per-file call but the corpus state is
//                  the warning list, which differs per file.
//   * from_where → STATELESS. Single parse per command.
//   * preset     → STATELESS. Tiny pure data.
//
// FUNCTION SURFACE
// ================
//   Sessioned relevance:
//     ctx_pack_relevance_session_open(goal_ptr, goal_len, budget,
//                                      out_handle) -> i32
//     ctx_pack_relevance_session_score(handle, file_json, file_len,
//                                       token_count, out_result) -> i32
//     ctx_pack_relevance_session_score_corpus(handle, files_json,
//                                              files_len, tokens_json,
//                                              tokens_len, out_result) -> i32
//     ctx_pack_relevance_session_rank(handle, files_json, files_len,
//                                      tokens_json, tokens_len, n,
//                                      out_result) -> i32
//     ctx_pack_relevance_session_close(handle) -> i32
//
//   Stateless (1-shot):
//     ctx_pack_relevance_score(file_json, file_len, goal_ptr,
//                               goal_len, token_count, budget,
//                               out_result) -> i32
//     ctx_pack_diff(diffs_json, diffs_len, opts_json, opts_len,
//                   out_result) -> i32
//     ctx_pack_redact(data_ptr, data_len, warnings_json, warnings_len,
//                     out_data, out_len) -> i32
//     ctx_pack_from_where(data_ptr, data_len, out_result) -> i32
//     ctx_pack_preset(name_ptr, name_len, out_result) -> i32
//     ctx_pack_free_string(s)
//     ctx_pack_free_bytes(buf, len)
//     ctx_pack_version() -> *const c_char

use std::ffi::{c_char, c_int, c_void, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::slice;

use crate::diff::render as diff_render;
use crate::from_where::parse as from_where_parse;
use crate::preset::apply_preset;
use crate::redact::redact_lines;
use crate::relevance::session::RelevanceSession;
use crate::relevance::score_relevance;
use crate::types::{DiffEntry, DiffOptions, FileInput, WarningInput};

const MAX_INPUT_BYTES: usize = 256 * 1024 * 1024;

const ERR_OK: c_int = 0;
const ERR_NULL_PTR: c_int = -1;
const ERR_TOO_LARGE: c_int = -2;
const ERR_BAD_JSON: c_int = -3;
const ERR_SERIALIZE: c_int = -4;
const ERR_PRESET: c_int = -5;
const ERR_FROM_WHERE: c_int = -6;
const ERR_BAD_HANDLE: c_int = -10;
const ERR_PANIC: c_int = -99;

static VERSION_C: once_cell::sync::Lazy<CString> =
    once_cell::sync::Lazy::new(|| CString::new("ctx-pack 0.1.0").expect("version cstr"));

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

fn emit_ok_value(value: impl serde::Serialize, out: *mut *mut c_char) -> c_int {
    let body = match serde_json::to_string(&value) {
        Ok(s) => s,
        Err(_) => return ERR_SERIALIZE,
    };
    emit_cstring(body, out)
}

fn decode_utf8(bytes: &[u8]) -> Result<&str, c_int> {
    std::str::from_utf8(bytes).map_err(|_| ERR_BAD_JSON)
}

// =================================================================
// Session-backed relevance API
// =================================================================

/// # Safety
/// `goal_ptr` valid for `goal_len`; `out_handle` writable.
#[no_mangle]
pub unsafe extern "C" fn ctx_pack_relevance_session_open(
    goal_ptr: *const u8,
    goal_len: usize,
    budget: i64,
    out_handle: *mut *mut c_void,
) -> c_int {
    let r = catch_unwind(AssertUnwindSafe(|| {
        if out_handle.is_null() {
            return ERR_NULL_PTR;
        }
        *out_handle = ptr::null_mut();
        let goal_bytes = match slice_from_raw(goal_ptr, goal_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let goal = match decode_utf8(goal_bytes) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let session = Box::new(RelevanceSession::new(goal, budget));
        *out_handle = Box::into_raw(session) as *mut c_void;
        ERR_OK
    }));
    r.unwrap_or(ERR_PANIC)
}

/// # Safety
/// `handle` must come from session_open and not yet be closed.
#[no_mangle]
pub unsafe extern "C" fn ctx_pack_relevance_session_score(
    handle: *mut c_void,
    file_json_ptr: *const u8,
    file_json_len: usize,
    token_count: i64,
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
        let file_bytes = match slice_from_raw(file_json_ptr, file_json_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let file: FileInput = match serde_json::from_slice(file_bytes) {
            Ok(v) => v,
            Err(_) => return ERR_BAD_JSON,
        };
        let session = &*(handle as *const RelevanceSession);
        let result = session.score_file(&file, token_count);
        emit_ok_value(result, out_result_ptr)
    }));
    r.unwrap_or(ERR_PANIC)
}

/// Score every file in `files_json` against the session's goal and
/// return the result array (one entry per input file, in order).
///
/// # Safety
/// See module-level docs.
#[no_mangle]
pub unsafe extern "C" fn ctx_pack_relevance_session_score_corpus(
    handle: *mut c_void,
    files_json_ptr: *const u8,
    files_json_len: usize,
    tokens_json_ptr: *const u8,
    tokens_json_len: usize,
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
        let files_bytes = match slice_from_raw(files_json_ptr, files_json_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let tokens_bytes = match slice_from_raw(tokens_json_ptr, tokens_json_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let files: Vec<FileInput> = match serde_json::from_slice(files_bytes) {
            Ok(v) => v,
            Err(_) => return ERR_BAD_JSON,
        };
        let tokens: Vec<i64> = if tokens_bytes.is_empty() {
            Vec::new()
        } else {
            match serde_json::from_slice(tokens_bytes) {
                Ok(v) => v,
                Err(_) => return ERR_BAD_JSON,
            }
        };
        let session = &*(handle as *const RelevanceSession);
        // Build a temp scratch context using session ctx + provided
        // files; this is what the high-throughput Go loop hits.
        let token_slice: Option<&[i64]> = if tokens.is_empty() {
            None
        } else {
            Some(&tokens)
        };
        let mut out = Vec::with_capacity(files.len());
        for (i, fi) in files.iter().enumerate() {
            let tc = token_slice.and_then(|t| t.get(i).copied()).unwrap_or(fi.tokens);
            out.push(session.score_file(fi, tc));
        }
        emit_ok_value(out, out_result_ptr)
    }));
    r.unwrap_or(ERR_PANIC)
}

/// Rank top-N files by relevance against the session's goal.
///
/// # Safety
/// See module-level docs.
#[no_mangle]
pub unsafe extern "C" fn ctx_pack_relevance_session_rank(
    handle: *mut c_void,
    files_json_ptr: *const u8,
    files_json_len: usize,
    tokens_json_ptr: *const u8,
    tokens_json_len: usize,
    n: c_int,
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
        let files_bytes = match slice_from_raw(files_json_ptr, files_json_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let tokens_bytes = match slice_from_raw(tokens_json_ptr, tokens_json_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let files: Vec<FileInput> = match serde_json::from_slice(files_bytes) {
            Ok(v) => v,
            Err(_) => return ERR_BAD_JSON,
        };
        let tokens: Vec<i64> = if tokens_bytes.is_empty() {
            Vec::new()
        } else {
            match serde_json::from_slice(tokens_bytes) {
                Ok(v) => v,
                Err(_) => return ERR_BAD_JSON,
            }
        };
        let session = &*(handle as *const RelevanceSession);
        let limit = if n <= 0 { files.len() } else { n as usize };
        let ranked = crate::relevance::rank_top_n(
            &crate::relevance::RelevanceContext {
                goal_keywords: session.goal_keywords().to_vec(),
                goal: String::new(),
                budget: session.budget(),
            },
            &files,
            if tokens.is_empty() { None } else { Some(&tokens) },
            limit,
        );
        // Encode as [{"index": i, "result": {...}}] so the Go side
        // can map back to the original FileInput slice.
        #[derive(serde::Serialize)]
        struct Out<'a> {
            index: usize,
            #[serde(flatten)]
            result: &'a crate::types::RelevanceResult,
        }
        let view: Vec<Out<'_>> = ranked
            .iter()
            .map(|(i, r)| Out { index: *i, result: r })
            .collect();
        emit_ok_value(view, out_result_ptr)
    }));
    r.unwrap_or(ERR_PANIC)
}

/// Close a session and reclaim its memory.
///
/// # Safety
/// `handle` must either be null or a value returned by
/// `ctx_pack_relevance_session_open` that has not been closed yet.
#[no_mangle]
pub unsafe extern "C" fn ctx_pack_relevance_session_close(handle: *mut c_void) -> c_int {
    let r = catch_unwind(AssertUnwindSafe(|| {
        if handle.is_null() {
            return ERR_NULL_PTR;
        }
        drop(Box::from_raw(handle as *mut RelevanceSession));
        ERR_OK
    }));
    r.unwrap_or(ERR_PANIC)
}

// =================================================================
// Stateless batch API
// =================================================================

/// # Safety
/// See module-level docs.
#[no_mangle]
pub unsafe extern "C" fn ctx_pack_relevance_score(
    file_json_ptr: *const u8,
    file_json_len: usize,
    goal_ptr: *const u8,
    goal_len: usize,
    token_count: i64,
    budget: i64,
    out_result_ptr: *mut *mut c_char,
) -> c_int {
    let r = catch_unwind(AssertUnwindSafe(|| {
        if out_result_ptr.is_null() {
            return ERR_NULL_PTR;
        }
        *out_result_ptr = ptr::null_mut();
        let file_bytes = match slice_from_raw(file_json_ptr, file_json_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let goal_bytes = match slice_from_raw(goal_ptr, goal_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let file: FileInput = match serde_json::from_slice(file_bytes) {
            Ok(v) => v,
            Err(_) => return ERR_BAD_JSON,
        };
        let goal = match decode_utf8(goal_bytes) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let result = score_relevance(&file, goal, token_count, budget);
        emit_ok_value(result, out_result_ptr)
    }));
    r.unwrap_or(ERR_PANIC)
}

/// # Safety
/// See module-level docs.
#[no_mangle]
pub unsafe extern "C" fn ctx_pack_diff(
    diffs_json_ptr: *const u8,
    diffs_json_len: usize,
    opts_json_ptr: *const u8,
    opts_json_len: usize,
    out_result_ptr: *mut *mut c_char,
) -> c_int {
    let r = catch_unwind(AssertUnwindSafe(|| {
        if out_result_ptr.is_null() {
            return ERR_NULL_PTR;
        }
        *out_result_ptr = ptr::null_mut();
        let diffs_bytes = match slice_from_raw(diffs_json_ptr, diffs_json_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let opts_bytes = match slice_from_raw(opts_json_ptr, opts_json_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let diffs: Vec<DiffEntry> = match serde_json::from_slice(diffs_bytes) {
            Ok(v) => v,
            Err(_) => return ERR_BAD_JSON,
        };
        let opts: DiffOptions = if opts_bytes.is_empty() {
            DiffOptions::default()
        } else {
            match serde_json::from_slice(opts_bytes) {
                Ok(v) => v,
                Err(_) => return ERR_BAD_JSON,
            }
        };
        let out = diff_render(&diffs, &opts);
        emit_cstring(out, out_result_ptr)
    }));
    r.unwrap_or(ERR_PANIC)
}

/// # Safety
/// See module-level docs.
#[no_mangle]
pub unsafe extern "C" fn ctx_pack_redact(
    data_ptr: *const u8,
    data_len: usize,
    warnings_json_ptr: *const u8,
    warnings_json_len: usize,
    out_buf: *mut *mut u8,
    out_len: *mut usize,
) -> c_int {
    let r = catch_unwind(AssertUnwindSafe(|| {
        if out_buf.is_null() || out_len.is_null() {
            return ERR_NULL_PTR;
        }
        *out_buf = ptr::null_mut();
        *out_len = 0;
        let data = match slice_from_raw(data_ptr, data_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let warnings_bytes = match slice_from_raw(warnings_json_ptr, warnings_json_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let warnings: Vec<WarningInput> = if warnings_bytes.is_empty() {
            Vec::new()
        } else {
            match serde_json::from_slice(warnings_bytes) {
                Ok(v) => v,
                Err(_) => return ERR_BAD_JSON,
            }
        };
        let redacted = redact_lines(data, &warnings);
        let len = redacted.len();
        let mut boxed = redacted.into_boxed_slice();
        let ptr = boxed.as_mut_ptr();
        std::mem::forget(boxed);
        *out_buf = ptr;
        *out_len = len;
        ERR_OK
    }));
    r.unwrap_or(ERR_PANIC)
}

/// # Safety
/// See module-level docs.
#[no_mangle]
pub unsafe extern "C" fn ctx_pack_from_where(
    data_ptr: *const u8,
    data_len: usize,
    out_result_ptr: *mut *mut c_char,
) -> c_int {
    let r = catch_unwind(AssertUnwindSafe(|| {
        if out_result_ptr.is_null() {
            return ERR_NULL_PTR;
        }
        *out_result_ptr = ptr::null_mut();
        let data = match slice_from_raw(data_ptr, data_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        match from_where_parse(data) {
            Ok(paths) => {
                let body = serde_json::json!({"ok": true, "paths": paths});
                if emit_ok_value(body, out_result_ptr) == ERR_OK {
                    ERR_OK
                } else {
                    ERR_SERIALIZE
                }
            }
            Err(e) => {
                let body = serde_json::json!({"ok": false, "error": e.to_string()});
                let rc = emit_ok_value(body, out_result_ptr);
                if rc == ERR_OK {
                    ERR_FROM_WHERE
                } else {
                    rc
                }
            }
        }
    }));
    r.unwrap_or(ERR_PANIC)
}

/// # Safety
/// See module-level docs.
#[no_mangle]
pub unsafe extern "C" fn ctx_pack_preset(
    name_ptr: *const u8,
    name_len: usize,
    out_result_ptr: *mut *mut c_char,
) -> c_int {
    let r = catch_unwind(AssertUnwindSafe(|| {
        if out_result_ptr.is_null() {
            return ERR_NULL_PTR;
        }
        *out_result_ptr = ptr::null_mut();
        let name_bytes = match slice_from_raw(name_ptr, name_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let name = match decode_utf8(name_bytes) {
            Ok(s) => s,
            Err(e) => return e,
        };
        match apply_preset(name) {
            Ok(patch) => {
                let body = serde_json::json!({"ok": true, "patch": patch});
                if emit_ok_value(body, out_result_ptr) == ERR_OK {
                    ERR_OK
                } else {
                    ERR_SERIALIZE
                }
            }
            Err(e) => {
                let body = serde_json::json!({"ok": false, "error": e.to_string()});
                let rc = emit_ok_value(body, out_result_ptr);
                if rc == ERR_OK {
                    ERR_PRESET
                } else {
                    rc
                }
            }
        }
    }));
    r.unwrap_or(ERR_PANIC)
}

/// # Safety
/// `s` must come from a prior ctx_pack_* call or be null.
#[no_mangle]
pub unsafe extern "C" fn ctx_pack_free_string(s: *mut c_char) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !s.is_null() {
            drop(CString::from_raw(s));
        }
    }));
}

/// Free a buffer allocated by `ctx_pack_redact`.
///
/// # Safety
/// `buf` must come from `ctx_pack_redact` or be null. `len` must match
/// the length returned by that call.
#[no_mangle]
pub unsafe extern "C" fn ctx_pack_free_bytes(buf: *mut u8, len: usize) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !buf.is_null() && len > 0 {
            let _ = Box::from_raw(slice::from_raw_parts_mut(buf, len));
        }
    }));
}

#[no_mangle]
pub extern "C" fn ctx_pack_version() -> *const c_char {
    VERSION_C.as_ptr()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;

    #[test]
    fn version_round_trips() {
        let p = ctx_pack_version();
        assert!(!p.is_null());
        let s = unsafe { CStr::from_ptr(p) }.to_str().unwrap();
        assert_eq!(s, "ctx-pack 0.1.0");
    }

    #[test]
    fn relevance_score_ffi() {
        let file = r#"{"path":"src/auth/login.ts","abs_path":"","is_dir":false,"tokens":100,"role":"core","metadata":{"size":100,"tokens_est":100,"role":"core","symbols":[{"name":"validateLoginSession","kind":"function","line":1}]},"content_head":[]}"#;
        let goal = "ログイン認証";
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe {
            ctx_pack_relevance_score(
                file.as_ptr(),
                file.len(),
                goal.as_ptr(),
                goal.len(),
                100,
                30000,
                &mut out,
            )
        };
        assert_eq!(rc, ERR_OK, "rc={rc}");
        let s = unsafe { CStr::from_ptr(out) }.to_str().unwrap().to_owned();
        unsafe { ctx_pack_free_string(out) };
        assert!(s.contains("\"tier\":\"High\""));
    }

    #[test]
    fn session_open_close_no_leak() {
        for _ in 0..500 {
            let goal = "認証";
            let mut h: *mut c_void = ptr::null_mut();
            let rc = unsafe {
                ctx_pack_relevance_session_open(goal.as_ptr(), goal.len(), 1000, &mut h)
            };
            assert_eq!(rc, ERR_OK);
            let rc = unsafe { ctx_pack_relevance_session_close(h) };
            assert_eq!(rc, ERR_OK);
        }
    }

    #[test]
    fn session_score_matches_stateless_ffi() {
        let goal = "ログイン認証";
        let budget: i64 = 30000;
        let file = r#"{"path":"src/auth/login.ts","abs_path":"","is_dir":false,"tokens":100,"role":"core","metadata":{"size":100,"tokens_est":100,"role":"core","symbols":[{"name":"validateLoginSession","kind":"function","line":1}]},"content_head":[]}"#;

        let mut h: *mut c_void = ptr::null_mut();
        let rc = unsafe {
            ctx_pack_relevance_session_open(goal.as_ptr(), goal.len(), budget, &mut h)
        };
        assert_eq!(rc, ERR_OK);
        let mut s_out: *mut c_char = ptr::null_mut();
        let rc = unsafe {
            ctx_pack_relevance_session_score(h, file.as_ptr(), file.len(), 100, &mut s_out)
        };
        assert_eq!(rc, ERR_OK);
        let sticky = unsafe { CStr::from_ptr(s_out) }.to_str().unwrap().to_owned();
        unsafe { ctx_pack_free_string(s_out) };
        unsafe { ctx_pack_relevance_session_close(h) };

        let mut sl_out: *mut c_char = ptr::null_mut();
        let rc = unsafe {
            ctx_pack_relevance_score(
                file.as_ptr(),
                file.len(),
                goal.as_ptr(),
                goal.len(),
                100,
                budget,
                &mut sl_out,
            )
        };
        assert_eq!(rc, ERR_OK);
        let stateless = unsafe { CStr::from_ptr(sl_out) }.to_str().unwrap().to_owned();
        unsafe { ctx_pack_free_string(sl_out) };

        assert_eq!(sticky, stateless);
    }

    #[test]
    fn diff_ffi_renders() {
        let diffs = r#"[{"path":"a.go","before_content":"old","after_content":"new","before_commit":"abc","after_commit":"def","patch":"","added":false,"deleted":false,"binary":false}]"#;
        let opts = r#"{"layout":"sequential","preset":""}"#;
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe {
            ctx_pack_diff(diffs.as_ptr(), diffs.len(), opts.as_ptr(), opts.len(), &mut out)
        };
        assert_eq!(rc, ERR_OK);
        let s = unsafe { CStr::from_ptr(out) }.to_str().unwrap().to_owned();
        unsafe { ctx_pack_free_string(out) };
        assert!(s.contains("**Before**"));
        assert!(s.contains("**After**"));
    }

    #[test]
    fn redact_ffi_returns_bytes() {
        let data = b"a\nSECRET=hunter2\nc";
        let warnings = r#"[{"path":"","line":2,"kind":"env"}]"#;
        let mut buf: *mut u8 = ptr::null_mut();
        let mut len: usize = 0;
        let rc = unsafe {
            ctx_pack_redact(
                data.as_ptr(),
                data.len(),
                warnings.as_ptr(),
                warnings.len(),
                &mut buf,
                &mut len,
            )
        };
        assert_eq!(rc, ERR_OK);
        assert!(!buf.is_null());
        let s = unsafe { String::from_utf8_lossy(slice::from_raw_parts(buf, len)).to_string() };
        unsafe { ctx_pack_free_bytes(buf, len) };
        assert!(s.contains("[REDACTED — kind=env]"));
    }

    #[test]
    fn from_where_ffi_json() {
        let data = br#"[{"path":"a.go","score":0.9}]"#;
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe { ctx_pack_from_where(data.as_ptr(), data.len(), &mut out) };
        assert_eq!(rc, ERR_OK);
        let s = unsafe { CStr::from_ptr(out) }.to_str().unwrap().to_owned();
        unsafe { ctx_pack_free_string(out) };
        assert!(s.contains("\"a.go\""));
    }

    #[test]
    fn from_where_empty_errors_with_envelope() {
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe { ctx_pack_from_where(b"".as_ptr(), 0, &mut out) };
        assert_eq!(rc, ERR_FROM_WHERE);
        let s = unsafe { CStr::from_ptr(out) }.to_str().unwrap().to_owned();
        unsafe { ctx_pack_free_string(out) };
        assert!(s.contains("\"ok\":false"));
    }

    #[test]
    fn preset_ffi_blog() {
        let name = "blog";
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe { ctx_pack_preset(name.as_ptr(), name.len(), &mut out) };
        assert_eq!(rc, ERR_OK);
        let s = unsafe { CStr::from_ptr(out) }.to_str().unwrap().to_owned();
        unsafe { ctx_pack_free_string(out) };
        assert!(s.contains("\"format\":\"markdown\""), "json: {s}");
        assert!(s.contains("\"frontmatter\":\"mdx\""), "json: {s}");
    }

    #[test]
    fn preset_ffi_unknown_errors() {
        let name = "bogus";
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe { ctx_pack_preset(name.as_ptr(), name.len(), &mut out) };
        assert_eq!(rc, ERR_PRESET);
        let s = unsafe { CStr::from_ptr(out) }.to_str().unwrap().to_owned();
        unsafe { ctx_pack_free_string(out) };
        assert!(s.contains("\"ok\":false"));
    }
}
