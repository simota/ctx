use std::collections::BTreeMap;

use crate::protocol::*;
use crate::types::*;
use crate::*;
use serde::Serialize;
use serde_json::{json, value::RawValue, Map, Value};

pub(crate) fn get_prompt(raw: Option<&RawValue>) -> Result<Value, RpcError> {
    let params: PromptGetParams = parse_json_opt(raw)?;
    if params.name.is_empty() {
        return Err(tool_error("prompts/get: 'name' is required"));
    }
    match params.name.as_str() {
        "onboard-codebase" => {
            let focus = prompt_arg(&params.arguments, "focus_area");
            Ok(prompt_result(
                "Drive a guided onboarding tour of this codebase using ctx tools.",
                render_onboard_codebase(&focus),
            ))
        }
        "summarize-recent-activity" => {
            let since = prompt_arg(&params.arguments, "since");
            let top = prompt_arg(&params.arguments, "top");
            Ok(prompt_result(
                "Summarise recent repository activity using ctx_digest, with reasoning.",
                render_summarize_recent_activity(&since, &top),
            ))
        }
        "find-code-for" => {
            let goal = prompt_arg(&params.arguments, "goal");
            if goal.is_empty() {
                return Err(tool_error(
                    "prompt find-code-for: argument \"goal\" is required",
                ));
            }
            Ok(prompt_result(
                "Locate the code responsible for a goal using ctx_where and ctx_focus.",
                render_find_code_for(&goal),
            ))
        }
        _ => Err(tool_error(&format!("unknown prompt: {}", params.name))),
    }
}

pub(crate) fn prompt_arg(args: &BTreeMap<String, String>, name: &str) -> String {
    sanitize_prompt_arg(args.get(name).map(String::as_str).unwrap_or(""))
}

pub(crate) fn sanitize_prompt_arg(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .filter(|ch| *ch == ' ' || !ch.is_control())
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.len() > MAX_GOAL_LEN {
        trimmed[..MAX_GOAL_LEN].to_string()
    } else {
        trimmed.to_string()
    }
}

pub(crate) fn prompt_result(description: &'static str, text: String) -> Value {
    let mut content = Map::new();
    content.insert("text".to_string(), json!(text));
    content.insert("type".to_string(), json!("text"));

    let mut message = Map::new();
    message.insert("content".to_string(), Value::Object(content));
    message.insert("role".to_string(), json!("user"));

    let mut result = Map::new();
    result.insert("description".to_string(), json!(description));
    result.insert("messages".to_string(), json!([Value::Object(message)]));
    Value::Object(result)
}

pub(crate) fn render_onboard_codebase(focus: &str) -> String {
    let mut out = String::new();
    out.push_str("You are onboarding a new contributor to this repository.\n\n");
    out.push_str("Use the ctx MCP tools to build a mental model and explain it:\n");
    out.push_str("  1. Call ctx_tree {\"path\":\".\",\"depth\":3,\"with_symbols\":false} to see the overall layout.\n");
    out.push_str("  2. Call ctx_digest {\"since\":\"14d\",\"top\":10} to identify hot files.\n");
    if focus.is_empty() {
        out.push_str("  3. Call ctx_skim {\"path\":\"<top-level main.go>\",\"budget\":2000} on each top-level package's main file to understand its public API.\n");
        out.push_str("  4. Call ctx_focus {\"anchor\":\"<most-changed-symbol>\",\"hops\":1} for a deep dive (anchor names come from the digest's Suggested next reads).\n");
    } else {
        out.push_str(&format!(
            "  3. Call ctx_skim {{\"path\":{},\"budget\":2000}} on key files under {} to understand their public API.\n",
            json!(focus),
            json!(focus)
        ));
        out.push_str(&format!(
            "  4. Call ctx_focus {{\"anchor\":{},\"hops\":1}} for a symbol-anchored deep dive.\n",
            json!(focus)
        ));
    }
    out.push_str("\nProduce a concise, sectioned overview: (a) purpose, (b) architecture, ");
    out.push_str("(c) hot areas in recent history, (d) suggested next reads. ");
    out.push_str("Cite specific file paths and symbol names — no vague claims.");
    out
}

pub(crate) fn render_summarize_recent_activity(since: &str, top: &str) -> String {
    let since = if since.is_empty() { "7d" } else { since };
    let top = if top.is_empty() { "10" } else { top };
    let mut out = String::new();
    out.push_str(&format!(
        "Summarise the last {since} of repository activity.\n\n"
    ));
    out.push_str(&format!(
        "Call ctx_digest {{\"since\":{},\"top\":{top},\"format\":\"markdown\"}}. ",
        json!(since)
    ));
    out.push_str("Then explain in 5-8 bullets:\n");
    out.push_str("  - the theme of changes (refactor / feature / bugfix / mixed)\n");
    out.push_str("  - the most affected components and why\n");
    out.push_str("  - risk areas based on net token/symbol delta\n");
    out.push_str("  - any contributors patterns worth noting\n\n");
    out.push_str("If a file in `hot_files` looks important, follow up with ctx_skim {\"path\":\"<that path>\",\"budget\":2000}. ");
    out.push_str("Do not paraphrase numbers — quote them.");
    out
}

pub(crate) fn render_find_code_for(goal: &str) -> String {
    let goal = if goal.is_empty() {
        "the user-specified goal"
    } else {
        goal
    };
    let mut out = String::new();
    out.push_str(&format!(
        "Find the code in this repository that implements: {goal}\n\n"
    ));
    out.push_str("Strategy:\n");
    out.push_str(&format!(
        "  1. Call ctx_where {{\"query\":{},\"limit\":10}} to get candidate files ranked by relevance.\n",
        json!(goal)
    ));
    out.push_str("  2. For each top candidate, call ctx_focus {\"anchor\":\"<path-or-symbol>\",\"hops\":1} to see the immediate neighbourhood. The anchor line emitted by ctx_where (e.g. `Foo@internal/x.go`) can be copied verbatim into the anchor field.\n");
    out.push_str("  3. If still ambiguous, call ctx_skim {\"path\":\"<file>\",\"budget\":2000} to read its public API.\n\n");
    out.push_str("Report: (a) the entry-point file, (b) the symbol that does the work, ");
    out.push_str("(c) which other files participate, (d) one sentence of caveat. ");
    out.push_str("Always cite `path:line` so the user can jump straight there.");
    out
}

#[derive(Serialize)]
pub(crate) struct PromptListEntry {
    pub(crate) arguments: Vec<PromptArgument>,
    pub(crate) description: &'static str,
    pub(crate) name: &'static str,
}

#[derive(Serialize)]
pub(crate) struct PromptArgument {
    pub(crate) description: &'static str,
    pub(crate) name: &'static str,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) required: bool,
}

pub(crate) fn list_prompts() -> Vec<PromptListEntry> {
    vec![
        PromptListEntry {
            arguments: vec![PromptArgument {
                description:
                    "Optional area to focus on (path or topic). Examples: 'internal/mcp', 'auth', 'cmd/ctx'",
                name: "focus_area",
                required: false,
            }],
            description: "Drive a guided onboarding tour of this codebase using ctx tools.",
            name: "onboard-codebase",
        },
        PromptListEntry {
            arguments: vec![
                PromptArgument {
                    description:
                        "Lookback window. Defaults to '7d'. (formats: 7d, 24h, 2w, 1mo — same as ctx_digest)",
                    name: "since",
                    required: false,
                },
                PromptArgument {
                    description: "Hot file count. Defaults to 10.",
                    name: "top",
                    required: false,
                },
            ],
            description: "Summarise recent repository activity using ctx_digest, with reasoning.",
            name: "summarize-recent-activity",
        },
        PromptListEntry {
            arguments: vec![PromptArgument {
                description:
                    "What the user is trying to find (free-form). Examples: 'audit log writer', 'rate limiter', 'config loader'",
                name: "goal",
                required: true,
            }],
            description: "Locate the code responsible for a goal using ctx_where and ctx_focus.",
            name: "find-code-for",
        },
    ]
}
