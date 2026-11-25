//! `ctx_mix`: list, show, save and extend saved file selections ("mixes")
//! — the same store `ctx mix`, `ctx browse`'s Pins board and
//! `ctx_pack {"mix": …}` use. Lets an agent find the selections a user
//! saved, and keep its own working set across sessions. Nothing is deleted
//! or overwritten from here: `save` always creates a new mix and `add` only
//! appends.

use std::path::Path;

use serde_json::{json, value::RawValue, Value};

use crate::protocol::*;
use crate::server::Server;
use crate::types::*;
use crate::*;

/// `entry` (`path` or `path:START-END`) names a regular file inside `root`.
fn check_entry(root: &Path, entry: &str) -> Result<String, String> {
    let entry = ctx_mix::normalize_path(entry).map_err(|e| e.to_string())?;
    let (file, _) = ctx_pack::excerpt::split_spec_in(root, &entry);
    let full = root.join(file);
    let real = std::fs::canonicalize(&full)
        .map_err(|_| format!("{file}: no such file under this root"))?;
    let real_root = std::fs::canonicalize(root).map_err(|e| e.to_string())?;
    if !real.starts_with(&real_root) {
        return Err(format!("{file}: resolves outside this root"));
    }
    if !real.is_file() {
        return Err(format!(
            "{file}: not a regular file (list the files themselves)"
        ));
    }
    Ok(entry)
}

fn checked_files(root: &Path, files: &[String]) -> Result<Vec<String>, RpcError> {
    let mut out = Vec::new();
    let mut errors = Vec::new();
    for file in files {
        match check_entry(root, file) {
            Ok(entry) => out.push(entry),
            Err(err) => errors.push(err),
        }
    }
    if !errors.is_empty() {
        return Err(tool_error(format!(
            "ctx_mix: {} file(s) rejected:\n- {}",
            errors.len(),
            errors.join("\n- ")
        )));
    }
    Ok(out)
}

fn describe_mix(root: &Path, mix: &ctx_mix::Mix) -> String {
    let mut out = format!("# Mix {} ({})\n\n", mix.name, mix.id);
    if !mix.goal.is_empty() {
        out.push_str(&format!("Goal: {}\n", mix.goal));
    }
    if mix.budget.limit > 0 {
        out.push_str(&format!("Budget: {} tokens\n", mix.budget.limit));
    }
    out.push_str(&format!("Created: {}\n", mix.created));
    if !mix.updated.is_empty() {
        out.push_str(&format!("Updated: {}\n", mix.updated));
    }
    out.push_str(&format!("\n## Files ({})\n", mix.files.len()));
    let mut missing = 0;
    for file in &mix.files {
        let (path, _) = ctx_pack::excerpt::split_spec_in(root, file);
        if root.join(path).is_file() {
            out.push_str(&format!("- {file}\n"));
        } else {
            missing += 1;
            out.push_str(&format!("- {file} (missing here)\n"));
        }
    }
    if missing > 0 {
        out.push_str(&format!(
            "\n{missing} file(s) are not under this root; ctx_pack skips them.\n"
        ));
    }
    out.push_str(&format!(
        "\nNext: ctx_pack {{\"mix\":\"{}\"}} packs it in this order.\n",
        mix.id
    ));
    out
}

impl Server {
    pub(crate) fn run_mix(&self, raw: Option<&RawValue>) -> Result<Value, RpcError> {
        let mut args: MixArgs = parse_json_opt(raw)?;
        validate_len("path", &args.path, MAX_PATH_LEN)?;
        validate_len("mix", &args.mix, MAX_MIX_REF_LEN)?;
        validate_len("name", &args.name, MAX_MIX_REF_LEN)?;
        validate_len("goal", &args.goal, MAX_GOAL_LEN)?;
        if args.files.len() > MAX_PACK_FILES {
            return invalid_params_with_hint(
                &format!("at most {MAX_PACK_FILES} files"),
                "save the most relevant files only",
            );
        }
        if !(0..=MAX_BUDGET).contains(&args.budget) {
            return invalid_params_with_hint(
                &format!("budget must be between 0 and {MAX_BUDGET}"),
                "suggested: 30000",
            );
        }
        if args.path.is_empty() {
            args.path = ".".to_string();
        }
        let action = if args.action.is_empty() {
            "list"
        } else {
            args.action.as_str()
        };
        let needs_mix = matches!(action, "show" | "add");
        if needs_mix && args.mix.trim().is_empty() {
            return invalid_params_with_hint(
                &format!("action {action:?} needs 'mix' (an id or name)"),
                "list them with {\"action\":\"list\"}",
            );
        }
        if matches!(action, "save" | "add") && args.files.is_empty() {
            return invalid_params_with_hint(
                &format!("action {action:?} needs 'files'"),
                "e.g. {\"files\":[\"src/a.rs\",\"src/b.rs:120-180\"]}",
            );
        }
        if action == "save" && args.name.trim().is_empty() {
            return invalid_params_with_hint(
                "action \"save\" needs 'name'",
                "e.g. {\"action\":\"save\",\"name\":\"auth review\",\"files\":[...]}",
            );
        }
        let root = self.resolve_path(&args.path)?;
        let store =
            ctx_mix::Store::open_default().map_err(|e| tool_error(format!("ctx_mix: {e}")))?;
        let text = match action {
            "list" => {
                let mixes = store
                    .list()
                    .map_err(|e| tool_error(format!("ctx_mix: {e}")))?;
                if mixes.is_empty() {
                    "No saved mixes. Save one with {\"action\":\"save\",\"name\":…,\"files\":[…]}, `ctx mix save` or the ctx browse Pins board.\n".to_string()
                } else {
                    let mut out = "| Id | Name | Files | Goal |\n|---|---|---|---|\n".to_string();
                    for mix in &mixes {
                        out.push_str(&format!(
                            "| {} | {} | {} | {} |\n",
                            mix.id,
                            mix.name.replace('|', "\\|"),
                            mix.files.len(),
                            mix.goal.replace('|', "\\|")
                        ));
                    }
                    out.push_str("\nNext: {\"action\":\"show\",\"mix\":\"<id>\"} lists its files; ctx_pack {\"mix\":\"<id>\"} packs it.\n");
                    out
                }
            }
            "show" => {
                let mix = store
                    .find(&args.mix)
                    .map_err(|e| tool_error(format!("ctx_mix: {e}")))?;
                describe_mix(&root, &mix)
            }
            "save" => {
                let files = checked_files(&root, &args.files)?;
                let mix = store
                    .create(ctx_mix::Draft {
                        id: String::new(),
                        name: args.name.clone(),
                        goal: args.goal.clone(),
                        files,
                        budget: args.budget,
                    })
                    .map_err(|e| tool_error(format!("ctx_mix: {e}")))?;
                format!("Saved.\n\n{}", describe_mix(&root, &mix))
            }
            "add" => {
                let files = checked_files(&root, &args.files)?;
                let mut mix = store
                    .find(&args.mix)
                    .map_err(|e| tool_error(format!("ctx_mix: {e}")))?;
                let before = mix.files.len();
                for file in files {
                    if !mix.files.contains(&file) {
                        mix.files.push(file);
                    }
                }
                let added = mix.files.len() - before;
                if !args.goal.is_empty() {
                    mix.goal = args.goal.clone();
                }
                if args.budget > 0 {
                    mix.budget.limit = args.budget;
                }
                let mix = store
                    .update(mix)
                    .map_err(|e| tool_error(format!("ctx_mix: {e}")))?;
                format!("Added {added} file(s).\n\n{}", describe_mix(&root, &mix))
            }
            other => {
                return invalid_params_with_hint(
                    &format!("unknown action {other:?}"),
                    "allowed: list, show, save, add",
                )
            }
        };
        Ok(json!({"content": [{"type": "text", "text": text}]}))
    }
}
