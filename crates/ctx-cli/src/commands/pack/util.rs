// Pack helper shims — the implementations moved to ctx_pack::assemble
// together with the renderer (see render.rs). Kept as pub(crate) fns so
// existing ctx-cli call sites stay unchanged without a duplicate copy.

pub(crate) fn estimate_text_tokens(input: &str) -> i64 {
    ctx_pack::assemble::estimate_text_tokens(input)
}

pub(crate) fn current_rfc3339_utc() -> String {
    ctx_pack::assemble::current_rfc3339_utc()
}

pub(crate) fn lang_for_path(path: &str) -> &'static str {
    ctx_pack::assemble::lang_for_path(path)
}

/// Render a filesystem path with forward slashes on Windows while preserving
/// literal backslashes on Unix. This matches Go's filepath.ToSlash semantics:
/// only the platform path separator is rewritten.
pub(crate) fn path_to_slash_lossy(path: &std::path::Path) -> String {
    let value = path.to_string_lossy();
    #[cfg(windows)]
    {
        value.replace('\\', "/")
    }
    #[cfg(not(windows))]
    {
        value.into_owned()
    }
}
