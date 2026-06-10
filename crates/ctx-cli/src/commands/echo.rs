use std::ffi::{OsStr, OsString};
use std::io::{self, Read};
use std::process::ExitCode;

use crate::common::*;

#[derive(Debug)]
pub(crate) struct EchoArgs {
    pack_file: String,
    goal: String,
    top: i32,
    threshold: f64,
    chunk_by: String,
    chunk_size: i32,
    format: String,
}

pub(crate) fn run_echo_command(args: &[OsString]) -> Option<ExitCode> {
    let parsed = parse_echo_args(args)?;
    match echo_command(parsed) {
        Ok(code) => Some(code),
        Err(err) => {
            eprintln!("{err}");
            Some(ExitCode::from(1))
        }
    }
}

pub(crate) fn parse_echo_args(args: &[OsString]) -> Option<EchoArgs> {
    let mut saw_echo = false;
    let mut json = false;
    let mut goal = String::new();
    let mut top = 10;
    let mut threshold = 0.0;
    let mut chunk_by = "paragraph".to_string();
    let mut chunk_size = 40;
    let mut format = "markdown".to_string();
    let mut positionals = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == OsStr::new("--json") {
            json = true;
        } else if arg == OsStr::new("echo") {
            if saw_echo {
                return None;
            }
            saw_echo = true;
        } else if let Some(value) = flag_value(arg, "--goal") {
            goal = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--goal") {
            i += 1;
            goal = args.get(i)?.to_string_lossy().into_owned();
        } else if let Some(value) = flag_value(arg, "--top") {
            top = value.to_string_lossy().parse().ok()?;
        } else if arg == OsStr::new("--top") {
            i += 1;
            top = args.get(i)?.to_string_lossy().parse().ok()?;
        } else if let Some(value) = flag_value(arg, "--threshold") {
            threshold = value.to_string_lossy().parse().ok()?;
        } else if arg == OsStr::new("--threshold") {
            i += 1;
            threshold = args.get(i)?.to_string_lossy().parse().ok()?;
        } else if let Some(value) = flag_value(arg, "--chunk-by") {
            chunk_by = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--chunk-by") {
            i += 1;
            chunk_by = args.get(i)?.to_string_lossy().into_owned();
        } else if let Some(value) = flag_value(arg, "--chunk-size") {
            chunk_size = value.to_string_lossy().parse().ok()?;
        } else if arg == OsStr::new("--chunk-size") {
            i += 1;
            chunk_size = args.get(i)?.to_string_lossy().parse().ok()?;
        } else if let Some(value) = flag_value(arg, "--format") {
            format = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--format") {
            i += 1;
            format = args.get(i)?.to_string_lossy().into_owned();
        } else if flag_value(arg, "--unit").is_some() || flag_value(arg, "--echo-engine").is_some()
        {
        } else if arg == OsStr::new("--unit") || arg == OsStr::new("--echo-engine") {
            i += 1;
            args.get(i)?;
        } else if is_option(arg) {
            return None;
        } else if saw_echo {
            positionals.push(arg.clone());
        } else {
            return None;
        }
        i += 1;
    }
    if !saw_echo || goal.is_empty() {
        return None;
    }
    if json {
        format = "json".to_string();
    }
    Some(EchoArgs {
        pack_file: match positionals.as_slice() {
            [pack_file] => pack_file.to_string_lossy().into_owned(),
            _ => return None,
        },
        goal,
        top,
        threshold,
        chunk_by,
        chunk_size,
        format,
    })
}

pub(crate) fn echo_command(args: EchoArgs) -> Result<ExitCode, String> {
    let body = if args.pack_file == "-" {
        let mut body = String::new();
        io::stdin()
            .read_to_string(&mut body)
            .map_err(|err| format!("echo: read stdin: {err}"))?;
        body
    } else {
        std::fs::read_to_string(&args.pack_file)
            .map_err(|err| format!("echo: read {}: {err}", args.pack_file))?
    };
    let result = ctx_echo::evaluate(
        &args.pack_file,
        &body,
        &ctx_echo::Options {
            goal: args.goal,
            top: args.top,
            threshold: args.threshold,
            chunk_by: args.chunk_by,
            chunk_size: args.chunk_size,
            format: args.format.clone(),
        },
    );
    print!("{}", ctx_echo::format::render(&result, &args.format));
    if result.exit_code == 0 {
        Ok(ExitCode::SUCCESS)
    } else {
        // Mirror Go's cobra ExitError{"", Code}: cobra prints "Error: \n" to
        // stderr after the command body when RunE returns an ExitError.
        print_cobra_empty_error();
        Ok(ExitCode::from(result.exit_code as u8))
    }
}
