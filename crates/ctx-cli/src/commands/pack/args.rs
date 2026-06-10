use std::env;
use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Path, PathBuf};

use crate::common::*;
use serde::Deserialize;

#[derive(Debug)]
pub(crate) struct PackArgs {
    pub(crate) root: PathBuf,
    pub(crate) from_where: bool,
    pub(crate) from_stdin: bool,
    pub(crate) diff_spec: String,
    pub(crate) since: String,
    pub(crate) until: String,
    pub(crate) use_mtime: bool,
    pub(crate) budget: i64,
    pub(crate) format: String,
    pub(crate) out: String,
    pub(crate) goal: String,
    pub(crate) no_contract: bool,
    pub(crate) no_warnings: bool,
    pub(crate) no_paths: bool,
    pub(crate) no_metadata: bool,
    pub(crate) frontmatter: String,
    pub(crate) plain_file_contents: bool,
    pub(crate) explain: bool,
    pub(crate) preset: String,
    pub(crate) changed: bool,
    pub(crate) api_only: bool,
    pub(crate) layout: String,
    pub(crate) from_mix: String,
    pub(crate) why_paths: Vec<String>,
    pub(crate) snapshot_id: String,
    pub(crate) since_snapshot: String,
    pub(crate) replay_shared: bool,
    pub(crate) replay_strict: bool,
    pub(crate) format_changed: bool,
    pub(crate) goal_changed: bool,
    pub(crate) budget_changed: bool,
    pub(crate) preset_changed: bool,
    pub(crate) no_warnings_changed: bool,
    pub(crate) no_paths_changed: bool,
    pub(crate) no_metadata_changed: bool,
    pub(crate) frontmatter_changed: bool,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct PackConfigToml {
    #[serde(default)]
    pub(crate) preset: String,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct IgnoreConfigToml {
    #[serde(default)]
    pub(crate) patterns: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PackCtxToml {
    #[serde(default)]
    pub(crate) pack: PackConfigToml,
    #[serde(default)]
    pub(crate) ignore: IgnoreConfigToml,
    #[serde(default)]
    pub(crate) security: SecurityToml,
}

impl Default for PackCtxToml {
    fn default() -> Self {
        Self {
            pack: PackConfigToml::default(),
            ignore: IgnoreConfigToml::default(),
            security: SecurityToml::default(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct NativeMixRecipe {
    #[serde(default)]
    pub(crate) goal: String,
    #[serde(default)]
    pub(crate) files: Vec<String>,
    #[serde(default)]
    pub(crate) budget: NativeMixBudget,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct NativeMixBudget {
    #[serde(default)]
    pub(crate) limit: i64,
}

pub(crate) fn parse_pack_args(args: &[OsString]) -> Option<PackArgs> {
    let mut saw_pack = false;
    let mut json = false;
    let mut plain = false;
    let mut from_where = false;
    let mut from_stdin = false;
    let mut diff_spec = String::new();
    let mut since = String::new();
    let mut until = String::new();
    let mut use_mtime = false;
    let mut budget = 50000_i64;
    let mut format = "markdown".to_string();
    let mut out = String::new();
    let mut goal = String::new();
    let mut no_contract = false;
    let mut no_warnings = false;
    let mut no_paths = false;
    let mut no_metadata = false;
    let mut frontmatter = String::new();
    let plain_file_contents = false;
    let mut explain = false;
    let mut preset = String::new();
    let mut changed = false;
    let mut api_only = false;
    let mut layout = "sequential".to_string();
    let mut from_mix = String::new();
    let mut why_paths = Vec::new();
    let mut snapshot_id = String::new();
    let mut since_snapshot = String::new();
    let mut replay_shared = false;
    let mut replay_strict = false;
    let mut format_changed = false;
    let mut goal_changed = false;
    let mut budget_changed = false;
    let mut preset_changed = false;
    let mut no_warnings_changed = false;
    let mut no_paths_changed = false;
    let mut no_metadata_changed = false;
    let mut frontmatter_changed = false;
    let mut positionals = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == OsStr::new("--json") {
            json = true;
            format_changed = true;
        } else if arg == OsStr::new("--plain") {
            plain = true;
            format_changed = true;
        } else if arg == OsStr::new("pack") {
            if saw_pack {
                return None;
            }
            saw_pack = true;
        } else if arg == OsStr::new("--from-where") {
            from_where = true;
        } else if arg == OsStr::new("--from-stdin") {
            from_stdin = true;
        } else if let Some(value) = flag_value(arg, "--diff") {
            diff_spec = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--diff") {
            i += 1;
            diff_spec = args.get(i)?.to_string_lossy().into_owned();
        } else if let Some(value) = flag_value(arg, "--since") {
            since = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--since") {
            i += 1;
            since = args.get(i)?.to_string_lossy().into_owned();
        } else if let Some(value) = flag_value(arg, "--until") {
            until = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--until") {
            i += 1;
            until = args.get(i)?.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--use-mtime") {
            use_mtime = true;
        } else if let Some(value) = flag_value(arg, "--budget") {
            budget = value.to_string_lossy().parse().ok()?;
            budget_changed = true;
        } else if arg == OsStr::new("--budget") {
            i += 1;
            budget = args.get(i)?.to_string_lossy().parse().ok()?;
            budget_changed = true;
        } else if let Some(value) = flag_value(arg, "--format") {
            format = value.to_string_lossy().into_owned();
            format_changed = true;
        } else if arg == OsStr::new("--format") {
            i += 1;
            format = args.get(i)?.to_string_lossy().into_owned();
            format_changed = true;
        } else if let Some(value) = flag_value(arg, "--out") {
            out = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--out") {
            i += 1;
            out = args.get(i)?.to_string_lossy().into_owned();
        } else if let Some(value) = flag_value(arg, "--goal") {
            goal = value.to_string_lossy().into_owned();
            goal_changed = true;
        } else if arg == OsStr::new("--goal") {
            i += 1;
            goal = args.get(i)?.to_string_lossy().into_owned();
            goal_changed = true;
        } else if arg == OsStr::new("--no-contract")
            || arg == OsStr::new("--contract=false")
            || arg == OsStr::new("--contract=0")
        {
            no_contract = true;
        } else if arg == OsStr::new("--contract")
            || arg == OsStr::new("--contract=true")
            || arg == OsStr::new("--contract=1")
        {
            no_contract = false;
        } else if arg == OsStr::new("--no-warnings") {
            no_warnings = true;
            no_warnings_changed = true;
        } else if arg == OsStr::new("--no-paths") {
            no_paths = true;
            no_paths_changed = true;
        } else if arg == OsStr::new("--no-metadata") {
            no_metadata = true;
            no_metadata_changed = true;
        } else if let Some(value) = flag_value(arg, "--frontmatter") {
            frontmatter = value.to_string_lossy().into_owned();
            frontmatter_changed = true;
        } else if arg == OsStr::new("--frontmatter") {
            i += 1;
            frontmatter = args.get(i)?.to_string_lossy().into_owned();
            frontmatter_changed = true;
        } else if arg == OsStr::new("--explain") {
            explain = true;
        } else if let Some(value) = flag_value(arg, "--preset") {
            preset = value.to_string_lossy().into_owned();
            preset_changed = true;
        } else if arg == OsStr::new("--preset") {
            i += 1;
            preset = args.get(i)?.to_string_lossy().into_owned();
            preset_changed = true;
        } else if arg == OsStr::new("--changed") {
            changed = true;
        } else if arg == OsStr::new("--api-only") {
            api_only = true;
        } else if let Some(value) = flag_value(arg, "--layout") {
            layout = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--layout") {
            i += 1;
            layout = args.get(i)?.to_string_lossy().into_owned();
        } else if let Some(value) = flag_value(arg, "--from-mix") {
            from_mix = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--from-mix") {
            i += 1;
            from_mix = args.get(i)?.to_string_lossy().into_owned();
        } else if let Some(value) = flag_value(arg, "--why") {
            why_paths.push(value.to_string_lossy().into_owned());
        } else if arg == OsStr::new("--why") {
            i += 1;
            why_paths.push(args.get(i)?.to_string_lossy().into_owned());
        } else if let Some(value) = flag_value(arg, "--snapshot") {
            snapshot_id = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--snapshot") {
            i += 1;
            snapshot_id = args.get(i)?.to_string_lossy().into_owned();
        } else if let Some(value) = flag_value(arg, "--since-snapshot") {
            since_snapshot = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--since-snapshot") {
            i += 1;
            since_snapshot = args.get(i)?.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--shared") {
            replay_shared = true;
        } else if arg == OsStr::new("--strict") {
            replay_strict = true;
        } else if flag_value(arg, "--pack-engine").is_some()
            || flag_value(arg, "--scan-engine").is_some()
        {
        } else if arg == OsStr::new("--pack-engine") || arg == OsStr::new("--scan-engine") {
            i += 1;
            args.get(i)?;
        } else if is_option(arg) {
            return None;
        } else if saw_pack {
            positionals.push(arg.clone());
        } else {
            return None;
        }
        i += 1;
    }
    if !saw_pack || positionals.len() > 1 {
        return None;
    }
    if json {
        format = "json".to_string();
    } else if plain && format == "markdown" {
        format = "plain".to_string();
    }
    Some(PackArgs {
        root: match positionals.as_slice() {
            [] => PathBuf::from("."),
            [root] => PathBuf::from(root),
            _ => return None,
        },
        from_where,
        from_stdin,
        diff_spec,
        since,
        until,
        use_mtime,
        budget,
        format,
        out,
        goal,
        no_contract,
        no_warnings,
        no_paths,
        no_metadata,
        frontmatter,
        plain_file_contents,
        explain,
        preset,
        changed,
        api_only,
        layout,
        from_mix,
        why_paths,
        snapshot_id,
        since_snapshot,
        replay_shared,
        replay_strict,
        format_changed,
        goal_changed,
        budget_changed,
        preset_changed,
        no_warnings_changed,
        no_paths_changed,
        no_metadata_changed,
        frontmatter_changed,
    })
}

pub(crate) fn load_pack_mix(id: &str) -> Result<NativeMixRecipe, String> {
    validate_mix_id(id)?;
    let dir = resolve_mix_store()?;
    std::fs::create_dir_all(&dir)
        .map_err(|err| format!("pack: --from-mix: cannot open mix store: {err}"))?;
    let path = dir.join(format!("{id}.mix.json"));
    let body = std::fs::read_to_string(&path).map_err(|err| {
        if err.kind() == io::ErrorKind::NotFound {
            format!("pack: --from-mix: mix: recipe not found: {id}")
        } else {
            format!("pack: --from-mix: {err}")
        }
    })?;
    serde_json::from_str(&body).map_err(|err| format!("pack: --from-mix: {err}"))
}

pub(crate) fn resolve_mix_store() -> Result<PathBuf, String> {
    if let Ok(xdg) = env::var("XDG_STATE_HOME") {
        let trimmed = xdg.trim();
        if !trimmed.is_empty() {
            return Ok(Path::new(trimmed).join("ctx").join("mixes"));
        }
    }
    let home = env::var("HOME").map_err(|_| "mix: cannot resolve store directory".to_string())?;
    if home.is_empty() {
        return Err("mix: cannot resolve store directory".to_string());
    }
    let state = Path::new(&home).join(".local").join("state");
    if state.exists() {
        Ok(state.join("ctx").join("mixes"))
    } else {
        Ok(Path::new(&home).join(".ctx").join("mixes"))
    }
}

pub(crate) fn validate_mix_id(id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err("pack: --from-mix: mix: invalid recipe id: empty id".to_string());
    }
    if id == "." || id == ".." || id.starts_with('.') {
        return Err(format!("pack: --from-mix: mix: invalid recipe id: {id:?}"));
    }
    for ch in id.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
            continue;
        }
        return Err(format!(
            "pack: --from-mix: mix: invalid recipe id: {id:?} contains disallowed character {ch:?}"
        ));
    }
    Ok(())
}

pub(crate) fn load_pack_ctx_toml(root: &Path) -> Result<PackCtxToml, String> {
    let path = root.join("ctx.toml");
    if !path.exists() {
        return Ok(PackCtxToml::default());
    }
    let body =
        std::fs::read_to_string(&path).map_err(|err| format!("read {}: {err}", path.display()))?;
    toml::from_str::<PackCtxToml>(&body).map_err(|err| format!("decode {}: {err}", path.display()))
}

pub(crate) fn apply_pack_preset(args: &mut PackArgs) -> Result<(), String> {
    let patch = ctx_pack::preset::apply_preset(&args.preset).map_err(|err| err.to_string())?;
    if let Some(format) = patch.format {
        if !args.format_changed {
            args.format = format;
        }
    }
    if let Some(no_warnings) = patch.no_warnings {
        if !args.no_warnings_changed {
            args.no_warnings = no_warnings;
        }
    }
    if let Some(no_paths) = patch.no_paths {
        if !args.no_paths_changed {
            args.no_paths = no_paths;
        }
    }
    if let Some(no_metadata) = patch.no_metadata {
        if !args.no_metadata_changed {
            args.no_metadata = no_metadata;
        }
    }
    if let Some(frontmatter) = patch.frontmatter {
        if !args.frontmatter_changed {
            args.frontmatter = frontmatter;
        }
    }
    if let Some(plain_file_contents) = patch.plain_file_contents {
        args.plain_file_contents = plain_file_contents;
    }
    if let Some(explain) = patch.explain {
        args.explain = explain;
    }
    Ok(())
}
