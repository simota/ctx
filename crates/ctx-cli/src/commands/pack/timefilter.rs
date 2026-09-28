use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::*;

#[derive(Debug)]
pub(crate) struct GitTimeIndex {
    pub(crate) commit_times: std::collections::BTreeMap<String, SystemTime>,
    pub(crate) head_paths: std::collections::BTreeSet<String>,
}

pub(crate) fn build_git_commit_time_index(
    root: &Path,
    since: Option<SystemTime>,
) -> Option<GitTimeIndex> {
    let mut args = vec![
        "log".to_string(),
        "--all".to_string(),
        "--name-only".to_string(),
        "-z".to_string(),
        "--format=%x00%ct".to_string(),
        "--diff-filter=ACDMRT".to_string(),
    ];
    if let Some(since) = since.and_then(system_time_unix_seconds) {
        args.push(format!("--since={since}"));
    }
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let log = git_output_bytes_in(root, &arg_refs).ok()?;
    let head = git_output_bytes_in(root, &["ls-tree", "-r", "-z", "--name-only", "HEAD"]).ok()?;

    Some(GitTimeIndex {
        commit_times: parse_git_time_log_z(&log),
        head_paths: parse_git_path_list_z(&head),
    })
}

/// Parse `git log --name-only -z --format=%x00%ct`.
///
/// Git inserts one formatting newline before the first name of each commit;
/// remove exactly that delimiter byte while leaving any newline that is part
/// of the filename intact.
fn parse_git_time_log_z(output: &[u8]) -> std::collections::BTreeMap<String, SystemTime> {
    let mut commit_times = std::collections::BTreeMap::new();
    let mut fields = output.split(|byte| *byte == 0);
    let mut current_time = None;
    let mut first_path = false;

    while let Some(field) = fields.next() {
        if field.is_empty() {
            let Some(raw_ts) = fields.next() else {
                break;
            };
            current_time = std::str::from_utf8(raw_ts)
                .ok()
                .and_then(|raw| raw.parse::<u64>().ok())
                .and_then(|ts| UNIX_EPOCH.checked_add(Duration::from_secs(ts)));
            first_path = true;
            continue;
        }

        let path = if first_path && field.first() == Some(&b'\n') {
            &field[1..]
        } else {
            field
        };
        first_path = false;
        if path.is_empty() {
            continue;
        }
        if let Some(time) = current_time {
            commit_times
                .entry(String::from_utf8_lossy(path).into_owned())
                .or_insert(time);
        }
    }

    commit_times
}

fn parse_git_path_list_z(output: &[u8]) -> std::collections::BTreeSet<String> {
    output
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| String::from_utf8_lossy(path).into_owned())
        .collect()
}

pub(crate) fn system_time_unix_seconds(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
}

pub(crate) fn parse_pack_time_filter(input: &str, now: SystemTime) -> Result<SystemTime, String> {
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

pub(crate) fn parse_positive_u64_filter(number: &str, original: &str) -> Result<u64, String> {
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

pub(crate) fn subtract_filter_duration(
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

pub(crate) fn parse_yyyy_mm_dd_utc(input: &str) -> Option<SystemTime> {
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

pub(crate) fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
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
    fn git_time_log_z_preserves_special_paths() {
        let raw = b"\0" b"200" b"\0\nline\nbreak.rs\0tab\tname.rs\0literal\\name.rs\0"
            b"\0" b"100" b"\0\nolder.rs\0";
        let times = parse_git_time_log_z(raw);

        assert_eq!(
            times.get("line\nbreak.rs").and_then(|time| system_time_unix_seconds(*time)),
            Some(200)
        );
        assert_eq!(
            times.get("tab\tname.rs").and_then(|time| system_time_unix_seconds(*time)),
            Some(200)
        );
        assert_eq!(
            times.get(r"literal\name.rs").and_then(|time| system_time_unix_seconds(*time)),
            Some(200)
        );
        assert_eq!(
            times.get("older.rs").and_then(|time| system_time_unix_seconds(*time)),
            Some(100)
        );
    }

    #[test]
    fn git_path_list_z_preserves_embedded_newlines_tabs_and_backslashes() {
        let paths = parse_git_path_list_z(b"line\nbreak.rs\0tab\tname.rs\0literal\\name.rs\0");
        assert!(paths.contains("line\nbreak.rs"));
        assert!(paths.contains("tab\tname.rs"));
        assert!(paths.contains(r"literal\name.rs"));
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
