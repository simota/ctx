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
        "--format=%x00%ct".to_string(),
        "--diff-filter=ACDMRT".to_string(),
    ];
    if let Some(since) = since.and_then(system_time_unix_seconds) {
        args.push(format!("--since={since}"));
    }
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let log = git_output_in(root, &arg_refs).ok()?;
    let head = git_output_in(root, &["ls-tree", "-r", "--name-only", "HEAD"]).ok()?;

    let mut commit_times = std::collections::BTreeMap::new();
    let mut current_time = None;
    for line in log.lines() {
        if let Some(raw_ts) = line.strip_prefix('\0') {
            current_time = raw_ts
                .parse::<u64>()
                .ok()
                .map(|ts| UNIX_EPOCH + Duration::from_secs(ts));
            continue;
        }
        if line.is_empty() {
            continue;
        }
        if let Some(time) = current_time {
            commit_times.entry(line.replace('\\', "/")).or_insert(time);
        }
    }
    let head_paths = head.lines().map(|line| line.replace('\\', "/")).collect();
    Some(GitTimeIndex {
        commit_times,
        head_paths,
    })
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
    Err(format!(
        "time filter {input:?}: unrecognised format (expected YYYY-MM-DD or relative like 7d/2w/1m/1y)"
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
    let mut parts = input.split('-');
    let year = parts.next()?.parse::<i64>().ok()?;
    let month = parts.next()?.parse::<u32>().ok()?;
    let day = parts.next()?.parse::<u32>().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let days = days_from_civil(year, month, day);
    if days < 0 {
        return None;
    }
    Some(UNIX_EPOCH + Duration::from_secs(days as u64 * 24 * 60 * 60))
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
