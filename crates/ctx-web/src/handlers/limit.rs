//! Shared `limit=` query-param parsing: parse an integer, falling back to a
//! per-route default on failure, then clamp to a per-route `[min, max]`
//! range. Used by every handler that accepts a `limit` param instead of each
//! re-implementing `parse::<i32>().unwrap_or(default).clamp(min, max)`.

/// Parse `s` as `i32`, falling back to `default` on failure, then clamp to
/// `[min, max]`.
pub fn parse_limit(s: &str, default: i32, min: i32, max: i32) -> i32 {
    s.parse::<i32>().unwrap_or(default).clamp(min, max)
}
