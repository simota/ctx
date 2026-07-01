// crates/ctx-heatmap/src/ffi.rs
//
// Phase 4 Tier 1 #2 stateless FFI surface for ctx-heatmap.
//
// The brief explicitly chooses **Option B (stateless)** for heatmap:
// `ctx map` invokes once per command (aggregate → squarify → render →
// done). The session/sticky-handle complexity does not earn its keep
// for a 1-caller × 1-shot workload.
//
// FUNCTION SURFACE
// ================
//   ctx_heatmap_aggregate(metrics_json, opts_json, out_buckets_json) -> i32
//   ctx_heatmap_squarify(buckets_json, w, h, out_rects_json) -> i32
//   ctx_heatmap_render_ascii(rects_json, opts_json, out_text) -> i32
//   ctx_heatmap_render_json(rects_json, opts_json, out_text) -> i32
//   ctx_heatmap_render_plain(buckets_json, opts_json, out_text) -> i32
//
//   ctx_heatmap_free_string(s)
//   ctx_heatmap_version() -> *const c_char
//
// JSON shapes follow types.rs serde derives. Errors are encoded as
// integer return codes (matching the ctx-focus convention so the Go
// bridge code can re-use the same enum).

use std::ffi::{c_char, c_int, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::slice;

use crate::aggregate::{aggregate, top_n};
use crate::render::ascii::render_ascii;
use crate::render::json::render_json;
use crate::render::plain::render_plain;
use crate::squarify::squarify;
use crate::types::{
    AggregateOptions, AsciiOptions, Bucket, FileMetric, JsonOptions, PlainOptions, Rect,
};

const MAX_INPUT_BYTES: usize = 256 * 1024 * 1024;

const ERR_OK: c_int = 0;
const ERR_NULL_PTR: c_int = -1;
const ERR_TOO_LARGE: c_int = -2;
const ERR_BAD_JSON: c_int = -3;
const ERR_SERIALIZE: c_int = -4;
const ERR_PANIC: c_int = -99;

static VERSION_C: once_cell::sync::Lazy<CString> =
    once_cell::sync::Lazy::new(|| CString::new("ctx-heatmap 0.1.0").expect("version cstr"));

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

fn emit_bytes(value: Vec<u8>, out: *mut *mut c_char) -> c_int {
    // Strip interior NULs defensively — the JSON renderer's pretty
    // formatter never emits them, but FFI contracts require it.
    let c = match CString::new(value) {
        Ok(c) => c,
        Err(_) => return ERR_SERIALIZE,
    };
    unsafe { *out = c.into_raw() };
    ERR_OK
}

/// # Safety
/// `metrics_ptr`/`opts_ptr` must be valid for `metrics_len`/`opts_len`
/// bytes (zero length permitted with NULL pointers).
/// `out_result_ptr` must be a valid writable pointer to a `*mut c_char`.
#[no_mangle]
pub unsafe extern "C" fn ctx_heatmap_aggregate(
    metrics_ptr: *const u8,
    metrics_len: usize,
    opts_ptr: *const u8,
    opts_len: usize,
    out_result_ptr: *mut *mut c_char,
) -> c_int {
    let r = catch_unwind(AssertUnwindSafe(|| {
        if out_result_ptr.is_null() {
            return ERR_NULL_PTR;
        }
        *out_result_ptr = ptr::null_mut();

        let metrics_bytes = match slice_from_raw(metrics_ptr, metrics_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let opts_bytes = match slice_from_raw(opts_ptr, opts_len) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let metrics: Vec<FileMetric> = match serde_json::from_slice(metrics_bytes) {
            Ok(v) => v,
            Err(_) => return ERR_BAD_JSON,
        };
        let opts: AggregateOptions = if opts_bytes.is_empty() {
            AggregateOptions::default()
        } else {
            match serde_json::from_slice(opts_bytes) {
                Ok(v) => v,
                Err(_) => return ERR_BAD_JSON,
            }
        };

        let buckets = aggregate(&metrics, &opts);
        let buckets = top_n(buckets, opts.top);
        let body = match serde_json::to_string(&buckets) {
            Ok(s) => s,
            Err(_) => return ERR_SERIALIZE,
        };
        emit_cstring(body, out_result_ptr)
    }));
    r.unwrap_or(ERR_PANIC)
}

/// # Safety
/// See module-level docs.
#[no_mangle]
pub unsafe extern "C" fn ctx_heatmap_squarify(
    buckets_ptr: *const u8,
    buckets_len: usize,
    w: c_int,
    h: c_int,
    out_result_ptr: *mut *mut c_char,
) -> c_int {
    let r = catch_unwind(AssertUnwindSafe(|| {
        if out_result_ptr.is_null() {
            return ERR_NULL_PTR;
        }
        *out_result_ptr = ptr::null_mut();
        let buckets_bytes = match slice_from_raw(buckets_ptr, buckets_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let buckets: Vec<Bucket> = match serde_json::from_slice(buckets_bytes) {
            Ok(v) => v,
            Err(_) => return ERR_BAD_JSON,
        };
        let rects = squarify(&buckets, w as i64, h as i64);
        let body = match serde_json::to_string(&rects) {
            Ok(s) => s,
            Err(_) => return ERR_SERIALIZE,
        };
        emit_cstring(body, out_result_ptr)
    }));
    r.unwrap_or(ERR_PANIC)
}

/// # Safety
/// See module-level docs.
#[no_mangle]
pub unsafe extern "C" fn ctx_heatmap_render_ascii(
    rects_ptr: *const u8,
    rects_len: usize,
    opts_ptr: *const u8,
    opts_len: usize,
    out_result_ptr: *mut *mut c_char,
) -> c_int {
    let r = catch_unwind(AssertUnwindSafe(|| {
        if out_result_ptr.is_null() {
            return ERR_NULL_PTR;
        }
        *out_result_ptr = ptr::null_mut();
        let rects_bytes = match slice_from_raw(rects_ptr, rects_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let opts_bytes = match slice_from_raw(opts_ptr, opts_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let rects: Vec<Rect> = match serde_json::from_slice(rects_bytes) {
            Ok(v) => v,
            Err(_) => return ERR_BAD_JSON,
        };
        let opts: AsciiOptions = if opts_bytes.is_empty() {
            AsciiOptions::default()
        } else {
            match serde_json::from_slice(opts_bytes) {
                Ok(v) => v,
                Err(_) => return ERR_BAD_JSON,
            }
        };
        let out = render_ascii(&rects, &opts);
        emit_cstring(out, out_result_ptr)
    }));
    r.unwrap_or(ERR_PANIC)
}

/// # Safety
/// See module-level docs.
#[no_mangle]
pub unsafe extern "C" fn ctx_heatmap_render_json(
    rects_ptr: *const u8,
    rects_len: usize,
    opts_ptr: *const u8,
    opts_len: usize,
    out_result_ptr: *mut *mut c_char,
) -> c_int {
    let r = catch_unwind(AssertUnwindSafe(|| {
        if out_result_ptr.is_null() {
            return ERR_NULL_PTR;
        }
        *out_result_ptr = ptr::null_mut();
        let rects_bytes = match slice_from_raw(rects_ptr, rects_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let opts_bytes = match slice_from_raw(opts_ptr, opts_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let rects: Vec<Rect> = match serde_json::from_slice(rects_bytes) {
            Ok(v) => v,
            Err(_) => return ERR_BAD_JSON,
        };
        let opts: JsonOptions = if opts_bytes.is_empty() {
            JsonOptions::default()
        } else {
            match serde_json::from_slice(opts_bytes) {
                Ok(v) => v,
                Err(_) => return ERR_BAD_JSON,
            }
        };
        let bytes = match render_json(&rects, &opts) {
            Ok(b) => b,
            Err(_) => return ERR_SERIALIZE,
        };
        emit_bytes(bytes, out_result_ptr)
    }));
    r.unwrap_or(ERR_PANIC)
}

/// # Safety
/// See module-level docs.
#[no_mangle]
pub unsafe extern "C" fn ctx_heatmap_render_plain(
    buckets_ptr: *const u8,
    buckets_len: usize,
    opts_ptr: *const u8,
    opts_len: usize,
    out_result_ptr: *mut *mut c_char,
) -> c_int {
    let r = catch_unwind(AssertUnwindSafe(|| {
        if out_result_ptr.is_null() {
            return ERR_NULL_PTR;
        }
        *out_result_ptr = ptr::null_mut();
        let buckets_bytes = match slice_from_raw(buckets_ptr, buckets_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let opts_bytes = match slice_from_raw(opts_ptr, opts_len) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let buckets: Vec<Bucket> = match serde_json::from_slice(buckets_bytes) {
            Ok(v) => v,
            Err(_) => return ERR_BAD_JSON,
        };
        let opts: PlainOptions = if opts_bytes.is_empty() {
            PlainOptions::default()
        } else {
            match serde_json::from_slice(opts_bytes) {
                Ok(v) => v,
                Err(_) => return ERR_BAD_JSON,
            }
        };
        let out = render_plain(&buckets, &opts);
        emit_cstring(out, out_result_ptr)
    }));
    r.unwrap_or(ERR_PANIC)
}

/// # Safety
/// `s` must either be null (no-op) or a pointer returned by a prior
/// successful FFI call.
#[no_mangle]
pub unsafe extern "C" fn ctx_heatmap_free_string(s: *mut c_char) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !s.is_null() {
            drop(CString::from_raw(s));
        }
    }));
}

/// Returns a pointer to a `'static` NUL-terminated version banner.
#[no_mangle]
pub extern "C" fn ctx_heatmap_version() -> *const c_char {
    VERSION_C.as_ptr()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;

    #[test]
    fn version_round_trips() {
        let p = ctx_heatmap_version();
        assert!(!p.is_null());
        let s = unsafe { CStr::from_ptr(p) }.to_str().unwrap();
        assert_eq!(s, "ctx-heatmap 0.1.0");
    }

    fn metric_json(path: &str, tokens: i64, symbols: i64) -> String {
        format!(
            r#"{{"path":"{}","is_dir":false,"tokens":{},"symbols":{}}}"#,
            path, tokens, symbols
        )
    }

    #[test]
    fn aggregate_round_trip() {
        let metrics = format!(
            "[{},{},{}]",
            metric_json("internal/cli/a.go", 1000, 10),
            metric_json("internal/cli/b.go", 500, 5),
            metric_json("cmd/ctx/main.go", 100, 1),
        );
        let opts = r#"{"by":"tokens","depth":2,"top":0}"#;
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe {
            ctx_heatmap_aggregate(
                metrics.as_ptr(),
                metrics.len(),
                opts.as_ptr(),
                opts.len(),
                &mut out,
            )
        };
        assert_eq!(rc, ERR_OK);
        let s = unsafe { CStr::from_ptr(out) }.to_str().unwrap().to_owned();
        unsafe { ctx_heatmap_free_string(out) };
        assert!(s.contains("internal/cli"), "{s}");
        assert!(s.contains("cmd/ctx"), "{s}");
    }

    #[test]
    fn squarify_round_trip() {
        let buckets = r#"[
            {"Path":"a","Tokens":0,"Files":0,"Symbols":0,"Weight":50.0},
            {"Path":"b","Tokens":0,"Files":0,"Symbols":0,"Weight":30.0}
        ]"#;
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe { ctx_heatmap_squarify(buckets.as_ptr(), buckets.len(), 80, 20, &mut out) };
        assert_eq!(rc, ERR_OK);
        let s = unsafe { CStr::from_ptr(out) }.to_str().unwrap().to_owned();
        unsafe { ctx_heatmap_free_string(out) };
        assert!(s.contains("\"Path\":\"a\""));
    }

    #[test]
    fn render_ascii_round_trip() {
        let rects = r#"[
            {"Bucket":{"Path":"a","Tokens":100,"Files":1,"Symbols":1,"Weight":100.0},
             "X":0,"Y":0,"W":40,"H":20},
            {"Bucket":{"Path":"b","Tokens":50,"Files":1,"Symbols":1,"Weight":50.0},
             "X":40,"Y":0,"W":40,"H":20}
        ]"#;
        let opts = r#"{"width":80,"height":20,"by":"tokens","root":".","budget":0}"#;
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe {
            ctx_heatmap_render_ascii(
                rects.as_ptr(),
                rects.len(),
                opts.as_ptr(),
                opts.len(),
                &mut out,
            )
        };
        assert_eq!(rc, ERR_OK);
        let s = unsafe { CStr::from_ptr(out) }.to_str().unwrap().to_owned();
        unsafe { ctx_heatmap_free_string(out) };
        assert!(s.contains("Heatmap (by tokens"), "{s}");
        assert!(s.contains("+--"), "{s}");
    }

    #[test]
    fn render_json_round_trip() {
        let rects = r#"[
            {"Bucket":{"Path":"a","Tokens":100,"Files":2,"Symbols":5,"Weight":100.0},
             "X":0,"Y":0,"W":40,"H":20}
        ]"#;
        let opts = r#"{"root":".","by":"tokens","budget":120}"#;
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe {
            ctx_heatmap_render_json(
                rects.as_ptr(),
                rects.len(),
                opts.as_ptr(),
                opts.len(),
                &mut out,
            )
        };
        assert_eq!(rc, ERR_OK);
        let s = unsafe { CStr::from_ptr(out) }.to_str().unwrap().to_owned();
        unsafe { ctx_heatmap_free_string(out) };
        assert!(s.contains("\"in_budget\": true"), "{s}");
        assert!(s.contains("\"budget\": 120"), "{s}");
    }

    #[test]
    fn render_plain_round_trip() {
        let buckets = r#"[
            {"Path":"internal/web","Tokens":8420,"Files":14,"Symbols":87,"Weight":8420.0}
        ]"#;
        let opts = r#"{"root":".","by":"tokens","budget":0}"#;
        let mut out: *mut c_char = ptr::null_mut();
        let rc = unsafe {
            ctx_heatmap_render_plain(
                buckets.as_ptr(),
                buckets.len(),
                opts.as_ptr(),
                opts.len(),
                &mut out,
            )
        };
        assert_eq!(rc, ERR_OK);
        let s = unsafe { CStr::from_ptr(out) }.to_str().unwrap().to_owned();
        unsafe { ctx_heatmap_free_string(out) };
        assert!(s.contains("internal/web"), "{s}");
        assert!(s.contains("8,420 tokens"), "{s}");
    }

    #[test]
    fn bad_json_rejected() {
        let mut out: *mut c_char = ptr::null_mut();
        let rc =
            unsafe { ctx_heatmap_aggregate(b"not-json".as_ptr(), 8, ptr::null(), 0, &mut out) };
        assert_eq!(rc, ERR_BAD_JSON);
        assert!(out.is_null());
    }

    #[test]
    fn null_out_pointer_rejected() {
        let rc = unsafe { ctx_heatmap_aggregate(ptr::null(), 0, ptr::null(), 0, ptr::null_mut()) };
        assert_eq!(rc, ERR_NULL_PTR);
    }
}
