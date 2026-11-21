use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::*;

pub(crate) use ctx_pack::timefilter::parse_pack_time_filter;

#[derive(Debug)]
pub(crate) struct GitTimeIndex {
    pub(crate) commit_times: std::collections::BTreeMap<String, SystemTime>,
    pub(crate) head_paths: std::collections::BTreeSet<String>,
}

pub(crate) fn build_git_commit_time_index(
    root: &Path,
    since: Option<SystemTime>,
) -> Option<GitTimeIndex> {
    let prefix = git_root_prefix(root).ok()?;
    // `diff.relative=true` would make `log --name-only` name paths from
    // `-C root` already; keep them work-tree-relative for the prefix strip.
    let mut args = vec![
        "-c".to_string(),
        "diff.relative=false".to_string(),
        "log".to_string(),
        "-z".to_string(),
        "--all".to_string(),
        "--name-only".to_string(),
        "--format=%x00%ct".to_string(),
        "--diff-filter=ACDMRT".to_string(),
    ];
    if let Some(since) = since.and_then(system_time_unix_seconds) {
        args.push(format!("--since={since}"));
    }
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let log = git_output_bytes_in(root, &arg_refs).ok()?;
    // `ls-tree` lists the tree under `-C root` with root-relative names.
    let head = git_output_bytes_in(root, &["ls-tree", "-z", "-r", "--name-only", "HEAD"]).ok()?;

    let head_paths = head
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| String::from_utf8_lossy(path).replace('\\', "/"))
        .collect();
    Some(GitTimeIndex {
        commit_times: parse_git_log_commit_times(&log, &prefix),
        head_paths,
    })
}

/// Newest commit time per root-relative path from `git log -z --name-only
/// --format=%x00%ct`, which prints each commit as `\0<ct>\0\n` followed by
/// its NUL-terminated, work-tree-relative names. `-z` keeps names with
/// non-ASCII bytes, quotes or newlines unquoted, so they match walker paths.
fn parse_git_log_commit_times(
    log: &[u8],
    prefix: &str,
) -> std::collections::BTreeMap<String, SystemTime> {
    let mut commit_times = std::collections::BTreeMap::new();
    let mut current_time = None;
    let mut first_name = false;
    let mut fields = log.split(|byte| *byte == 0);
    while let Some(field) = fields.next() {
        if field.is_empty() {
            current_time = fields
                .next()
                .and_then(|raw_ts| std::str::from_utf8(raw_ts).ok()?.parse::<u64>().ok())
                .map(|ts| UNIX_EPOCH + Duration::from_secs(ts));
            first_name = true;
            continue;
        }
        let name = if first_name {
            first_name = false;
            field.strip_prefix(b"\n").unwrap_or(field)
        } else {
            field
        };
        let Some(time) = current_time else {
            continue;
        };
        // Strip before normalising `\`: the prefix keeps its raw spelling.
        let name = String::from_utf8_lossy(name);
        if let Some(path) = strip_git_root_prefix(&name, prefix) {
            commit_times.entry(path.replace('\\', "/")).or_insert(time);
        }
    }
    commit_times
}

pub(crate) fn system_time_unix_seconds(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_z_parser_keeps_newest_time_and_names_paths_from_the_root() {
        let log =
            b"\x00200\x00\nsub/nl\nname.rs\x00top.rs\x00\x00100\x00\nsub/a.rs\x00sub/nl\nname.rs\x00\x00050\x00\n\n\x00";
        let times = parse_git_log_commit_times(log, "sub/");
        let at = |ts| UNIX_EPOCH + Duration::from_secs(ts);

        assert_eq!(times.get("nl\nname.rs"), Some(&at(200)));
        assert_eq!(times.get("a.rs"), Some(&at(100)));
        assert_eq!(times.len(), 2);
        assert_eq!(
            parse_git_log_commit_times(log, "").get("sub/a.rs"),
            Some(&at(100))
        );
    }

    #[test]
    fn log_z_parser_strips_a_prefix_containing_a_backslash() {
        let log = b"\x00300\x00\na\\b/x.rs\x00";
        let times = parse_git_log_commit_times(log, "a\\b/");

        assert_eq!(
            times.get("x.rs"),
            Some(&(UNIX_EPOCH + Duration::from_secs(300)))
        );
    }
}
