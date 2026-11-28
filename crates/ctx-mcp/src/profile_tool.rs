//! Pack profiles over MCP: `ctx_pack {"profile":"review"}` applies a
//! `[profiles.<name>]` table of the repository's `ctx.toml`, and
//! `ctx_profiles` lists them so an agent can discover the strategies the
//! user's team has named instead of guessing flags.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::value::RawValue;
use serde_json::{json, Value};

use crate::server::Server;
use crate::types::PackArgs;
use crate::*;

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProfilesArgs {
    #[serde(default)]
    pub(crate) path: String,
}

impl Server {
    /// The repository whose ctx.toml defines profiles for a call on `path`:
    /// `path` itself when it has a ctx.toml, otherwise the server root.
    fn profiles_dir(&self, path: &str) -> Result<PathBuf, RpcError> {
        let dir = self.resolve_path(if path.is_empty() { "." } else { path })?;
        let dir = if dir.is_file() {
            dir.parent().map(Path::to_path_buf).unwrap_or(dir)
        } else {
            dir
        };
        if dir.join("ctx.toml").is_file() {
            Ok(dir)
        } else {
            Ok(self.root.clone())
        }
    }

    fn load_profiles(&self, path: &str) -> Result<ctx_pack::profile::Profiles, RpcError> {
        let dir = self.profiles_dir(path)?;
        ctx_pack::profile::load(&dir).map_err(|err| tool_error(format!("profiles: {err}")))
    }

    /// Fill the options the caller left unset from the named profile.
    /// Explicit arguments win; `include` / `exclude` add to the profile's.
    pub(crate) fn apply_pack_profile(&self, args: &mut PackArgs) -> Result<(), RpcError> {
        let name = args.profile.trim().to_string();
        if name.is_empty() {
            return Ok(());
        }
        let profiles = self.load_profiles(&args.path)?;
        let profile = match profiles.get(&name) {
            Ok(profile) => profile.clone(),
            Err(err) => {
                return invalid_params_with_hint(
                    &err.to_string(),
                    "call ctx_profiles to list the profiles this repository defines",
                )
            }
        };
        if args.goal.is_empty() {
            if let Some(goal) = profile.goal {
                args.goal = goal;
            }
        }
        if args.budget == 0 {
            if let Some(budget) = profile.budget {
                args.budget = budget;
                args.profile_budget = true;
            }
        }
        if args.format.is_empty() {
            if let Some(format) = profile.format {
                args.format = format;
            }
        }
        if args.base.trim().is_empty() {
            if let Some(base) = profile.base {
                args.base = base;
            }
        }
        if profile.changed == Some(true) {
            args.changed = true;
        }
        for (slot, value) in [
            (&mut args.companions, profile.companions),
            (&mut args.deps, profile.deps),
            (&mut args.dependents, profile.dependents),
            (&mut args.tests, profile.tests),
        ] {
            if *slot == 0 {
                if let Some(n) = value {
                    *slot = i64::from(n);
                }
            }
        }
        if !profile.include.is_empty() {
            let mut include = profile.include;
            include.append(&mut args.include);
            args.include = include;
        }
        if !profile.exclude.is_empty() {
            let mut exclude = profile.exclude;
            exclude.append(&mut args.exclude);
            args.exclude = exclude;
        }
        if args.overflow.is_empty() {
            if let Some(overflow) = profile.overflow {
                args.overflow = overflow;
            }
        }
        if profile.explain == Some(true) {
            args.explain = true;
        }
        Ok(())
    }

    pub(crate) fn run_profiles(&self, raw: Option<&RawValue>) -> Result<Value, RpcError> {
        let args: ProfilesArgs = parse_json_opt(raw)?;
        validate_len("path", &args.path, MAX_PATH_LEN)?;
        let profiles = self.load_profiles(&args.path)?;
        let text = if profiles.is_empty() {
            "No pack profiles: this repository's ctx.toml defines no [profiles.<name>] tables. \
             Use ctx_pack's own arguments, or ask the user to define one (see `ctx profiles --help`)."
                .to_string()
        } else {
            let mut out = format!("{} pack profile(s):\n", profiles.entries.len());
            for (name, profile) in &profiles.entries {
                out.push_str(&format!("\n- {name}"));
                if !profile.description.is_empty() {
                    out.push_str(&format!(": {}", profile.description));
                }
                let fields = serde_json::to_value(profile).unwrap_or(Value::Null);
                let mut settings = fields
                    .as_object()
                    .map(|map| {
                        map.iter()
                            .filter(|(key, _)| key.as_str() != "description")
                            .map(|(key, value)| format!("{key}={value}"))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                if settings.is_empty() {
                    settings.push("(no options)".to_string());
                }
                out.push_str(&format!("\n  {}", settings.join(", ")));
                out.push_str(&format!("\n  use: ctx_pack {}", json!({"profile": name})));
            }
            out.push_str("\n\nArguments you pass to ctx_pack override a profile's values; include / exclude add to them.");
            out
        };
        Ok(json!({"content": [{"type": "text", "text": text}]}))
    }
}
