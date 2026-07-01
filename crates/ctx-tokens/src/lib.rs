// crates/ctx-tokens/src/lib.rs
//
// Token counting with the cl100k_base BPE encoder — mirrors internal/tokens.
//
// The encoder table is loaded once per process via OnceLock (mirrors Go's
// sync.Once in getSharedEncoder). encode_ordinary matches Go's
// enc.Encode(text, nil, nil) semantics: special tokens are treated as
// ordinary text, no allowed-special / disallowed-special overrides.

use std::sync::OnceLock;

use tiktoken_rs::CoreBPE;

static ENCODER: OnceLock<CoreBPE> = OnceLock::new();

fn get_encoder() -> &'static CoreBPE {
    ENCODER.get_or_init(|| {
        tiktoken_rs::cl100k_base().expect("cl100k_base BPE data must be available at compile time")
    })
}

/// Count the number of cl100k_base tokens in `text`.
///
/// Mirrors Go's `counter.CountString(text)` →
/// `len(enc.Encode(text, nil, nil))`.
pub fn count_str(text: &str) -> i64 {
    get_encoder().encode_ordinary(text).len() as i64
}

/// Count cl100k_base tokens for the content of a file.
///
/// Returns `Ok(count)` on success, `Err` if the file cannot be read or is
/// not valid UTF-8.  Mirrors Go's `counter.CountFile(path)`.
pub fn count_file(path: &str) -> Result<i64, String> {
    let bytes = std::fs::read(path).map_err(|err| format!("read {path}: {err}"))?;
    let text = std::str::from_utf8(&bytes).map_err(|err| format!("utf8 {path}: {err}"))?;
    Ok(count_str(text))
}

/// Rough size-based estimate: 4 bytes per token, matching Go's EstimateBySize.
/// Use this only as a fallback when the real encoder is unavailable or the
/// file cannot be decoded as UTF-8.
pub fn estimate_by_size(size: i64) -> i64 {
    (size / 4).max(1)
}
