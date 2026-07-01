use std::env;
use std::ffi::{OsStr, OsString};
use std::io::{self, BufReader, Write};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::common::*;

#[derive(Debug)]
pub(crate) struct BrowseArgs {
    bind: String,
    allow_nonlocal: bool,
    /// Selected web engine: only "rust" (native axum server in this binary)
    /// is supported — there is no Go fallback any more. Also settable via
    /// `CTX_WEB_ENGINE`.
    web_engine: String,
    /// Resolved bind port (0 = ephemeral). Parsed from `--port`, falling back
    /// to the port embedded in `--bind host:port` when --port is not given.
    port: u16,
    /// Served project path (positional; defaults to ".").
    path: String,
    /// Do not launch the system browser after the server URL is ready.
    no_open: bool,
}

/// Native `ctx tui [path]` — route to the ratatui ctx-tui crate.
///
/// Mirrors `internal/cli/tui.go`: optional path positional (default "."),
/// `walk.New/Walk` + `countTokens` is done by `ctx_tui::build_tree`, then
/// `ctx_tui::run` drives the event loop. Always returns `Some(..)` (a native
/// ExitCode) — `tui` is fully cut over, so it must never fall through to the
/// Go delegate. In a non-TTY (e.g. closed stdin) `run` fails fast and we map
/// the error to a non-zero exit instead of blocking or delegating.
pub(crate) fn run_tui_command(args: &[OsString]) -> Option<ExitCode> {
    // args[0] == "tui". Accept an optional single path positional.
    let mut path: Option<String> = None;
    for arg in &args[1..] {
        if is_option(arg) {
            // Unknown flag on tui: still native (no delegate), surface usage err.
            eprintln!("ctx tui: unrecognized argument {arg:?}");
            return Some(ExitCode::from(2));
        }
        if path.is_some() {
            eprintln!("ctx tui: accepts at most one path argument");
            return Some(ExitCode::from(2));
        }
        path = Some(arg.to_string_lossy().into_owned());
    }
    let root_path = path.as_deref().unwrap_or(".");

    // Fail fast on a non-TTY BEFORE the (potentially expensive) filesystem walk,
    // so a non-interactive invocation exits immediately instead of walking the
    // tree only to bail when entering raw mode. This is the native path running
    // (no delegate to Go), as the cutover oracle requires.
    if !ctx_tui::is_interactive() {
        eprintln!("ctx tui: requires an interactive terminal (TTY)");
        return Some(ExitCode::from(1));
    }

    let tree = match ctx_tui::build_tree(root_path) {
        Ok(t) => t,
        Err(err) => {
            eprintln!("ctx tui: {err}");
            return Some(ExitCode::from(1));
        }
    };
    match ctx_tui::run(tree, root_path) {
        Ok(()) => Some(ExitCode::SUCCESS),
        Err(err) => {
            eprintln!("ctx tui: {err}");
            Some(ExitCode::from(1))
        }
    }
}

pub(crate) fn run_browse_command(args: &[OsString]) -> Option<ExitCode> {
    let parsed = parse_browse_args(args)?;
    let host = browse_bind_host(&parsed.bind);
    if !parsed.allow_nonlocal && !is_loopback_host(&host) {
        eprintln!(
            "refusing to bind to non-loopback {:?}; pass --allow-nonlocal to override",
            host
        );
        return Some(ExitCode::from(1));
    }
    // `--web-engine rust` (or CTX_WEB_ENGINE=rust) runs the native axum
    // server in THIS binary. There is no Go fallback any more, so any other
    // value is a usage error — not a delegate signal.
    if parsed.web_engine == "rust" {
        return Some(serve_rust_web(&parsed, &host));
    }
    eprintln!(
        "ctx browse: unknown --web-engine value {:?}: only \"rust\" is supported",
        parsed.web_engine
    );
    Some(ExitCode::from(2))
}

/// Run the native ctx-web (axum) server headlessly, blocking until the process
/// is signalled. Prints the same `ctx browse: serving … at <url>` line the Go
/// server emits so the parent browser-launcher can parse the URL unchanged.
pub(crate) fn serve_rust_web(parsed: &BrowseArgs, host: &str) -> ExitCode {
    let bind = format!("{host}:{}", parsed.port);
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(err) => {
            eprintln!("ctx browse: failed to start runtime: {err}");
            return ExitCode::from(1);
        }
    };
    rt.block_on(async move {
        let mut server = ctx_web::Server::new(parsed.path.clone(), bind, false);
        if let Err(err) = server.listen().await {
            eprintln!("ctx browse: failed to bind: {err}");
            return ExitCode::from(1);
        }
        let addr = match server.addr() {
            Some(a) => a,
            None => {
                eprintln!("ctx browse: listener not bound");
                return ExitCode::from(1);
            }
        };
        let url = format!("http://{addr}/");
        // Match the Go output line exactly so URL-parsing parents keep working.
        println!("ctx browse: serving {} at {}", parsed.path, url);
        let _ = io::stdout().flush();
        if !parsed.no_open {
            if let Err(err) = open_url(&url) {
                eprintln!("warning: could not launch browser: {err}");
            }
        }
        let shutdown = async {
            let _ = tokio::signal::ctrl_c().await;
        };
        match server.serve(shutdown).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("ctx browse: server error: {err}");
                ExitCode::from(1)
            }
        }
    })
}

pub(crate) fn parse_browse_args(args: &[OsString]) -> Option<BrowseArgs> {
    let mut saw_browse = false;
    let mut bind = "127.0.0.1".to_string();
    let mut allow_nonlocal = false;
    let mut web_engine = match std::env::var("CTX_WEB_ENGINE") {
        Ok(value) if !value.is_empty() => value,
        _ => "rust".to_string(),
    };
    let mut port: u16 = 0;
    let mut port_set = false;
    let mut no_open = false;
    let mut positionals = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == OsStr::new("browse") {
            if saw_browse {
                return None;
            }
            saw_browse = true;
        } else if let Some(value) = flag_value(arg, "--bind") {
            bind = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--bind") {
            i += 1;
            bind = args.get(i)?.to_string_lossy().into_owned();
        } else if let Some(value) = flag_value(arg, "--web-engine") {
            web_engine = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--web-engine") {
            i += 1;
            web_engine = args.get(i)?.to_string_lossy().into_owned();
        } else if let Some(value) = flag_value(arg, "--port") {
            port = value.to_string_lossy().parse().ok()?;
            port_set = true;
        } else if arg == OsStr::new("--port") {
            i += 1;
            port = args.get(i)?.to_string_lossy().parse().ok()?;
            port_set = true;
        } else if arg == OsStr::new("--allow-nonlocal") {
            allow_nonlocal = true;
        } else if arg == OsStr::new("--no-open") {
            no_open = true;
        } else if arg == OsStr::new("--audit") || arg == OsStr::new("--no-register") {
        } else if flag_value(arg, "--relations-engine").is_some() {
        } else if arg == OsStr::new("--relations-engine") {
            i += 1;
            args.get(i)?;
        } else if is_option(arg) {
            return None;
        } else if saw_browse {
            positionals.push(arg.clone());
        } else {
            return None;
        }
        i += 1;
    }
    if !saw_browse || positionals.len() > 1 {
        return None;
    }
    let path = positionals
        .first()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| ".".to_string());
    // `--bind host:port` carries a port; honor it unless --port was given.
    if !port_set {
        if let Some(bind_port) = browse_bind_port(&bind) {
            port = bind_port;
        }
    }
    Some(BrowseArgs {
        bind,
        allow_nonlocal,
        web_engine,
        port,
        path,
        no_open,
    })
}

#[derive(Debug)]
pub(crate) struct McpServeArgs {
    root: PathBuf,
    allow_outside_root: bool,
}

pub(crate) fn run_mcp_command(args: &[OsString]) -> Option<ExitCode> {
    match parse_mcp_serve_args(args)? {
        Ok(parsed) => Some(serve_mcp_stdio(parsed)),
        Err(message) => {
            eprintln!("{message}");
            Some(ExitCode::from(2))
        }
    }
}

pub(crate) fn parse_mcp_serve_args(args: &[OsString]) -> Option<Result<McpServeArgs, String>> {
    if args.len() < 2 || args[0] != OsStr::new("mcp") || args[1] != OsStr::new("serve") {
        return None;
    }

    let mut root: Option<PathBuf> = None;
    let mut allow_outside_root = false;
    let mut i = 2;
    while i < args.len() {
        let arg = &args[i];
        if let Some(value) = flag_value(arg, "--root") {
            root = Some(PathBuf::from(value));
        } else if arg == OsStr::new("--root") {
            i += 1;
            let Some(value) = args.get(i) else {
                return Some(Err("ctx mcp serve: --root requires a value".to_string()));
            };
            root = Some(PathBuf::from(value));
        } else if arg == OsStr::new("--allow-outside-root") {
            allow_outside_root = true;
        } else if flag_value(arg, "--log-file").is_some() {
        } else if arg == OsStr::new("--log-file") {
            i += 1;
            if args.get(i).is_none() {
                return Some(Err("ctx mcp serve: --log-file requires a value".to_string()));
            }
        } else if is_option(arg) {
            return Some(Err(format!(
                "ctx mcp serve: unrecognized argument {:?}",
                arg.to_string_lossy()
            )));
        } else {
            return Some(Err(format!(
                "ctx mcp serve: unexpected argument {:?}",
                arg.to_string_lossy()
            )));
        }
        i += 1;
    }

    let root = match absolute_mcp_root(root.as_deref().unwrap_or_else(|| Path::new("."))) {
        Ok(root) => root,
        Err(err) => return Some(Err(err)),
    };
    Some(Ok(McpServeArgs {
        root,
        allow_outside_root,
    }))
}

pub(crate) fn absolute_mcp_root(root: &Path) -> Result<PathBuf, String> {
    if root.is_absolute() {
        return Ok(root.to_path_buf());
    }
    let cwd = env::current_dir().map_err(|err| format!("ctx mcp serve: cwd: {err}"))?;
    Ok(cwd.join(root))
}

pub(crate) fn serve_mcp_stdio(parsed: McpServeArgs) -> ExitCode {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let opts = ctx_mcp::ServeOptions {
        root: parsed.root,
        allow_outside_root: parsed.allow_outside_root,
    };
    match ctx_mcp::serve(BufReader::new(stdin.lock()), stdout.lock(), opts) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("ctx mcp serve: {err}");
            ExitCode::FAILURE
        }
    }
}

pub(crate) fn browse_bind_host(bind: &str) -> String {
    if let Ok(addr) = bind.parse::<std::net::SocketAddr>() {
        return addr.ip().to_string();
    }
    if let Some((host, port)) = bind.rsplit_once(':') {
        if port.parse::<u16>().is_ok() && !host.is_empty() {
            return host.trim_matches(['[', ']']).to_string();
        }
    }
    bind.to_string()
}

/// Port embedded in a `--bind` value, mirroring `browse_bind_host`'s
/// host:port forms. None when the bind is host-only.
pub(crate) fn browse_bind_port(bind: &str) -> Option<u16> {
    if let Ok(addr) = bind.parse::<std::net::SocketAddr>() {
        return Some(addr.port());
    }
    let (host, port) = bind.rsplit_once(':')?;
    if host.is_empty() {
        return None;
    }
    port.parse::<u16>().ok()
}

pub(crate) fn is_loopback_host(host: &str) -> bool {
    if host == "localhost" {
        return true;
    }
    host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os_args(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    #[test]
    fn parse_browse_opens_browser_by_default() {
        let parsed = parse_browse_args(&os_args(&["browse"])).expect("parse browse");
        assert!(!parsed.no_open);
    }

    #[test]
    fn parse_browse_honors_no_open() {
        let parsed =
            parse_browse_args(&os_args(&["browse", ".", "--no-open"])).expect("parse browse");
        assert!(parsed.no_open);
        assert_eq!(parsed.path, ".");
    }
}
