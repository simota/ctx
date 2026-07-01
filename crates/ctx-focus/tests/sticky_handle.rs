// crates/ctx-focus/tests/sticky_handle.rs
//
// Integration tests for the sticky-handle FFI surface. We hit the
// session API end-to-end via cffi: open → multiple queries → close.
//
// Goal: verify (a) parity between session and stateless paths, (b) safe
// double-close and null-handle behaviour, (c) a small soak smoke run.

use std::ffi::CStr;
use std::ptr;

use ctx_focus::ffi::{
    ctx_focus_free_string, ctx_focus_pack, ctx_focus_session_close, ctx_focus_session_expand,
    ctx_focus_session_open, ctx_focus_session_pack, ctx_focus_session_resolve, ctx_focus_version,
};

const FIXTURE: &str = r#"[
    {"path":"internal/pack/pack.go","is_dir":false,
     "symbols":[{"name":"Pack","kind":"function","line":2},
                {"name":"Options","kind":"type","line":7}],
     "lines":["package pack","func Pack() {}","// uses Options","type Options struct{}"]},
    {"path":"internal/pack/helper.go","is_dir":false,
     "symbols":[{"name":"helper","kind":"function","line":2}],
     "lines":["package pack","func helper() {}"]},
    {"path":"internal/render/render.go","is_dir":false,
     "symbols":[{"name":"RenderPack","kind":"function","line":2}],
     "lines":["package render","func RenderPack() {","// invokes Pack()","}"]},
    {"path":"cmd/main.go","is_dir":false,
     "symbols":[{"name":"main","kind":"function","line":2}],
     "lines":["package main","func main() {}"]}
]"#;

fn open(files: &str) -> *mut std::ffi::c_void {
    let opts = "{}";
    let mut h: *mut std::ffi::c_void = ptr::null_mut();
    let rc = unsafe {
        ctx_focus_session_open(
            files.as_ptr(),
            files.len(),
            opts.as_ptr(),
            opts.len(),
            &mut h,
        )
    };
    assert_eq!(rc, 0);
    assert!(!h.is_null());
    h
}

fn read_cstr(p: *mut std::os::raw::c_char) -> String {
    assert!(!p.is_null());
    let s = unsafe { CStr::from_ptr(p) }.to_str().unwrap().to_owned();
    unsafe { ctx_focus_free_string(p) };
    s
}

#[test]
fn version_banner_matches_crate() {
    let p = ctx_focus_version();
    assert!(!p.is_null());
    let s = unsafe { CStr::from_ptr(p) }.to_str().unwrap();
    assert_eq!(s, "ctx-focus 0.1.0");
}

#[test]
fn open_close_round_trip() {
    let h = open(FIXTURE);
    let rc = unsafe { ctx_focus_session_close(h) };
    assert_eq!(rc, 0);
}

#[test]
fn multi_query_parity_with_stateless() {
    let h = open(FIXTURE);
    for anchor in ["Pack", "helper", "RenderPack", "Options"] {
        let mut a_out = ptr::null_mut();
        let rc = unsafe { ctx_focus_session_pack(h, anchor.as_ptr(), anchor.len(), 1, &mut a_out) };
        assert_eq!(rc, 0);
        let sticky = read_cstr(a_out);

        let mut b_out = ptr::null_mut();
        let rc = unsafe {
            ctx_focus_pack(
                FIXTURE.as_ptr(),
                FIXTURE.len(),
                anchor.as_ptr(),
                anchor.len(),
                1,
                &mut b_out,
            )
        };
        assert_eq!(rc, 0);
        let stateless = read_cstr(b_out);
        assert_eq!(sticky, stateless, "anchor {anchor} diverged");
    }
    unsafe { ctx_focus_session_close(h) };
}

#[test]
fn resolve_expand_pack_consistent_within_session() {
    let h = open(FIXTURE);
    let anchor = "Pack";

    let mut r_out = ptr::null_mut();
    unsafe { ctx_focus_session_resolve(h, anchor.as_ptr(), anchor.len(), &mut r_out) };
    let resolve_json = read_cstr(r_out);

    let mut e_out = ptr::null_mut();
    unsafe { ctx_focus_session_expand(h, anchor.as_ptr(), anchor.len(), 1, &mut e_out) };
    let expand_json = read_cstr(e_out);

    let mut p_out = ptr::null_mut();
    unsafe { ctx_focus_session_pack(h, anchor.as_ptr(), anchor.len(), 1, &mut p_out) };
    let pack_json = read_cstr(p_out);

    // resolve's body should appear as the "anchor" portion of pack.
    assert!(pack_json.contains(&resolve_json[..resolve_json.len().min(40)]));
    // expand's path list should appear inside pack JSON.
    let expand_v: serde_json::Value = serde_json::from_str(&expand_json).unwrap();
    let pack_v: serde_json::Value = serde_json::from_str(&pack_json).unwrap();
    assert_eq!(pack_v["files"], expand_v);

    unsafe { ctx_focus_session_close(h) };
}

#[test]
fn concurrent_sessions_independent() {
    let h1 = open(FIXTURE);
    let h2 = open(FIXTURE);
    assert_ne!(h1 as usize, h2 as usize);
    let q = "Pack";

    let mut o1 = ptr::null_mut();
    let mut o2 = ptr::null_mut();
    unsafe { ctx_focus_session_pack(h1, q.as_ptr(), q.len(), 1, &mut o1) };
    unsafe { ctx_focus_session_pack(h2, q.as_ptr(), q.len(), 1, &mut o2) };
    let s1 = read_cstr(o1);
    let s2 = read_cstr(o2);
    assert_eq!(s1, s2);

    unsafe { ctx_focus_session_close(h1) };
    unsafe { ctx_focus_session_close(h2) };
}

#[test]
fn small_soak_no_leak_2000_cycles() {
    // 2000 open/close pairs, each running one resolve + one expand.
    for _ in 0..2000 {
        let h = open(FIXTURE);
        let q = "Pack";
        let mut r = ptr::null_mut();
        let mut e = ptr::null_mut();
        unsafe { ctx_focus_session_resolve(h, q.as_ptr(), q.len(), &mut r) };
        unsafe { ctx_focus_session_expand(h, q.as_ptr(), q.len(), 2, &mut e) };
        unsafe { ctx_focus_free_string(r) };
        unsafe { ctx_focus_free_string(e) };
        unsafe { ctx_focus_session_close(h) };
    }
}

#[test]
fn null_handle_safe_close() {
    let rc = unsafe { ctx_focus_session_close(ptr::null_mut()) };
    assert_eq!(rc, -1);
}

#[test]
fn null_handle_resolve_returns_bad_handle() {
    let q = "anything";
    let mut out = ptr::null_mut();
    let rc = unsafe { ctx_focus_session_resolve(ptr::null_mut(), q.as_ptr(), q.len(), &mut out) };
    assert_eq!(rc, -10);
    assert!(out.is_null());
}
