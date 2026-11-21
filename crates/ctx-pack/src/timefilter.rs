//! Time-filter strings shared by `ctx pack --since/--until`, the root tree
//! command and `GET /api/tree?since=&until=`.
//!
//! Accepted formats: relative (`Ns` / `Nm` / `Nh` / `Nd` / `Nw` / `Nmo` /
//! `Ny`) or absolute `YYYY-MM-DD` (UTC midnight). Pure: no I/O, `now` is
//! passed in.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub fn parse_pack_time_filter(input: &str, now: SystemTime) -> Result<SystemTime, String> {
    if input.is_empty() {
        return Err("time filter: empty string".to_string());
    }
    if let Some(t) = parse_yyyy_mm_dd_utc(input) {
        return Ok(t);
    }
    let lower = input.to_ascii_lowercase();
    let calendar_units = [
        ("mo", 30_u64 * 24 * 60 * 60),
        ("w", 7_u64 * 24 * 60 * 60),
        ("d", 24_u64 * 60 * 60),
        ("y", 365_u64 * 24 * 60 * 60),
    ];
    for (suffix, seconds) in calendar_units {
        if lower.ends_with(suffix) {
            let number = &input[..input.len() - suffix.len()];
            let n = parse_positive_u64_filter(number, input)?;
            return subtract_filter_duration(now, n, seconds, input);
        }
    }
    let duration_units = [("h", 60_u64 * 60), ("m", 60_u64), ("s", 1_u64)];
    for (suffix, seconds) in duration_units {
        if lower.ends_with(suffix) {
            let number = &input[..input.len() - suffix.len()];
            let n = parse_positive_u64_filter(number, input)?;
            return subtract_filter_duration(now, n, seconds, input);
        }
    }
    // `m` is minutes; months are `mo` — keep the example list unambiguous.
    Err(format!(
        "time filter {input:?}: unrecognised format (expected YYYY-MM-DD or relative like 7d/2w/1mo/1y)"
    ))
}

fn parse_positive_u64_filter(number: &str, original: &str) -> Result<u64, String> {
    if number.is_empty() {
        return Err(format!("time filter {original:?}: missing numeric part"));
    }
    if !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!(
            "time filter {original:?}: invalid numeric part {number:?}"
        ));
    }
    let value = number
        .parse::<u64>()
        .map_err(|err| format!("time filter {original:?}: {err}"))?;
    if value == 0 {
        return Err(format!(
            "time filter {original:?}: value must be positive, got 0"
        ));
    }
    Ok(value)
}

fn subtract_filter_duration(
    now: SystemTime,
    amount: u64,
    unit_seconds: u64,
    original: &str,
) -> Result<SystemTime, String> {
    let seconds = amount
        .checked_mul(unit_seconds)
        .ok_or_else(|| format!("time filter {original:?}: duration overflow"))?;
    now.checked_sub(Duration::from_secs(seconds))
        .ok_or_else(|| format!("time filter {original:?}: duration is before unix epoch"))
}

fn parse_yyyy_mm_dd_utc(input: &str) -> Option<SystemTime> {
    let bytes = input.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes
            .iter()
            .enumerate()
            .any(|(index, byte)| index != 4 && index != 7 && !byte.is_ascii_digit())
    {
        return None;
    }

    let year = input[0..4].parse::<i64>().ok()?;
    let month = input[5..7].parse::<u32>().ok()?;
    let day = input[8..10].parse::<u32>().ok()?;
    let max_day = days_in_month(year, month)?;
    if day == 0 || day > max_day {
        return None;
    }

    let days = days_from_civil(year, month, day);
    if days < 0 {
        return None;
    }
    let seconds = (days as u64).checked_mul(24 * 60 * 60)?;
    UNIX_EPOCH.checked_add(Duration::from_secs(seconds))
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i64, month: u32) -> Option<u32> {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => Some(31),
        4 | 6 | 9 | 11 => Some(30),
        2 if is_leap_year(year) => Some(29),
        2 => Some(28),
        _ => None,
    }
}

pub fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let month = month as i64;
    let day = day as i64;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_filter_parser_relative_units() {
        let now = SystemTime::now();
        // 7d → 7 × 24 × 3600 seconds before now
        let t = parse_pack_time_filter("7d", now).unwrap();
        let expected = now.checked_sub(Duration::from_secs(7 * 24 * 3600)).unwrap();
        assert_eq!(t, expected);

        // 2w → 2 × 7 × 24 × 3600
        let t2 = parse_pack_time_filter("2w", now).unwrap();
        let expected2 = now
            .checked_sub(Duration::from_secs(2 * 7 * 24 * 3600))
            .unwrap();
        assert_eq!(t2, expected2);

        // 1mo → 30 × 24 × 3600
        let t3 = parse_pack_time_filter("1mo", now).unwrap();
        let expected3 = now
            .checked_sub(Duration::from_secs(30 * 24 * 3600))
            .unwrap();
        assert_eq!(t3, expected3);
    }

    #[test]
    fn time_filter_parser_absolute_date() {
        let now = SystemTime::now();
        let t = parse_pack_time_filter("2024-01-15", now).unwrap();
        // 2024-01-15 UTC midnight
        let days = days_from_civil(2024, 1, 15);
        let expected = UNIX_EPOCH + Duration::from_secs(days as u64 * 24 * 3600);
        assert_eq!(t, expected);
    }

    #[test]
    fn time_filter_parser_invalid_returns_err() {
        let now = SystemTime::now();
        assert!(parse_pack_time_filter("notadate", now).is_err());
        assert!(parse_pack_time_filter("", now).is_err());
        assert!(parse_pack_time_filter("0d", now).is_err());
        assert!(parse_pack_time_filter("-1d", now).is_err());
        assert!(parse_pack_time_filter("2023-02-29", now).is_err());
        assert!(parse_pack_time_filter("2024-02-30", now).is_err());
        assert!(parse_pack_time_filter("2026-04-31", now).is_err());
        assert!(parse_pack_time_filter("2026-1-01", now).is_err());
        assert!(parse_pack_time_filter("9223372036854775807-01-01", now).is_err());
    }

    #[test]
    fn absolute_date_parser_validates_calendar_dates() {
        assert!(parse_yyyy_mm_dd_utc("2024-02-29").is_some());
        assert!(parse_yyyy_mm_dd_utc("2026-01-31").is_some());

        for invalid in [
            "2023-02-29",
            "2024-02-30",
            "2026-04-31",
            "2026-00-10",
            "2026-13-10",
            "2026-01-00",
        ] {
            assert!(
                parse_yyyy_mm_dd_utc(invalid).is_none(),
                "{invalid} must be rejected"
            );
        }
    }

    #[test]
    fn absolute_date_parser_requires_yyyy_mm_dd_shape() {
        for invalid in [
            "2026-1-01",
            "2026-01-1",
            "26-01-01",
            "20260101",
            "2026/01/01",
            "9223372036854775807-01-01",
        ] {
            assert!(
                parse_yyyy_mm_dd_utc(invalid).is_none(),
                "{invalid} must be rejected"
            );
        }
    }
}
