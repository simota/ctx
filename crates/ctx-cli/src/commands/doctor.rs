use std::env;
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::commands::audit::verify_audit_chain;
use crate::common::*;
use serde::{Deserialize, Serialize};

pub(crate) fn run_doctor(args: &[OsString]) -> Option<ExitCode> {
    let parsed = parse_doctor_args(args)?;
    match doctor_report(&parsed.root, parsed.strict_offline) {
        Ok(report) => {
            if parsed.json {
                if let Err(err) = serde_json::to_writer_pretty(io::stdout(), &report) {
                    eprintln!("Error: {err}");
                    return Some(ExitCode::from(1));
                }
                println!();
            } else {
                render_doctor(&report);
            }
            Some(ExitCode::SUCCESS)
        }
        Err(err) => {
            eprintln!("Error: {err}");
            Some(ExitCode::from(1))
        }
    }
}

#[derive(Debug)]
pub(crate) struct DoctorArgs {
    root: PathBuf,
    json: bool,
    strict_offline: bool,
}

pub(crate) fn parse_doctor_args(args: &[OsString]) -> Option<DoctorArgs> {
    let mut json = false;
    let mut strict_offline = false;
    let mut saw_doctor = false;
    let mut positionals = Vec::new();

    for arg in args {
        if arg == OsStr::new("doctor") {
            if saw_doctor {
                return None;
            }
            saw_doctor = true;
        } else if arg == OsStr::new("--json") {
            json = true;
        } else if arg == OsStr::new("--strict-offline") {
            strict_offline = true;
        } else if is_option(arg) {
            return None;
        } else if saw_doctor {
            positionals.push(PathBuf::from(arg));
        } else {
            return None;
        }
    }

    if !saw_doctor || positionals.len() > 1 {
        return None;
    }
    Some(DoctorArgs {
        root: positionals.pop().unwrap_or_else(|| PathBuf::from(".")),
        json,
        strict_offline,
    })
}

#[derive(Debug, Serialize)]
pub(crate) struct DoctorReport {
    system: DoctorSystem,
    components: Vec<DoctorComponent>,
    strict_offline: DoctorStrictOffline,
    configuration: DoctorConfiguration,
    browse: DoctorBrowse,
}

#[derive(Debug, Serialize)]
pub(crate) struct DoctorSystem {
    // Honest rename of Go's `go_version`: the native build has no Go runtime,
    // so we report the ctx-rust binary version instead and name the key
    // accordingly. ADR-0005 Wave 4 "native-honest doctor".
    runtime: String,
    platform: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct DoctorComponent {
    name: &'static str,
    detail: &'static str,
    status: &'static str,
    #[serde(skip_serializing_if = "str::is_empty")]
    note: &'static str,
}

#[derive(Debug, Serialize)]
pub(crate) struct DoctorStrictOffline {
    flag_value: bool,
    capability: &'static str,
}

#[derive(Debug, Serialize)]
pub(crate) struct DoctorConfiguration {
    ctx_toml: String,
    audit_log: String,
    chain_integrity: String,
    query_masking: String,
    ctxignore: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct DoctorBrowse {
    loopback: DoctorCheck,
    audit_writable: DoctorCheck,
    embedded_ui: DoctorCheck,
    strict_offline: DoctorCheck,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct DoctorCheck {
    status: &'static str,
    detail: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CtxToml {
    #[serde(default)]
    audit: AuditToml,
    #[serde(default)]
    security: SecurityToml,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AuditToml {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    path: String,
    #[serde(default)]
    query_handling: String,
    #[serde(default)]
    mask_patterns: Vec<String>,
}

impl Default for AuditToml {
    fn default() -> Self {
        Self {
            enabled: false,
            path: "~/.ctx/audit.log".to_string(),
            query_handling: String::new(),
            mask_patterns: Vec::new(),
        }
    }
}

pub(crate) fn doctor_report(root: &Path, strict_flag: bool) -> Result<DoctorReport, String> {
    let (cfg, cfg_path) = load_ctx_toml(root)?;
    let strict = strict_flag
        || env::var_os("CTX_STRICT_OFFLINE").as_deref() == Some(OsStr::new("1"))
        || cfg.security.strict_offline;
    Ok(DoctorReport {
        system: DoctorSystem {
            runtime: format!("ctx-rust {}", env!("CARGO_PKG_VERSION")),
            platform: format!("{}/{}", go_os_name(), go_arch_name()),
        },
        components: vec![
            DoctorComponent {
                name: "Tokenizer",
                detail: "ctx-tokens / cl100k_base (local)",
                status: "ok",
                note: "local",
            },
            DoctorComponent {
                name: "Tree-sitter",
                detail: "vendored C grammars (no CGO)",
                status: "ok",
                note: "local",
            },
            DoctorComponent {
                name: "Git",
                detail: "ctx-git (native Rust object reader)",
                status: "ok",
                note: "",
            },
            DoctorComponent {
                name: "MCP transport",
                detail: "stdio",
                status: "ok",
                note: "no network",
            },
            DoctorComponent {
                name: "AI summary",
                detail: "not implemented",
                status: "na",
                note: "",
            },
        ],
        strict_offline: DoctorStrictOffline {
            flag_value: strict,
            capability: "supported (all features are local)",
        },
        configuration: DoctorConfiguration {
            ctx_toml: cfg_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "(none)".to_string()),
            audit_log: audit_status(&cfg.audit),
            chain_integrity: chain_integrity_status(&cfg.audit),
            query_masking: query_masking_status(&cfg.audit),
            ctxignore: ctxignore_status(root),
        },
        browse: browse_readiness(&cfg, strict),
    })
}

pub(crate) fn load_ctx_toml(root: &Path) -> Result<(CtxToml, Option<PathBuf>), String> {
    let path = root.join("ctx.toml");
    if !path.exists() {
        return Ok((CtxToml::default(), None));
    }
    let body =
        std::fs::read_to_string(&path).map_err(|err| format!("read {}: {err}", path.display()))?;
    let cfg = toml::from_str::<CtxToml>(&body)
        .map_err(|err| format!("decode {}: {err}", path.display()))?;
    Ok((cfg, Some(path)))
}

impl Default for CtxToml {
    fn default() -> Self {
        Self {
            audit: AuditToml::default(),
            security: SecurityToml::default(),
        }
    }
}

pub(crate) fn audit_status(cfg: &AuditToml) -> String {
    let path = audit_path(cfg);
    if let Some(path) = path {
        format!("{} (enabled)", path.display())
    } else if !cfg.path.is_empty() {
        format!(
            "{} (disabled)",
            expand_path(OsString::from(&cfg.path)).display()
        )
    } else {
        "~/.ctx/audit.log (disabled)".to_string()
    }
}

pub(crate) fn audit_path(cfg: &AuditToml) -> Option<PathBuf> {
    if env::var_os("CTX_AUDIT_DISABLE").as_deref() == Some(OsStr::new("1")) {
        return None;
    }
    if let Some(path) = env::var_os("CTX_AUDIT_LOG").filter(|path| !path.is_empty()) {
        return Some(expand_path(path));
    }
    if cfg.enabled {
        let path = if cfg.path.is_empty() {
            "~/.ctx/audit.log"
        } else {
            &cfg.path
        };
        return Some(expand_path(OsString::from(path)));
    }
    None
}

pub(crate) fn chain_integrity_status(cfg: &AuditToml) -> String {
    let Some(path) = audit_path(cfg) else {
        return "\u{2014} (audit disabled)".to_string();
    };
    match verify_audit_chain(&path) {
        Ok(result) if result.ok => {
            let ts = rfc3339_now_utc();
            format!("\u{2713} (last verified {ts})")
        }
        Ok(result) if result.broken_end > result.broken_at => {
            format!(
                "\u{2717} broken range: {}-{}",
                result.broken_at, result.broken_end
            )
        }
        Ok(result) => format!("\u{2717} broken at line {}", result.broken_at),
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            "\u{2014} (no audit log yet)".to_string()
        }
        Err(err) => format!("\u{2717} error: {err}"),
    }
}

pub(crate) fn query_masking_status(cfg: &AuditToml) -> String {
    match cfg.query_handling.as_str() {
        "" => "raw".to_string(),
        "mask" => format!("mask ({} pattern(s))", cfg.mask_patterns.len()),
        other => other.to_string(),
    }
}

pub(crate) fn ctxignore_status(root: &Path) -> String {
    let path = root.join(".ctxignore");
    let Ok(body) = std::fs::read_to_string(&path) else {
        return "absent".to_string();
    };
    let count = body
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with('#')
        })
        .count();
    format!("present ({count} rules)")
}

pub(crate) fn browse_readiness(cfg: &CtxToml, strict: bool) -> DoctorBrowse {
    let loopback = if TcpListener::bind("127.0.0.1:0").is_ok() {
        DoctorCheck {
            status: "ok",
            detail: "127.0.0.1 is bindable; default browse target works".to_string(),
        }
    } else {
        DoctorCheck {
            status: "fail",
            detail: "127.0.0.1 unavailable; ctx browse will fail without --bind override"
                .to_string(),
        }
    };

    let resolved_audit_path = if cfg.audit.path.is_empty() {
        expand_path(OsString::from("~/.ctx/audit.log"))
    } else {
        expand_path(OsString::from(&cfg.audit.path))
    };
    let audit_writable = match can_write_audit_log(&resolved_audit_path) {
        Ok(()) if cfg.audit.enabled => DoctorCheck {
            status: "ok",
            detail: format!("{} is writable", resolved_audit_path.display()),
        },
        Ok(()) => DoctorCheck {
            status: "warn",
            detail: format!(
                "{} writable but audit disabled; enable [audit].enabled or pass --audit",
                resolved_audit_path.display()
            ),
        },
        Err(err) => DoctorCheck {
            status: "fail",
            detail: format!("{} not writable: {err}", resolved_audit_path.display()),
        },
    };

    let embedded_ui = if ctx_web::embed::index_html_embedded() {
        DoctorCheck {
            status: "ok",
            detail: "SPA build (index.html + assets) embedded in binary".to_string(),
        }
    } else {
        DoctorCheck {
            status: "fail",
            detail: "ctx-web dist not embedded: index.html missing".to_string(),
        }
    };

    let strict_offline = if strict {
        DoctorCheck {
            status: "ok",
            detail: "strict offline is on; ctx browse remains fully local".to_string(),
        }
    } else {
        DoctorCheck {
            status: "warn",
            detail:
                "strict offline is off; ctx browse is still local but other ctx features may not be"
                    .to_string(),
        }
    };

    DoctorBrowse {
        loopback,
        audit_writable,
        embedded_ui,
        strict_offline,
        recommendations: browse_recommendations(cfg, strict),
    }
}

pub(crate) fn browse_recommendations(cfg: &CtxToml, strict: bool) -> Vec<String> {
    let mut recs = Vec::new();
    if !cfg.audit.enabled {
        recs.push(
            "Team mode: set [audit].enabled = true so every `ctx browse` request is traceable."
                .to_string(),
        );
    }
    if !strict {
        recs.push(
            "Regulated industries: set [security].strict_offline = true for a defensible offline posture."
                .to_string(),
        );
    }
    if !cfg.security.secret_scan {
        recs.push("Enable [security].secret_scan so served files redact tokens before reaching the browser.".to_string());
    }
    recs
}

pub(crate) fn can_write_audit_log(path: &Path) -> io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir)?;
    File::options()
        .create(true)
        .append(true)
        .open(path)
        .map(|_| ())
}

pub(crate) fn render_doctor(report: &DoctorReport) {
    println!("System");
    println!("  Runtime:           {}", report.system.runtime);
    println!("  Platform:          {}\n", report.system.platform);
    println!("Components");
    for component in &report.components {
        let mark = if component.status == "na" {
            "\u{2014}"
        } else {
            "\u{2713}"
        };
        let note = if component.note.is_empty() {
            String::new()
        } else {
            format!(" ({})", component.note)
        };
        println!(
            "  {:<17} {:<31} {}{}",
            format!("{}:", component.name),
            component.detail,
            mark,
            note
        );
    }
    println!();
    println!("Strict offline");
    println!("  Flag value:        {}", report.strict_offline.flag_value);
    println!(
        "  Capability:        \u{2713} {}\n",
        report.strict_offline.capability
    );
    println!("Configuration");
    println!("  ctx.toml:          {}", report.configuration.ctx_toml);
    println!("  Audit log:         {}", report.configuration.audit_log);
    println!(
        "  Chain integrity:   {}",
        report.configuration.chain_integrity
    );
    println!(
        "  Query masking:     {}",
        report.configuration.query_masking
    );
    println!("  .ctxignore:        {}\n", report.configuration.ctxignore);
    println!("Browse readiness");
    render_browse_check("Loopback bind", &report.browse.loopback);
    render_browse_check("Audit writable", &report.browse.audit_writable);
    render_browse_check("Embedded UI", &report.browse.embedded_ui);
    render_browse_check("Strict offline", &report.browse.strict_offline);
    if !report.browse.recommendations.is_empty() {
        println!("  Recommendations:");
        for rec in &report.browse.recommendations {
            println!("    - {rec}");
        }
    }
}

pub(crate) fn render_browse_check(label: &str, check: &DoctorCheck) {
    let mark = match check.status {
        "ok" => "\u{2713}",
        "warn" => "\u{26a0}",
        "fail" => "\u{2717}",
        _ => "\u{2014}",
    };
    println!("  {:<17} {} {}", format!("{label}:"), mark, check.detail);
}

pub(crate) fn go_os_name() -> &'static str {
    match env::consts::OS {
        "macos" => "darwin",
        other => other,
    }
}

pub(crate) fn go_arch_name() -> &'static str {
    match env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "amd64",
        other => other,
    }
}
