//! Option reference shown by `ctx <command> --help`. Subcommands parse their
//! own arguments (clap only forwards them), so clap cannot list their flags;
//! this table supplies a usage line and an options section instead.

/// `(usage, options)` for commands that document their flags.
pub(crate) fn command_help(name: &str) -> Option<(&'static str, &'static str)> {
    Some(match name {
        "pack" => (
            "ctx pack [ROOT] [OPTIONS]",
            "\
Selection:
      --budget <N>            Token budget (default 50000)
      --goal <TEXT>           Rank files by relevance to a goal
      --changed               Only files changed in the work tree (git status)
      --companions[=<N>]      Also pack up to N (default 5) files that usually change
                              together with the selected ones (git co-change); needs
                              --changed, --from-where, --from-stdin or --from-mix
      --since <WHEN>          Only files changed after a date or duration (7d, 2w, 2026-01-01)
      --until <WHEN>          Only files changed before a date or duration
      --use-mtime             Use file mtime instead of git commit time for --since/--until
      --api-only              Keep public API signatures only
      --from-where            Read `ctx where --format json` results from stdin
      --from-stdin            Read paths (one per line) from stdin
      --from-mix <ID>         Use a saved mix recipe
      --diff <BASE..HEAD>     Pack a git diff instead of files
      --layout <LAYOUT>       --diff layout: sequential, side-by-side, unified

Output:
      --format <FORMAT>       markdown (default), xml, json, plain
      --json / --plain        Shorthand for --format json / plain
      --preset <NAME>         blog, review, debug, llm (or [pack] preset in ctx.toml)
      --out <FILE>            Write to FILE instead of stdout
      --explain               Annotate why each file was selected
      --why <PATH>            Explain whether PATH would be packed (repeatable)
      --no-contract           Omit the ctx:contract manifest
      --no-warnings / --no-paths / --no-metadata / --frontmatter <mdx|jekyll>

Snapshots:
      --snapshot <ID>         Save a replay snapshot of this pack
      --since-snapshot <ID>   Only files added or changed since snapshot ID
      --shared                Use the repository-shared snapshot store (.ctx/replay)
      --strict                Compare snapshots by content hash

Examples:
  ctx pack . --goal \"debug session expiry\" --budget 30000 --out context.md
  ctx pack --changed --companions
  ctx where \"session login\" --format json | ctx pack --from-where --companions=3",
        ),
        "where" => (
            "ctx where <QUERY>... [ROOT] [OPTIONS]",
            "\
Command options:
      --limit <N>             Max results (default 10)
      --format <FORMAT>       default, vimgrep, json (--json is shorthand)
      --plain                 Screen-reader friendly output
      --context <N>           Lines of context around each match
      --all                   Require every query term (AND)
      --regex <PATTERN>       Filter matches with a regular expression
      --literal <TEXT>        Case-sensitive exact match filter
      --explain               Show score breakdowns
      --no-suggest            Do not suggest near matches when nothing is found
      --repos-only            List repositories containing matches
      --no-ignore             Ignore .gitignore / .ctxignore (--no-gitignore, --no-ctxignore)
      --no-deps               Skip dependency and build directories (target/, vendor/, ...)

Examples:
  ctx where \"session login\"
  ctx where \"Pack\" --format json --limit 5 | ctx pack --from-where",
        ),
        "cochange" => (
            "ctx cochange [PATH]... [OPTIONS]",
            "\
Lists the files that historically change in the same commits as PATH (a file
or directory, relative to the current directory), with how often and how
reliably they move together. Without PATH, lists the most coupled file pairs.
Paths in the output are relative to the repository top level.

Command options:
      --since <WHEN>          Only scan commits newer than this (90d, 6mo, 1y)
      --commits <N>           Newest commits to scan (default 500)
      --min-weight <N>        Ignore pairs seen together fewer times (default 2)
      --top <N>               Max results (default 20, 0 = all)
      --format <FORMAT>       text (default), plain, json, paths
      --json / --plain        Shorthand for --format json / plain

Examples:
  ctx cochange src/server.rs
  ctx cochange src/auth/ --since 6mo
  ctx cochange src/server.rs --format paths | ctx pack --from-stdin",
        ),
        "digest" => (
            "ctx digest [ROOT] [OPTIONS]",
            "\
Summarises recent activity: commits, authors, file churn and the net token /
symbol delta. ROOT defaults to the repository in the current directory; a
subdirectory scopes the digest to it.

Command options:
      --since <WHEN>          Lookback window (default 7d; e.g. 24h, 2w, 1mo)
      --top <N>               Max hot files (default 10)
      --format <FORMAT>       markdown (default), plain, json (--json is shorthand)
      --out <FILE>            Write to FILE instead of stdout",
        ),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::command_help;

    #[test]
    fn documented_commands_list_their_options() {
        let (usage, options) = command_help("pack").expect("pack help");
        assert!(usage.starts_with("ctx pack"));
        for flag in ["--changed", "--companions", "--from-where", "--budget"] {
            assert!(options.contains(flag), "pack help lacks {flag}");
        }
        assert!(command_help("cochange").unwrap().1.contains("--min-weight"));
        assert!(command_help("digest").unwrap().0.contains("[ROOT]"));
        assert!(command_help("no-such-command").is_none());
    }
}
