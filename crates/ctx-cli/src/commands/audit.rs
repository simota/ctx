use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::PathBuf;
use std::process::ExitCode;

use crate::common::*;
use serde_json::Value;

pub(crate) fn run_audit_verify(args: &[OsString]) -> Option<ExitCode> {
    if args
        .iter()
        .any(|arg| arg == OsStr::new("--help") || arg == OsStr::new("-h"))
    {
        return Some(render_subcommand_help(
            "ctx audit verify",
            "Verify the integrity of the audit log hash chain",
            "[PATH]",
        ));
    }
    if args.iter().any(|arg| is_option(arg)) || args.len() > 1 {
        return None;
    }

    let path = if let Some(path) = args.first() {
        PathBuf::from(path)
    } else if let Some(path) = default_audit_log_path() {
        path
    } else {
        eprintln!("audit verify: no audit log path configured");
        print_cobra_empty_error();
        return Some(ExitCode::from(2));
    };

    match verify_audit_chain(&path) {
        Ok(result) if result.ok => {
            println!("OK");
            Some(ExitCode::SUCCESS)
        }
        Ok(result) if result.broken_end > result.broken_at => {
            println!("broken range: {}-{}", result.broken_at, result.broken_end);
            print_cobra_empty_error();
            Some(ExitCode::from(1))
        }
        Ok(result) => {
            println!("broken at line: {}", result.broken_at);
            print_cobra_empty_error();
            Some(ExitCode::from(1))
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            eprintln!("audit verify: file not found: {}", path.display());
            print_cobra_empty_error();
            Some(ExitCode::from(2))
        }
        Err(err) => {
            eprintln!("audit verify: {err}");
            Some(ExitCode::from(1))
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct AuditVerifyResult {
    pub(crate) ok: bool,
    pub(crate) total: usize,
    pub(crate) broken_at: usize,
    pub(crate) broken_end: usize,
}

pub(crate) fn verify_audit_chain(path: &PathBuf) -> io::Result<AuditVerifyResult> {
    let file = File::open(path)?;
    let reader = BufReader::with_capacity(512 * 1024, file);

    let mut result = AuditVerifyResult::default();
    let mut prev_raw: Option<String> = None;
    let mut in_broken = false;

    for line in reader.lines() {
        let line = line?;
        if line.is_empty() {
            continue;
        }
        result.total += 1;
        let line_num = result.total;

        let prev_hash = serde_json::from_str::<Value>(&line).ok().and_then(|entry| {
            match entry.get("prev_hash") {
                Some(Value::String(hash)) => Some(Some(hash.clone())),
                Some(Value::Null) | None => Some(None),
                _ => None,
            }
        });

        let broken = match (prev_raw.as_deref(), prev_hash) {
            (None, Some(None)) => false,
            (None, _) => true,
            (Some(prev), Some(Some(hash))) => hash != sha256_hex(prev),
            (Some(_), _) => true,
        };

        if broken {
            if !in_broken {
                result.broken_at = line_num;
                in_broken = true;
            }
            result.broken_end = line_num;
        } else {
            in_broken = false;
        }

        prev_raw = Some(line);
    }

    result.ok = result.broken_at == 0;
    Ok(result)
}
