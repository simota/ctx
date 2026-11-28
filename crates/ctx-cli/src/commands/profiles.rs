//! `ctx profiles [ROOT]`: the named pack profiles a repository defines in
//! its `ctx.toml` (`[profiles.<name>]`), each with the `ctx pack` options it
//! stands for — so a teammate (or an agent) can discover what
//! `ctx pack --profile <name>` will do before running it.

use std::env;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use std::process::ExitCode;

use serde::Serialize;

#[derive(Debug, Default)]
pub(crate) struct ProfilesArgs {
    root: Option<PathBuf>,
    json: bool,
}

#[derive(Debug, Serialize)]
struct ProfileRow<'a> {
    name: &'a str,
    /// The `ctx pack` options the profile applies.
    command: String,
    #[serde(flatten)]
    profile: &'a ctx_pack::profile::Profile,
}

pub(crate) fn run_profiles_command(args: &[OsString]) -> Option<ExitCode> {
    let parsed = parse_profiles_args(args)?;
    match profiles_command(&parsed) {
        Ok(text) => {
            print!("{text}");
            Some(ExitCode::SUCCESS)
        }
        Err(err) => {
            eprintln!("Error: {err}");
            Some(ExitCode::from(1))
        }
    }
}

pub(crate) fn parse_profiles_args(args: &[OsString]) -> Option<ProfilesArgs> {
    let mut out = ProfilesArgs::default();
    let mut saw = false;
    for arg in args {
        if !saw {
            if arg == OsStr::new("profiles") {
                saw = true;
                continue;
            }
            if arg == OsStr::new("--json") {
                out.json = true;
                continue;
            }
            return None;
        }
        if arg == OsStr::new("--json") {
            out.json = true;
        } else if arg.to_string_lossy().starts_with('-') || out.root.is_some() {
            return None;
        } else {
            out.root = Some(PathBuf::from(arg));
        }
    }
    saw.then_some(out)
}

fn profiles_command(args: &ProfilesArgs) -> Result<String, String> {
    let root = args.root.clone().unwrap_or_else(|| PathBuf::from("."));
    let root = if root.is_absolute() {
        root
    } else {
        env::current_dir().map_err(|e| e.to_string())?.join(root)
    };
    let profiles = ctx_pack::profile::load(&root).map_err(|err| err.to_string())?;
    render(&profiles, args.json)
}

fn render(profiles: &ctx_pack::profile::Profiles, json: bool) -> Result<String, String> {
    let rows: Vec<ProfileRow<'_>> = profiles
        .entries
        .iter()
        .map(|(name, profile)| ProfileRow {
            name,
            command: profile.command_line(),
            profile,
        })
        .collect();
    if json {
        let mut text = serde_json::to_string_pretty(&rows).map_err(|e| e.to_string())?;
        text.push('\n');
        return Ok(text);
    }
    if rows.is_empty() {
        return Ok("No pack profiles. Define one in ctx.toml:\n\n\
             [profiles.review]\n\
             description = \"Branch review with callers and tests\"\n\
             base = \"main\"\n\
             dependents = 5\n\
             tests = 5\n\
             budget = 60000\n\n\
             then run: ctx pack --profile review\n"
            .to_string());
    }
    let mut out = String::new();
    for row in &rows {
        out.push_str(row.name);
        if !row.profile.description.is_empty() {
            out.push_str("  — ");
            out.push_str(&row.profile.description);
        }
        out.push('\n');
        out.push_str("  ctx pack ");
        out.push_str(if row.command.is_empty() {
            "(no options)"
        } else {
            &row.command
        });
        out.push('\n');
    }
    out.push_str(
        "\nUse: ctx pack --profile <name> [options]  (options given on the command line win)\n",
    );
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    #[test]
    fn parses_root_and_json() {
        let a = parse_profiles_args(&os(&["profiles", "repo", "--json"])).unwrap();
        assert_eq!(a.root, Some(PathBuf::from("repo")));
        assert!(a.json);
        assert!(parse_profiles_args(&os(&["profiles", "a", "b"])).is_none());
        assert!(parse_profiles_args(&os(&["profiles", "--bogus"])).is_none());
        assert!(parse_profiles_args(&os(&["pack"])).is_none());
    }

    #[test]
    fn lists_each_profile_with_its_command_line() {
        let profiles = ctx_pack::profile::parse(
            "[profiles.review]\ndescription = \"Branch review\"\nbase = \"main\"\ntests = 5\n",
        )
        .unwrap();
        let text = render(&profiles, false).unwrap();
        assert!(
            text.contains("review  — Branch review\n  ctx pack --base main --tests=5\n"),
            "{text}"
        );
        let json = render(&profiles, true).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value[0]["name"], "review");
        assert_eq!(value[0]["base"], "main");
        assert_eq!(value[0]["description"], "Branch review");
        assert_eq!(value[0]["command"], "--base main --tests=5");
    }

    #[test]
    fn empty_list_shows_how_to_define_one() {
        let text = render(&Default::default(), false).unwrap();
        assert!(text.contains("[profiles.review]"));
        assert_eq!(render(&Default::default(), true).unwrap(), "[]\n");
    }
}
