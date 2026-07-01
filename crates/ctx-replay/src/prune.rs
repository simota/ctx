// crates/ctx-replay/src/prune.rs
//
// Port of internal/replay/prune.go — ParseDuration extension + Prune.

use crate::store::{Store, StoreError};

/// Result type mirroring `replay.PruneResult`.
#[derive(Debug, Clone, Default)]
pub struct PruneResult {
    pub deleted: Vec<String>,
    pub kept: i64,
}

/// Parses Go's `time.Duration` syntax extended with `d` (day) and `w` (week).
/// Returns total seconds (i64) for portability — callers convert as needed.
///
/// Mirrors `replay.ParseDuration`. Returns the total duration in
/// nanoseconds (matching Go's `time.Duration`).
pub fn parse_duration(s: &str) -> Result<i64, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("replay: empty duration".into());
    }
    let (neg, rest) = if let Some(t) = s.strip_prefix('-') {
        (true, t)
    } else if let Some(t) = s.strip_prefix('+') {
        (false, t)
    } else {
        (false, s)
    };
    if rest.is_empty() {
        return Err("replay: empty duration".into());
    }

    let bytes = rest.as_bytes();
    let mut total: i64 = 0;
    let mut num_start: usize = 0;
    let mut has_number = false;
    let mut i: usize = 0;
    while i < bytes.len() {
        let ch = bytes[i];
        if ch.is_ascii_digit() {
            if !has_number {
                num_start = i;
                has_number = true;
            }
            i += 1;
            continue;
        }
        if !has_number {
            return Err(format!("replay: invalid duration {rest:?}"));
        }
        let unit_start = i;
        while i < bytes.len() && !bytes[i].is_ascii_digit() {
            i += 1;
        }
        let unit = &rest[unit_start..i];
        let num_str = &rest[num_start..unit_start];
        has_number = false;

        match unit {
            "d" => {
                let d = atoi(num_str)?;
                total += d * 24 * 3_600 * 1_000_000_000;
            }
            "w" => {
                let d = atoi(num_str)?;
                total += d * 7 * 24 * 3_600 * 1_000_000_000;
            }
            _ => {
                let part = parse_go_duration_atom(num_str, unit)
                    .map_err(|e| format!("replay: invalid duration {rest:?}: {e}"))?;
                total += part;
            }
        }
    }
    if has_number {
        return Err(format!("replay: duration {rest:?} missing unit"));
    }
    if neg {
        total = -total;
    }
    Ok(total)
}

fn atoi(s: &str) -> Result<i64, String> {
    if s.is_empty() {
        return Err("replay: empty number".into());
    }
    let mut n: i64 = 0;
    for c in s.chars() {
        if !c.is_ascii_digit() {
            return Err(format!("replay: invalid number {s:?}"));
        }
        n = n * 10 + (c as i64 - '0' as i64);
    }
    Ok(n)
}

/// Parses a single stdlib-style duration atom like "30m" or "2h" and
/// returns nanoseconds. Mirrors the subset of `time.ParseDuration` Go's
/// replay.ParseDuration delegates to (ns, us, µs, ms, s, m, h).
fn parse_go_duration_atom(num: &str, unit: &str) -> Result<i64, String> {
    let n = atoi(num)?;
    let mult: i64 = match unit {
        "ns" => 1,
        "us" | "µs" => 1_000,
        "ms" => 1_000_000,
        "s" => 1_000_000_000,
        "m" => 60 * 1_000_000_000,
        "h" => 3_600 * 1_000_000_000,
        other => return Err(format!("unknown unit {other:?}")),
    };
    Ok(n * mult)
}

/// Mirrors `replay.Prune`.
///
/// Deletes manifests with `created_at < now - older` (older specified in
/// nanoseconds). `now` is an RFC3339 timestamp string for parity with
/// the Go input.
pub fn prune(store: &Store, now: &str, older_nanos: i64) -> Result<PruneResult, StoreError> {
    let manifests = store.list()?;
    let now_secs = rfc3339_to_unix_nanos(now)
        .ok_or_else(|| StoreError::InvalidId(format!("now is not RFC3339: {now}")))?;
    let cutoff = now_secs - older_nanos;
    let mut result = PruneResult::default();
    for m in manifests {
        let ts = rfc3339_to_unix_nanos(&m.created_at).unwrap_or(i64::MAX);
        if ts < cutoff {
            store.delete(&m.id)?;
            result.deleted.push(m.id);
        } else {
            result.kept += 1;
        }
    }
    Ok(result)
}

/// Minimal RFC3339 → unix-nanos parser. Handles the subset Go emits
/// (`YYYY-MM-DDTHH:MM:SS[.fractional](Z|±HH:MM)`).
fn rfc3339_to_unix_nanos(s: &str) -> Option<i64> {
    let s = s.trim();
    let len = s.len();
    if len < 20 {
        return None;
    }
    let b = s.as_bytes();
    let y = parse_int(&b[0..4])?;
    if b[4] != b'-' {
        return None;
    }
    let mo = parse_int(&b[5..7])?;
    if b[7] != b'-' {
        return None;
    }
    let d = parse_int(&b[8..10])?;
    if b[10] != b'T' && b[10] != b't' && b[10] != b' ' {
        return None;
    }
    let hh = parse_int(&b[11..13])?;
    if b[13] != b':' {
        return None;
    }
    let mm = parse_int(&b[14..16])?;
    if b[16] != b':' {
        return None;
    }
    let ss = parse_int(&b[17..19])?;

    let mut idx = 19usize;
    let mut frac_nanos: i64 = 0;
    if idx < len && b[idx] == b'.' {
        idx += 1;
        let start = idx;
        while idx < len && b[idx].is_ascii_digit() {
            idx += 1;
        }
        let frac = &b[start..idx];
        // pad/truncate to 9 digits for nanoseconds.
        let mut buf = [b'0'; 9];
        for (i, &c) in frac.iter().take(9).enumerate() {
            buf[i] = c;
        }
        frac_nanos = parse_int(&buf)?;
    }
    let offset_secs: i64 = if idx >= len {
        return None;
    } else if b[idx] == b'Z' || b[idx] == b'z' {
        0
    } else if b[idx] == b'+' || b[idx] == b'-' {
        let sign = if b[idx] == b'+' { 1 } else { -1 };
        if idx + 5 >= len {
            return None;
        }
        let oh = parse_int(&b[idx + 1..idx + 3])?;
        // optional colon
        let mm_start = if b[idx + 3] == b':' { idx + 4 } else { idx + 3 };
        let om = parse_int(&b[mm_start..mm_start + 2])?;
        sign * (oh * 3600 + om * 60)
    } else {
        return None;
    };

    let civil = civil_to_unix(y, mo, d, hh, mm, ss)?;
    Some((civil - offset_secs) * 1_000_000_000 + frac_nanos)
}

fn parse_int(bytes: &[u8]) -> Option<i64> {
    let mut n: i64 = 0;
    for &b in bytes {
        if !b.is_ascii_digit() {
            return None;
        }
        n = n * 10 + (b - b'0') as i64;
    }
    Some(n)
}

/// Converts a civil (UTC) date-time to a Unix timestamp in seconds.
///
/// Uses the Hinnant-style days-from-civil algorithm.
fn civil_to_unix(y: i64, m: i64, d: i64, hh: i64, mm: i64, ss: i64) -> Option<i64> {
    if !(1..=12).contains(&m) {
        return None;
    }
    if !(1..=31).contains(&d) {
        return None;
    }
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hh * 3600 + mm * 60 + ss)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_duration_seconds() {
        assert_eq!(parse_duration("30s").unwrap(), 30 * 1_000_000_000);
    }

    #[test]
    fn parse_duration_days_weeks() {
        let week = parse_duration("1w").unwrap();
        let seven_days = parse_duration("7d").unwrap();
        assert_eq!(week, seven_days);
    }

    #[test]
    fn parse_duration_mixed() {
        let v = parse_duration("1w12h").unwrap();
        let expected = 7 * 24 * 3600 * 1_000_000_000_i64 + 12 * 3600 * 1_000_000_000_i64;
        assert_eq!(v, expected);
    }

    #[test]
    fn parse_duration_negative() {
        let v = parse_duration("-1h").unwrap();
        assert_eq!(v, -3600 * 1_000_000_000);
    }

    #[test]
    fn parse_duration_empty_fails() {
        assert!(parse_duration("").is_err());
        assert!(parse_duration("  ").is_err());
        assert!(parse_duration("5").is_err());
    }

    #[test]
    fn rfc3339_round_trip() {
        let ns = rfc3339_to_unix_nanos("2026-05-29T12:00:00Z").unwrap();
        // 2026-05-29 12:00:00 UTC
        // verify ordering only — base Unix timestamps differ by tooling.
        let later = rfc3339_to_unix_nanos("2026-05-29T13:00:00Z").unwrap();
        assert!(later - ns == 3600 * 1_000_000_000);
    }

    #[test]
    fn rfc3339_with_offset() {
        let a = rfc3339_to_unix_nanos("2026-05-29T12:00:00Z").unwrap();
        let b = rfc3339_to_unix_nanos("2026-05-29T13:00:00+01:00").unwrap();
        assert_eq!(a, b);
    }
}
