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
      --base <REV>            Branch review: files changed since the merge base with REV
                              (committed on the branch, staged, unstaged, untracked);
                              implies --changed. REV: main, origin/main, HEAD~3, ...
      --companions[=<N>]      Also pack up to N (default 5) files that usually change
                              together with the selected ones (git co-change); needs
                              --changed, --from-where, --from-stdin, --from-refs or --from-mix
      --deps[=<N>]            Also pack up to N (default 5) files the selected ones import
                              (the definitions they use); same selections as --companions
      --dependents[=<N>]      Also pack up to N (default 5) files that import the selected
                              ones (the callers a change can break)
      --tests[=<N>]           Also pack up to N (default 5) test files that exercise the
                              selected ones (named after or importing them; `ctx tests`)
      --since <WHEN>          Only files changed after a date or duration (7d, 2w, 2026-01-01)
      --until <WHEN>          Only files changed before a date or duration
      --use-mtime             Use file mtime instead of git commit time for --since/--until
      --include <GLOB>        Only pack matching paths (repeatable or comma-separated;
                              `*.rs`, `src/**/*.ts`, `crates/web`, `*.{rs,toml}`)
      --exclude <GLOB>        Never pack matching paths (same syntax; wins over --include)
      --overflow <MODE>       Relevant file over budget: drop (default) or signatures
                              (pack its declaration lines with line numbers instead;
                              request a body later with `path:START-END`)
      --api-only              Keep public API signatures only
      --from-where            Read `ctx where --format json` results from stdin
      --around <N>            With --from-where: pack N lines either side of each
                              matched line instead of whole files. With --from-refs:
                              lines either side of each referenced line (default 10)
      --from-stdin            Read paths (one per line) from stdin; `path:START-END`,
                              `path:N` or `path#LSTART-LEND` packs only those lines
      --from-refs             Read free text from stdin (a model reply, stack trace,
                              review comment, permalink) and pack the files and lines
                              it mentions: `src/a.rs:40`, `a.ts:12:5`, `#L10-L20`,
                              `File \"x.py\", line 8`, unique bare names (`store.rs`)
      --from-mix <ID|NAME>    Use a saved mix recipe (see `ctx mix`; may hold line ranges)
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
  ctx pack --changed --deps --dependents --goal \"what does this change break?\"
  ctx pack --changed --tests --goal \"make the failing test pass\"
  ctx pack --base main --companions --goal \"review this branch\"
  ctx pack --include 'crates/ctx-web/**' --exclude tests --goal \"routing\" --overflow signatures
  echo 'src/store.rs:120-180' | ctx pack --from-stdin
  ctx where \"session expiry\" --json | ctx pack --from-where --around 15
  pbpaste | ctx pack --from-refs --budget 20000      # files a reply or stack trace names
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
        "deps" => (
            "ctx deps <PATH>... [OPTIONS]",
            "\
Lists the files PATH imports — what you need open to understand it.
PATH is a file or a directory, relative to the current directory; a directory
(or several paths) is treated as one unit and answers for everything in it.
Imports are resolved for Rust, Go, TypeScript / JavaScript (incl. Svelte, Vue),
Python, Java / Kotlin, PHP and Swift.

Command options:
      --depth <N>             Follow imports N hops (default 1 = direct only)
      --format <FORMAT>       text (default), json, paths (one path per line)
      --json                  Shorthand for --format json

Examples:
  ctx deps src/server.rs
  ctx deps src/server.rs --depth 2
  ctx deps src/server.rs --format paths | ctx pack --from-stdin",
        ),
        "impact" => (
            "ctx impact <PATH>... [OPTIONS]",
            "\
Lists the files that import PATH — what may break when it changes.
PATH is a file or a directory, relative to the current directory; a directory
(or several paths) is treated as one unit and answers for everything in it.
Imports are resolved for Rust, Go, TypeScript / JavaScript (incl. Svelte, Vue),
Python, Java / Kotlin, PHP and Swift.

Command options:
      --depth <N>             Follow imports N hops (default 1 = direct only)
      --format <FORMAT>       text (default), json, paths (one path per line)
      --json                  Shorthand for --format json

Examples:
  ctx impact src/store.rs
  ctx impact src/store.rs --depth 3 --json
  ctx impact crates/ctx-pack/ --format paths | ctx pack --from-stdin --goal \"review callers\"",
        ),
        "tests" => (
            "ctx tests [PATH]... [OPTIONS]",
            "\
Lists the test files that exercise PATH — the tests to read with a change and
to run after it. A test is paired with a source file when it is named after it
next to it or in a mirrored test directory (`foo_test.go`, `test_foo.py`,
`foo.test.ts`, `__tests__/foo.ts`, `tests/foo.rs`, `src/foo/tests.rs`,
`src/test/java/.../FooTest.java`, `spec/foo_spec.rb`), or when it imports it.
PATH is a file or directory relative to the current directory. Needs no git
history, so it also works for new files.

Command options:
      --changed               Use the work tree's changed files (git status);
                              changed test files are listed too
      --base <REV>            Use the files the branch changes since the merge base
                              with REV (implies --changed)
      --top <N>               Max results (default: all)
      --format <FORMAT>       text (default), json, paths (one path per line)
      --json                  Shorthand for --format json

Examples:
  ctx tests src/store.rs
  ctx tests --changed --format paths            # the tests to run for this change
  ctx tests --base main --json
  ctx tests src/auth/ --format paths | ctx pack --from-stdin",
        ),
        "replay" => (
            "ctx replay <list|show|prune|diff> [ARGS]... [OPTIONS]",
            "\
Pack snapshots saved with `ctx pack --snapshot <ID>`: what was packed, when,
for which goal — to reproduce a pack, or see how a selection drifted.

Actions:
  list                        List saved snapshots
  show ID                     Print a snapshot's manifest (JSON)
  diff A B                    Files added, removed and changed between two snapshots
  prune --older-than <DUR>    Delete snapshots older than DUR (30d, 2w, 12h)

Command options:
      --shared                Use the repository-shared store (.ctx/replay)
      --json                  JSON output (list, prune)
      --by <KEY>              diff: order by tier (default), tokens or score
      --format <FORMAT>       diff: markdown (default) or json

Examples:
  ctx pack --goal \"auth\" --snapshot auth-v1
  ctx replay list
  ctx replay diff auth-v1 auth-v2
  ctx pack --since-snapshot auth-v1          # only what changed since then",
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
        "mix" => (
            "ctx mix <ACTION> [ARGS]... [OPTIONS]",
            "\
Saved file selections (\"mixes\"): an ordered list of repository files plus the
goal and budget they were chosen for. Pack one later with
`ctx pack --from-mix <ID|NAME>`; `ctx browse` and the MCP ctx_pack tool use the
same store.

Actions:
  save NAME [PATH...]         Create a mix (a directory adds the files a pack of it would walk)
  list                        List saved mixes
  show ID|NAME                Show a mix; marks files missing under the current root
  add ID|NAME PATH...         Append files
  drop ID|NAME PATH...        Remove files (a directory removes everything under it)
  set ID|NAME                 Change --name, --goal and/or --budget
  rm ID|NAME...               Delete mixes

Command options:
      --goal <TEXT>           Goal recorded with the mix (save, set)
      --budget <N>            Token budget recorded with the mix (save, set)
      --name <TEXT>           New display name (set)
      --id <ID>               Explicit id for save (default: derived from NAME)
      --from-where            Also read `ctx where --format json` results from stdin (save, add)
      --from-stdin            Also read paths, one per line, from stdin (save, add)
      --from-refs             Also read free text from stdin and add the files and lines
                              it mentions, as `ctx pack --from-refs` would (save, add)
      --root <DIR>            Repository root paths are stored relative to (default .)
      --json                  JSON output (save, list, show)
      --paths                 Print only the file list (show)

Examples:
  ctx mix save \"auth review\" src/auth/ src/session.rs --goal \"session expiry\" --budget 30000
  ctx where \"session login\" --json | ctx mix save login --from-where
  ctx pack --from-mix auth-review --out context.md",
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
        for flag in [
            "--changed",
            "--companions",
            "--deps",
            "--dependents",
            "--tests",
            "--from-where",
            "--budget",
        ] {
            assert!(options.contains(flag), "pack help lacks {flag}");
        }
        assert!(command_help("cochange").unwrap().1.contains("--min-weight"));
        assert!(command_help("digest").unwrap().0.contains("[ROOT]"));
        assert!(command_help("deps").unwrap().1.contains("--depth"));
        assert!(command_help("impact").unwrap().1.contains("--format"));
        assert!(command_help("tests").unwrap().1.contains("--changed"));
        assert!(command_help("replay").unwrap().1.contains("prune --older-than"));
        assert!(command_help("mix").unwrap().1.contains("--from-where"));
        assert!(command_help("no-such-command").is_none());
    }
}
