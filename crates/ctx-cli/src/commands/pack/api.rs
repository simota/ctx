use std::path::Path;

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
    if !args.api_only && cfg.security.secret_scan && cfg.security.redact {
        let opts = ctx_scan::Options {
            allowlist: cfg.security.allowlist.clone(),
            allowlist_files: cfg.security.allowlist_files.clone(),
            enable_entropy: cfg.security.enable_entropy,
        };
        if let Ok(warnings) = ctx_scan::scan_file_with_options(abs, &opts) {
            let warning_inputs: Vec<ctx_pack::WarningInput> = warnings
                .iter()
                .map(|warning| ctx_pack::WarningInput {
                    path: rel.to_string(),
                    line: warning.line,
                    kind: warning.kind.clone(),
                })
                .collect();
            let redacted = ctx_pack::redact::redact_lines(body.as_bytes(), &warning_inputs);
            body = String::from_utf8_lossy(&redacted).into_owned();
        }
    }
    Ok(body)
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
