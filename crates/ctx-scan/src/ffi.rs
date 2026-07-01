// crates/ctx-scan/src/ffi.rs
//
// Phase 1 FFI surface for ctx-scan. Mirrors the pioneer's
// crates/ctx-contract/src/ffi.rs conventions verbatim:
//
//   * All inputs are BORROWED for the call window; we never retain.
//   * Outputs are emitted as heap-owned CStrings via `into_raw`; the
//     caller MUST call `ctx_scan_free_string` exactly once on each
//     non-null out pointer.
//   * Every extern wraps its body in `catch_unwind` — on panic we
//     return -99 and leave out-params untouched.
//
// FUNCTION SURFACE
// ================
//   ctx_scan_file(path_ptr, path_len, opts_json_ptr, opts_json_len,
//                 out_result_ptr) -> i32
//     Reads `path` from disk, scans it under `opts`, returns a JSON
//     array of Warning structs.
//
//   ctx_scan_text(text_ptr, text_len, virtual_path_ptr, virtual_path_len,
//                 opts_json_ptr, opts_json_len, out_result_ptr) -> i32
//     Scans `text` as if it had been read from `virtual_path` (so
//     Warning.path matches what the Go caller expects). Avoids the
//     filesystem round-trip — needed for callers that already have
//     bytes in hand (e.g. internal/pack/redact streaming an
//     in-memory body).
//
//   ctx_scan_files(paths_json_ptr, paths_json_len, opts_json_ptr,
//                  opts_json_len, out_result_ptr) -> i32
//     `paths_json` is a JSON array of UTF-8 strings.
//
//   ctx_scan_free_string(s) — drop a string previously returned.
//   ctx_scan_version() -> *const c_char — static banner.

use std::ffi::{c_char, c_int, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::slice;

use crate::scan::{scan_file_with_options, scan_files_with_options};
use crate::types::{Options, Warning};

const MAX_INPUT_BYTES: usize = 100 * 1024 * 1024;

const ERR_OK: c_int = 0;
const ERR_NULL_PTR: c_int = -1;
const ERR_TOO_LARGE: c_int = -2;
const ERR_BAD_JSON: c_int = -3;
const ERR_SERIALIZE: c_int = -4;
const ERR_IO: c_int = -5;
const ERR_PANIC: c_int = -99;

static VERSION_C: once_cell::sync::Lazy<CString> =
    once_cell::sync::Lazy::new(|| CString::new("ctx-scan 0.1.0").expect("version cstr"));

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
// ctx_scan_file
// ---------------------------------------------------------------------

/// Scan `path` under `opts_json`. On success writes a JSON array of
/// Warning structs into `*out_result_ptr` (always an array, possibly
/// empty `[]`).
///
/// # Safety
/// See module-level docs.
#[no_mangle]
pub unsafe extern "C" fn ctx_scan_file(
    path_ptr: *const u8,
    path_len: usize,
    opts_json_ptr: *const u8,
    opts_json_len: usize,
    out_result_ptr: *mut *mut c_char,
) -> c_int {
    let result = catch_unwind(AssertUnwindSafe(|| {
        if out_result_ptr.is_null() {
            return ERR_NULL_PTR;
        }
        *out_result_ptr = ptr::null_mut();

        let path_bytes = match slice_from_raw(path_ptr, path_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let opts_bytes = match slice_from_raw(opts_json_ptr, opts_json_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let path = match decode_utf8(path_bytes) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let opts = match decode_opts(opts_bytes) {
            Ok(o) => o,
            Err(e) => return e,
        };
        let warnings: Vec<Warning> = match scan_file_with_options(path, &opts) {
            Ok(w) => w,
            Err(_) => return ERR_IO,
        };
        let json = match serde_json::to_string(&warnings) {
            Ok(s) => s,
            Err(_) => return ERR_SERIALIZE,
        };
        emit_cstring(json, out_result_ptr)
    }));
    result.unwrap_or(ERR_PANIC)
}

// ---------------------------------------------------------------------
// ctx_scan_text
// ---------------------------------------------------------------------

/// Scan `text` (already-in-memory bytes) under `opts_json` as if it had
/// been read from `virtual_path`. Useful for callers that hold the
/// content in memory and don't want the disk round-trip (e.g. an
/// in-memory diff body).
///
/// # Safety
/// See module-level docs.
#[no_mangle]
pub unsafe extern "C" fn ctx_scan_text(
    text_ptr: *const u8,
    text_len: usize,
    virtual_path_ptr: *const u8,
    virtual_path_len: usize,
    opts_json_ptr: *const u8,
    opts_json_len: usize,
    out_result_ptr: *mut *mut c_char,
) -> c_int {
    let result = catch_unwind(AssertUnwindSafe(|| {
        if out_result_ptr.is_null() {
            return ERR_NULL_PTR;
        }
        *out_result_ptr = ptr::null_mut();

        let text_bytes = match slice_from_raw(text_ptr, text_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let vpath_bytes = match slice_from_raw(virtual_path_ptr, virtual_path_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let opts_bytes = match slice_from_raw(opts_json_ptr, opts_json_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let text = match decode_utf8(text_bytes) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let vpath = match decode_utf8(vpath_bytes) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let opts = match decode_opts(opts_bytes) {
            Ok(o) => o,
            Err(e) => return e,
        };
        let warnings = scan_text_inner(text, vpath, &opts);
        let json = match serde_json::to_string(&warnings) {
            Ok(s) => s,
            Err(_) => return ERR_SERIALIZE,
        };
        emit_cstring(json, out_result_ptr)
    }));
    result.unwrap_or(ERR_PANIC)
}

/// In-memory scan helper. Mirrors `scan_file_with_options` but skips
/// the open/read and uses `virtual_path` for the emitted Warning.path.
fn scan_text_inner(text: &str, virtual_path: &str, opts: &Options) -> Vec<Warning> {
    use crate::patterns::secret_patterns;

    if !opts.allowlist_files.is_empty()
        && allowlisted_file_local(virtual_path, &opts.allowlist_files)
    {
        return Vec::new();
    }

    let mut warnings: Vec<Warning> = Vec::new();
    let mut line_no: i64 = 0;
    for line in text.lines() {
        line_no += 1;
        for pattern in secret_patterns() {
            if let Some(m) = pattern.re.find(line) {
                let matched = m.as_str();
                if allowlisted_local(matched, line, &opts.allowlist) {
                    continue;
                }
                warnings.push(Warning {
                    path: virtual_path.to_string(),
                    line: line_no,
                    kind: pattern.kind.to_string(),
                    severity: pattern.severity.to_string(),
                    message: "secret-like pattern detected".to_string(),
                    preview: preview_local(matched),
                });
                break;
            }
        }
        if opts.enable_entropy {
            for token in entropy_candidates_local(line) {
                if allowlisted_local(&token, line, &opts.allowlist) {
                    continue;
                }
                if token.chars().count() >= 20 && crate::entropy::shannon_entropy(&token) >= 4.0 {
                    warnings.push(Warning {
                        path: virtual_path.to_string(),
                        line: line_no,
                        kind: "high_entropy".to_string(),
                        severity: "low".to_string(),
                        message: "high-entropy string detected".to_string(),
                        preview: preview_local(&token),
                    });
                    break;
                }
            }
        }
    }
    warnings
}

// The helpers below are byte-identical to the ones in scan.rs; we
// duplicate them rather than re-export the privates so this module
// stays self-contained.

fn allowlisted_local(matched: &str, line: &str, allowlist: &[String]) -> bool {
    for allowed in allowlist {
        if allowed.is_empty() {
            continue;
        }
        if matched.contains(allowed.as_str()) || line.contains(allowed.as_str()) {
            return true;
        }
    }
    false
}

fn allowlisted_file_local(path: &str, patterns: &[String]) -> bool {
    let slash = path.replace('\\', "/");
    for pattern in patterns {
        if pattern.is_empty() {
            continue;
        }
        let pat = pattern.replace('\\', "/");
        if glob_match_local(&pat, &slash) {
            return true;
        }
        if let Some(prefix) = pat.strip_suffix("/**") {
            if slash.starts_with(&format!("{prefix}/")) {
                return true;
            }
            if slash.contains(&format!("/{prefix}/")) {
                return true;
            }
        }
    }
    false
}

fn glob_match_local(pattern: &str, name: &str) -> bool {
    let pattern = pattern.as_bytes();
    let name = name.as_bytes();
    let mut p = 0;
    let mut n = 0;
    let mut star_p: Option<usize> = None;
    let mut star_n: usize = 0;

    while n < name.len() {
        if p < pattern.len() {
            let pc = pattern[p];
            if pc == b'*' {
                star_p = Some(p);
                star_n = n;
                p += 1;
                continue;
            }
            if pc == b'?' && name[n] != b'/' {
                p += 1;
                n += 1;
                continue;
            }
            if pc == name[n] {
                p += 1;
                n += 1;
                continue;
            }
        }
        if let Some(sp) = star_p {
            if name[star_n] == b'/' {
                return false;
            }
            p = sp + 1;
            star_n += 1;
            n = star_n;
            continue;
        }
        return false;
    }
    while p < pattern.len() && pattern[p] == b'*' {
        p += 1;
    }
    p == pattern.len()
}

fn preview_local(s: &str) -> String {
    if s.len() <= 12 {
        return s.to_string();
    }
    let bytes = s.as_bytes();
    let mut end = 4.min(bytes.len());
    while end > 0 && (bytes[end] & 0xC0) == 0x80 {
        end -= 1;
    }
    let head = std::str::from_utf8(&bytes[..end]).unwrap_or("");

    let len = bytes.len();
    let mut start = if len >= 4 { len - 4 } else { 0 };
    while start < len && (bytes[start] & 0xC0) == 0x80 {
        start += 1;
    }
    let tail = std::str::from_utf8(&bytes[start..]).unwrap_or("");
    format!("{head}[...]{tail}")
}

fn entropy_candidates_local(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for r in line.chars() {
        if r.is_alphabetic() || r.is_numeric() || matches!(r, '_' | '-' | '+' | '/' | '=') {
            current.push(r);
        } else if !current.is_empty() {
            out.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

// ---------------------------------------------------------------------
// ctx_scan_files
// ---------------------------------------------------------------------

/// Scan a batch of paths. `paths_json` is a JSON array of UTF-8
/// strings. Errors on individual paths are silently dropped (matching
/// the Go `continue` behaviour in `ScanFilesWithOptions`).
///
/// # Safety
/// See module-level docs.
#[no_mangle]
pub unsafe extern "C" fn ctx_scan_files(
    paths_json_ptr: *const u8,
    paths_json_len: usize,
    opts_json_ptr: *const u8,
    opts_json_len: usize,
    out_result_ptr: *mut *mut c_char,
) -> c_int {
    let result = catch_unwind(AssertUnwindSafe(|| {
        if out_result_ptr.is_null() {
            return ERR_NULL_PTR;
        }
        *out_result_ptr = ptr::null_mut();

        let paths_bytes = match slice_from_raw(paths_json_ptr, paths_json_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let opts_bytes = match slice_from_raw(opts_json_ptr, opts_json_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let paths: Vec<String> = match serde_json::from_slice(paths_bytes) {
            Ok(p) => p,
            Err(_) => return ERR_BAD_JSON,
        };
        let opts = match decode_opts(opts_bytes) {
            Ok(o) => o,
            Err(e) => return e,
        };
        let warnings = scan_files_with_options(&paths, &opts);
        let json = match serde_json::to_string(&warnings) {
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

/// Free a string previously returned from one of the `ctx_scan_*`
/// functions via `out_*_ptr`. Safe to call on a null pointer (no-op).
///
/// # Safety
/// `s` must either be null or a pointer originally returned by this
/// crate's FFI. Calling on any other pointer is undefined behaviour.
#[no_mangle]
pub unsafe extern "C" fn ctx_scan_free_string(s: *mut c_char) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !s.is_null() {
            drop(CString::from_raw(s));
        }
    }));
}

// ---------------------------------------------------------------------
// ctx_scan_version
// ---------------------------------------------------------------------

/// Returns a pointer to a `'static` NUL-terminated C string carrying
/// the crate's version banner. The caller MUST NOT free it.
#[no_mangle]
pub extern "C" fn ctx_scan_version() -> *const c_char {
    VERSION_C.as_ptr()
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;
    use std::io::Write;

    fn cstr_into_string(p: *mut c_char) -> String {
        assert!(!p.is_null(), "expected non-null out-string");
        let s = unsafe { CStr::from_ptr(p) }
            .to_str()
            .expect("utf-8")
            .to_owned();
        unsafe { ctx_scan_free_string(p) };
        s
    }

    fn write_temp(content: &str) -> String {
        let mut dir = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        dir.push(format!("ctx-scan-ffi-test-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("input.txt");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path.to_string_lossy().into_owned()
    }

    fn sample_aws_access_key() -> String {
        ["AKIA", "IOSFODNN7EXAMPLE"].concat()
    }

    fn sample_github_pat() -> String {
        ["ghp_", "123456789012345678901234567890123456"].concat()
    }

    #[test]
    fn version_round_trips() {
        let p = ctx_scan_version();
        assert!(!p.is_null());
        let s = unsafe { CStr::from_ptr(p) }.to_str().unwrap();
        assert_eq!(s, "ctx-scan 0.1.0");
    }

    #[test]
    fn scan_file_happy_path() {
        let path = write_temp(&format!("aws=\"{}\"", sample_aws_access_key()));
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe { ctx_scan_file(path.as_ptr(), path.len(), ptr::null(), 0, &mut out) };
        assert_eq!(rc, ERR_OK);
        let json = cstr_into_string(out);
        assert!(json.contains("\"aws_access_key\""), "json = {json}");
        assert!(json.contains("\"line\":1"), "json = {json}");
    }

    #[test]
    fn scan_text_happy_path() {
        let text = format!("github=\"{}\"", sample_github_pat());
        let vpath = "virtual.go";
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe {
            ctx_scan_text(
                text.as_ptr(),
                text.len(),
                vpath.as_ptr(),
                vpath.len(),
                ptr::null(),
                0,
                &mut out,
            )
        };
        assert_eq!(rc, ERR_OK);
        let json = cstr_into_string(out);
        assert!(json.contains("\"github_pat\""), "json = {json}");
        assert!(json.contains("\"virtual.go\""), "json = {json}");
    }

    #[test]
    fn scan_files_handles_empty_array() {
        let paths = b"[]";
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe { ctx_scan_files(paths.as_ptr(), paths.len(), ptr::null(), 0, &mut out) };
        assert_eq!(rc, ERR_OK);
        let json = cstr_into_string(out);
        // Go serialises an empty `[]model.Warning(nil)` as `null`; we
        // emit `[]`. The dispatcher normalises both sides.
        assert!(json == "[]" || json == "null", "json = {json}");
    }

    #[test]
    fn rejects_null_out_ptr() {
        let rc = unsafe { ctx_scan_file(ptr::null(), 0, ptr::null(), 0, ptr::null_mut()) };
        assert_eq!(rc, ERR_NULL_PTR);
    }

    #[test]
    fn rejects_oversize_input() {
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe {
            ctx_scan_text(
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
        let path = write_temp("nothing");
        let bad = b"{not json";
        let mut out: *mut c_char = ptr::null_mut();
        let rc =
            unsafe { ctx_scan_file(path.as_ptr(), path.len(), bad.as_ptr(), bad.len(), &mut out) };
        assert_eq!(rc, ERR_BAD_JSON);
    }

    #[test]
    fn free_string_on_null_is_safe() {
        unsafe { ctx_scan_free_string(ptr::null_mut()) };
    }
}
