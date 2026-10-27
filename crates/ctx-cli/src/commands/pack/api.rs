use std::path::Path;

use crate::common::SecurityToml;

use super::*;

pub(crate) fn read_native_pack_content(
    rel: &str,
    abs: &str,
    args: &PackArgs,
    cfg: &PackCtxToml,
) -> Result<String, String> {
    let body = std::fs::read_to_string(abs).map_err(|err| format!("pack: read {rel}: {err}"))?;
    let mut body = if args.api_only {
        extract_public_api_light(rel, &body).unwrap_or_default()
    } else {
        body
    };
    // Scan the text that is emitted, not the file on disk: `--api-only`
    // keeps `export const TOKEN = "…"` lines but renumbers them, so file
    // line numbers would not match (and skipping the scan leaked them).
    redact_pack_body(abs, &mut body, &cfg.security);
    Ok(body)
}

/// Apply the `[security]` secret redaction to text about to be packed.
pub(crate) fn redact_pack_body(abs: &str, body: &mut String, security: &SecurityToml) {
    if !(security.secret_scan && security.redact) {
        return;
    }
    let opts = ctx_scan::Options {
        allowlist: security.allowlist.clone(),
        allowlist_files: security.allowlist_files.clone(),
        enable_entropy: security.enable_entropy,
    };
    *body = ctx_pack::redact::redact_secrets(abs, body, &opts);
}

pub(crate) fn extract_public_api_light(path: &str, body: &str) -> Option<String> {
    let ext = Path::new(path)
        .extension()?
        .to_string_lossy()
        .to_lowercase();
    match ext.as_str() {
        "go" => Some(extract_go_api_light(body)),
        "ts" | "tsx" | "js" | "jsx" | "mjs" => Some(extract_jsts_api_light(body)),
        "py" => Some(extract_python_api_light(body)),
        _ => None,
    }
}

pub(crate) fn supports_api_only_light(path: &str) -> bool {
    Path::new(path)
        .extension()
        .map(|ext| {
            matches!(
                ext.to_string_lossy().to_lowercase().as_str(),
                "go" | "ts" | "tsx" | "js" | "jsx" | "mjs" | "py"
            )
        })
        .unwrap_or(false)
}

pub(crate) fn extract_go_api_light(body: &str) -> String {
    let mut out = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("package ") || trimmed.starts_with("import ") {
            out.push(line.trim_end().to_string());
            continue;
        }
        if (trimmed.starts_with("func ") || trimmed.starts_with("type "))
            && line_has_exported_identifier(trimmed)
        {
            out.push(trim_signature_line(line));
        }
    }
    finish_api_lines(out)
}

pub(crate) fn extract_jsts_api_light(body: &str) -> String {
    let mut out = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("import ") || trimmed.starts_with("export ") {
            out.push(trim_signature_line(line));
        }
    }
    finish_api_lines(out)
}

pub(crate) fn extract_python_api_light(body: &str) -> String {
    let mut out = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("import ") || trimmed.starts_with("from ") {
            out.push(line.trim_end().to_string());
        } else if (trimmed.starts_with("def ") || trimmed.starts_with("class "))
            && !trimmed
                .split_whitespace()
                .nth(1)
                .unwrap_or("")
                .starts_with('_')
        {
            out.push(line.trim_end().to_string());
        }
    }
    finish_api_lines(out)
}

pub(crate) fn line_has_exported_identifier(line: &str) -> bool {
    let name = if let Some(rest) = line.strip_prefix("func (") {
        rest.split(')').nth(1).unwrap_or("").trim_start()
    } else if let Some(rest) = line.strip_prefix("func ") {
        rest
    } else if let Some(rest) = line.strip_prefix("type ") {
        rest
    } else {
        ""
    };
    name.chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase())
}

pub(crate) fn trim_signature_line(line: &str) -> String {
    if let Some((prefix, _)) = line.split_once('{') {
        prefix.trim_end().to_string()
    } else {
        line.trim_end().to_string()
    }
}

pub(crate) fn finish_api_lines(lines: Vec<String>) -> String {
    if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    }
}
