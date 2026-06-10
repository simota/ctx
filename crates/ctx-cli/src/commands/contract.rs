use std::env;
use std::ffi::{OsStr, OsString};
use std::io::{self};
use std::process::ExitCode;

use crate::common::*;

#[derive(Debug)]
pub(crate) struct ContractVerifyArgs {
    pack_path: OsString,
    format: String,
    response_path: Option<OsString>,
    strict: bool,
    no_symbols: bool,
    check_worktree: bool,
    root: Option<OsString>,
}

pub(crate) fn run_contract_verify(args: &[OsString]) -> Option<ExitCode> {
    if args
        .iter()
        .any(|arg| arg == OsStr::new("--help") || arg == OsStr::new("-h"))
    {
        return Some(render_subcommand_help(
            "ctx contract verify",
            "Verify pack-as-contract evidence against a model response",
            "<PACK> [flags]",
        ));
    }
    let parsed = parse_contract_verify_args(args)?;
    match contract_verify(parsed) {
        Ok(code) => Some(code),
        Err(err) => {
            // Cobra (Go) prints the error twice for non-ExitError returns:
            // once as "Error: <msg>" and once as "<msg>" (via main's fmt.Fprintln).
            eprintln!("Error: {err}");
            eprintln!("{err}");
            Some(ExitCode::from(1))
        }
    }
}

pub(crate) fn parse_contract_verify_args(args: &[OsString]) -> Option<ContractVerifyArgs> {
    let mut out = ContractVerifyArgs {
        pack_path: OsString::new(),
        format: "markdown".to_string(),
        response_path: None,
        strict: false,
        no_symbols: false,
        check_worktree: false,
        root: None,
    };
    let mut positionals: Vec<OsString> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == OsStr::new("--strict") {
            out.strict = true;
        } else if arg == OsStr::new("--no-symbols") {
            out.no_symbols = true;
        } else if arg == OsStr::new("--check-worktree") {
            out.check_worktree = true;
        } else if let Some(value) = flag_value(arg, "--format") {
            out.format = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--format") {
            i += 1;
            out.format = args.get(i)?.to_string_lossy().into_owned();
        } else if let Some(value) = flag_value(arg, "--response") {
            out.response_path = Some(value.to_os_string());
        } else if arg == OsStr::new("--response") {
            i += 1;
            out.response_path = Some(args.get(i)?.clone());
        } else if let Some(value) = flag_value(arg, "--root") {
            out.root = Some(value.to_os_string());
        } else if arg == OsStr::new("--root") {
            i += 1;
            out.root = Some(args.get(i)?.clone());
        } else if let Some(engine) = flag_value(arg, "--engine") {
            let engine = engine.to_string_lossy();
            if engine != "go" && engine != "rust" {
                return None;
            }
        } else if arg == OsStr::new("--engine") {
            i += 1;
            let engine = args.get(i)?.to_string_lossy();
            if engine != "go" && engine != "rust" {
                return None;
            }
        } else if is_option(arg) {
            return None;
        } else {
            positionals.push(arg.clone());
        }
        i += 1;
    }
    if positionals.len() != 1 {
        return None;
    }
    out.pack_path = positionals.remove(0);
    Some(out)
}

pub(crate) fn contract_verify(args: ContractVerifyArgs) -> Result<ExitCode, String> {
    let pack_path = args.pack_path.to_string_lossy().into_owned();
    if pack_path == "-"
        && args
            .response_path
            .as_deref()
            .is_none_or(|p| p == OsStr::new("-"))
    {
        return Err(
            "pack stdin requires --response <file>; stdin cannot carry both pack and response"
                .to_string(),
        );
    }

    let pack_body = read_maybe_stdin(&args.pack_path).map_err(|err| format!("read pack: {err}"))?;
    let contract = ctx_contract::embed::parse_from_pack(&pack_body).ok_or_else(|| {
        format!(
            "pack {pack_path} does not contain a ctx:contract block — regenerate with `ctx pack` (contract emission is on by default; do not pass --no-contract)"
        )
    })?;
    let response = read_response(args.response_path.as_deref())
        .map_err(|err| format!("read response: {err}"))?;
    let worktree_root = if args.check_worktree {
        if let Some(root) = args.root {
            root.to_string_lossy().into_owned()
        } else {
            env::current_dir()
                .map_err(|err| format!("resolve worktree root: {err}"))?
                .to_string_lossy()
                .into_owned()
        }
    } else {
        String::new()
    };

    let mut result = ctx_contract::verify::verify(
        &contract,
        &response,
        &ctx_contract::VerifyOptions {
            strict: args.strict,
            no_symbols: args.no_symbols,
            worktree_root,
        },
    );
    result.pack_file = pack_path;
    ctx_contract::format::render(&mut io::stdout(), &result, &args.format)
        .map_err(|err| err.to_string())?;
    if result.exit_code != 0 {
        print_cobra_empty_error();
        return Ok(ExitCode::from(result.exit_code as u8));
    }
    Ok(ExitCode::SUCCESS)
}
