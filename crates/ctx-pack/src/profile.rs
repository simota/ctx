// crates/ctx-pack/src/profile.rs
//
// Named pack profiles: reusable *selection strategies* a repository keeps
// in its `ctx.toml` under `[profiles.<name>]`, so the long flag sets a team
// types every day ("branch review with callers and tests, 60k tokens,
// signatures for overflow") become one word on the CLI (`ctx pack
// --profile review`) and over MCP (`ctx_pack {"profile":"review"}`).
//
// A mix saves *which files*; a profile saves *how to choose and render
// them*. The schema is the set of options both front ends understand, so a
// profile behaves the same wherever it is used. Explicit options always win
// over the profile's values.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::pathfilter::PathFilter;

/// Largest budget a profile may set (same ceiling as MCP `ctx_pack`).
pub(crate) const MAX_PROFILE_BUDGET: i64 = 1_000_000;
/// Largest companions / deps / dependents / tests count a profile may set.
pub(crate) const MAX_PROFILE_COUNT: u32 = 50;
/// Longest profile name.
pub(crate) const MAX_PROFILE_NAME_LEN: usize = 64;

/// One `[profiles.<name>]` table.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    /// One line shown by `ctx profiles` and MCP `ctx_profiles`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<i64>,
    /// markdown | plain | json | xml
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// Pack only files changed in the work tree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changed: Option<bool>,
    /// Branch review against this revision (implies `changed`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub companions: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deps: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dependents: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tests: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub include: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<String>,
    /// drop | signatures
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overflow: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explain: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileError {
    /// `ctx.toml` could not be read or is not valid TOML / schema.
    Config(String),
    /// A profile's values are out of range or unknown.
    Invalid { name: String, reason: String },
    /// No profile by that name; `available` lists the defined ones.
    Unknown {
        name: String,
        available: Vec<String>,
    },
}

impl std::fmt::Display for ProfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProfileError::Config(msg) => write!(f, "{msg}"),
            ProfileError::Invalid { name, reason } => {
                write!(f, "profile {name:?}: {reason}")
            }
            ProfileError::Unknown { name, available } if available.is_empty() => write!(
                f,
                "unknown profile {name:?}: ctx.toml defines no [profiles.<name>] tables"
            ),
            ProfileError::Unknown { name, available } => write!(
                f,
                "unknown profile {name:?} (defined: {})",
                available.join(", ")
            ),
        }
    }
}

impl std::error::Error for ProfileError {}

#[derive(Debug, Default, Deserialize)]
struct ProfilesToml {
    #[serde(default)]
    profiles: BTreeMap<String, Profile>,
}

/// The profiles a repository defines, by name (sorted).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Profiles {
    pub entries: BTreeMap<String, Profile>,
}

impl Profiles {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn names(&self) -> Vec<String> {
        self.entries.keys().cloned().collect()
    }

    /// Look up `name`, or say which names exist.
    pub fn get(&self, name: &str) -> Result<&Profile, ProfileError> {
        self.entries
            .get(name.trim())
            .ok_or_else(|| ProfileError::Unknown {
                name: name.trim().to_string(),
                available: self.names(),
            })
    }
}

/// Parse and validate the `[profiles.*]` tables of a `ctx.toml` body.
/// Other sections are ignored.
pub fn parse(body: &str) -> Result<Profiles, ProfileError> {
    let doc: ProfilesToml =
        toml::from_str(body).map_err(|err| ProfileError::Config(format!("ctx.toml: {err}")))?;
    for (name, profile) in &doc.profiles {
        validate_name(name)?;
        profile.validate().map_err(|reason| ProfileError::Invalid {
            name: name.clone(),
            reason,
        })?;
    }
    Ok(Profiles {
        entries: doc.profiles,
    })
}

/// Load the profiles of the repository at `root` (a directory, or a file
/// whose directory is used). A missing `ctx.toml` means no profiles.
pub fn load(root: &Path) -> Result<Profiles, ProfileError> {
    let dir = if root.is_file() {
        root.parent().unwrap_or(root)
    } else {
        root
    };
    let path = dir.join("ctx.toml");
    if !path.exists() {
        return Ok(Profiles::default());
    }
    let body = std::fs::read_to_string(&path)
        .map_err(|err| ProfileError::Config(format!("read {}: {err}", path.display())))?;
    parse(&body).map_err(|err| match err {
        ProfileError::Config(msg) => {
            ProfileError::Config(msg.replacen("ctx.toml", &path.display().to_string(), 1))
        }
        other => other,
    })
}

fn validate_name(name: &str) -> Result<(), ProfileError> {
    let ok = !name.is_empty()
        && name.len() <= MAX_PROFILE_NAME_LEN
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && !name.starts_with('-');
    if ok {
        Ok(())
    } else {
        Err(ProfileError::Invalid {
            name: name.to_string(),
            reason: format!(
                "names use letters, digits, '-' and '_' (at most {MAX_PROFILE_NAME_LEN}) and do not start with '-'"
            ),
        })
    }
}

impl Profile {
    /// Check every value against what `ctx pack` / `ctx_pack` accept.
    pub fn validate(&self) -> Result<(), String> {
        if let Some(budget) = self.budget {
            if !(1..=MAX_PROFILE_BUDGET).contains(&budget) {
                return Err(format!(
                    "budget must be between 1 and {MAX_PROFILE_BUDGET}, got {budget}"
                ));
            }
        }
        if let Some(format) = &self.format {
            if !matches!(format.as_str(), "markdown" | "plain" | "json" | "xml") {
                return Err(format!(
                    "unknown format {format:?} (use markdown, plain, json or xml)"
                ));
            }
        }
        if let Some(overflow) = &self.overflow {
            if !matches!(overflow.as_str(), "drop" | "signatures") {
                return Err(format!(
                    "unknown overflow {overflow:?} (use drop or signatures)"
                ));
            }
        }
        if let Some(base) = &self.base {
            let base = base.trim();
            if base.is_empty() {
                return Err("base must name a revision (main, origin/main, HEAD~3, ...)".into());
            }
            if base.starts_with('-') {
                return Err("base must be a revision, not an option".into());
            }
        }
        if self.base.is_some() && self.changed == Some(false) {
            return Err("base implies changed; drop changed = false".into());
        }
        for (field, value) in [
            ("companions", self.companions),
            ("deps", self.deps),
            ("dependents", self.dependents),
            ("tests", self.tests),
        ] {
            if let Some(n) = value {
                if n > MAX_PROFILE_COUNT {
                    return Err(format!(
                        "{field} must be between 0 and {MAX_PROFILE_COUNT}, got {n}"
                    ));
                }
                if n > 0 && !self.selects_changed() {
                    return Err(format!(
                        "{field} needs a file selection to start from: set changed = true or base"
                    ));
                }
            }
        }
        PathFilter::new(&self.include, &self.exclude)
            .map_err(|err| format!("include / exclude: {err}"))?;
        Ok(())
    }

    /// Whether the profile selects the changed files (`changed` or `base`).
    pub fn selects_changed(&self) -> bool {
        self.changed == Some(true) || self.base.is_some()
    }

    /// The `ctx pack` options this profile stands for, in a fixed order.
    pub fn to_cli_args(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(goal) = &self.goal {
            out.push("--goal".to_string());
            out.push(goal.clone());
        }
        if let Some(budget) = self.budget {
            out.push(format!("--budget={budget}"));
        }
        if let Some(format) = &self.format {
            out.push(format!("--format={format}"));
        }
        if let Some(base) = &self.base {
            out.push("--base".to_string());
            out.push(base.trim().to_string());
        } else if self.changed == Some(true) {
            out.push("--changed".to_string());
        }
        for (flag, value) in [
            ("--companions", self.companions),
            ("--deps", self.deps),
            ("--dependents", self.dependents),
            ("--tests", self.tests),
        ] {
            if let Some(n) = value.filter(|n| *n > 0) {
                out.push(format!("{flag}={n}"));
            }
        }
        for pattern in &self.include {
            out.push("--include".to_string());
            out.push(pattern.clone());
        }
        for pattern in &self.exclude {
            out.push("--exclude".to_string());
            out.push(pattern.clone());
        }
        if let Some(overflow) = &self.overflow {
            out.push(format!("--overflow={overflow}"));
        }
        if self.explain == Some(true) {
            out.push("--explain".to_string());
        }
        out
    }

    /// The options as one shell-safe line: `--base main --deps=5 ...`.
    pub fn command_line(&self) -> String {
        self.to_cli_args()
            .iter()
            .map(|arg| shell_word(arg))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn shell_word(arg: &str) -> String {
    let plain = !arg.is_empty()
        && arg.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '=' | '/' | '.' | ',' | ':' | '~')
        });
    if plain {
        arg.to_string()
    } else {
        format!("'{}'", arg.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
[pack]
preset = ""

[profiles.review]
description = "Branch review with callers and tests"
base = "main"
deps = 5
dependents = 5
tests = 3
budget = 60000
goal = "review this branch"
exclude = ["docs/**", "*.lock"]
overflow = "signatures"

[profiles.web]
include = ["web/src/**"]
format = "xml"
"#;

    #[test]
    fn parses_profiles_and_ignores_other_sections() {
        let profiles = parse(SAMPLE).unwrap();
        assert_eq!(profiles.names(), vec!["review", "web"]);
        let review = profiles.get("review").unwrap();
        assert_eq!(review.base.as_deref(), Some("main"));
        assert_eq!(review.deps, Some(5));
        assert!(review.selects_changed());
    }

    #[test]
    fn cli_args_follow_a_fixed_order_and_quote_for_display() {
        let profiles = parse(SAMPLE).unwrap();
        let review = profiles.get("review").unwrap();
        assert_eq!(
            review.to_cli_args(),
            vec![
                "--goal",
                "review this branch",
                "--budget=60000",
                "--base",
                "main",
                "--deps=5",
                "--dependents=5",
                "--tests=3",
                "--exclude",
                "docs/**",
                "--exclude",
                "*.lock",
                "--overflow=signatures",
            ]
        );
        assert_eq!(
            review.command_line(),
            "--goal 'review this branch' --budget=60000 --base main --deps=5 --dependents=5 --tests=3 --exclude 'docs/**' --exclude '*.lock' --overflow=signatures"
        );
    }

    #[test]
    fn no_profiles_section_is_empty() {
        assert!(parse("[pack]\npreset = \"llm\"\n").unwrap().is_empty());
    }

    #[test]
    fn unknown_name_lists_the_defined_ones() {
        let err = parse(SAMPLE).unwrap().get("reveiw").unwrap_err();
        assert_eq!(
            err.to_string(),
            "unknown profile \"reveiw\" (defined: review, web)"
        );
        let err = Profiles::default().get("x").unwrap_err();
        assert!(err.to_string().contains("defines no [profiles.<name>]"));
    }

    #[test]
    fn rejects_unknown_keys_and_bad_values() {
        let err = parse("[profiles.a]\nbugdet = 10\n").unwrap_err();
        assert!(err.to_string().contains("bugdet"), "{err}");
        for (body, needle) in [
            ("[profiles.a]\nbudget = 0\n", "budget must be between"),
            ("[profiles.a]\nformat = \"html\"\n", "unknown format"),
            ("[profiles.a]\noverflow = \"keep\"\n", "unknown overflow"),
            ("[profiles.a]\nbase = \"--all\"\n", "not an option"),
            ("[profiles.a]\nbase = \" \"\n", "must name a revision"),
            ("[profiles.a]\ndeps = 5\n", "needs a file selection"),
            (
                "[profiles.a]\nchanged = true\ntests = 99\n",
                "between 0 and 50",
            ),
            (
                "[profiles.a]\ninclude = [\"src/{a\"]\n",
                "include / exclude",
            ),
            (
                "[profiles.a]\nbase = \"main\"\nchanged = false\n",
                "implies changed",
            ),
            ("[profiles.\"bad name\"]\n", "names use letters"),
        ] {
            let err = parse(body).unwrap_err().to_string();
            assert!(err.contains(needle), "{body}: {err}");
        }
    }

    #[test]
    fn changed_without_base_emits_changed_flag() {
        let profiles =
            parse("[profiles.wip]\nchanged = true\ntests = 0\nexplain = true\n").unwrap();
        assert_eq!(
            profiles.get("wip").unwrap().to_cli_args(),
            vec!["--changed", "--explain"]
        );
    }

    #[test]
    fn load_reads_root_ctx_toml() {
        let dir = std::env::temp_dir().join(format!("ctx-profile-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(load(&dir).unwrap().is_empty());
        std::fs::write(dir.join("ctx.toml"), SAMPLE).unwrap();
        assert_eq!(load(&dir).unwrap().names(), vec!["review", "web"]);
        std::fs::write(dir.join("ctx.toml"), "[profiles.a]\nbudget = -1\n").unwrap();
        assert!(load(&dir)
            .unwrap_err()
            .to_string()
            .contains("profile \"a\""));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
