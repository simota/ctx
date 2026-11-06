use std::io::{BufRead, Write};
use std::path::PathBuf;

mod budget;
mod cochange;
mod pack;
mod prompts;
mod protocol;
mod resources;
mod server;
mod symbols;
mod tools;
mod types;
mod util;
mod where_q;
use crate::protocol::*;
use crate::server::Server;

const PROTOCOL_VERSION: &str = "2024-11-05";

const MAX_PATH_LEN: usize = 4096;

const MAX_QUERY_LEN: usize = 256;

const MAX_LIMIT: i64 = 1_000;

const MAX_PAGE_SIZE: i64 = 500;

const MAX_CURSOR_LEN: usize = 256;

const MAX_GOAL_LEN: usize = 1024;

const MAX_BUDGET: i64 = 1_000_000;

const MAX_LANG_LEN: usize = 32;

const MAX_SINCE_LEN: usize = 32;

const MAX_TOP: i64 = 200;

const MAX_ANCHOR_LEN: usize = 256;

const MAX_COMMITS: i64 = 10_000;

const MAX_COMPANIONS: i64 = 50;
/// Upper bound on `ctx_pack` `files` entries.
const MAX_PACK_FILES: usize = 1_000;
/// Upper bound on `ctx_pack` `include` / `exclude` patterns (each).
const MAX_PACK_PATTERNS: usize = 64;
/// Upper bound on `ctx_pack` `refs` text.
const MAX_REFS_LEN: usize = 256 * 1024;
/// Upper bound on `ctx_pack` `around`.
const MAX_AROUND: i64 = 1_000;
/// `ctx_pack` `around` when `refs` is given without one.
const DEFAULT_REFS_AROUND: usize = 10;
/// Upper bound on a `ctx_pack` `mix` reference (id or name).
const MAX_MIX_REF_LEN: usize = 200;

const MAX_MIN_WEIGHT: i64 = 1_000;

const MAX_HOPS: i64 = 2;

const MAX_DEPTH: i64 = 16;

const FILE_RESOURCE_PREFIX: &str = "ctx://file/";

const MAX_FILE_RESOURCE_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone)]
pub struct ServeOptions {
    pub root: PathBuf,
    pub allow_outside_root: bool,
}

impl Default for ServeOptions {
    fn default() -> Self {
        let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self {
            root,
            allow_outside_root: false,
        }
    }
}

pub fn serve<R: BufRead, W: Write>(
    reader: R,
    mut writer: W,
    opts: ServeOptions,
) -> std::io::Result<()> {
    let server = Server::new(opts);
    // Split on raw bytes rather than `lines()`: one non-UTF-8 frame must not
    // abort the whole session. Invalid sequences become U+FFFD, as Go's
    // `json.Unmarshal` does for the `bufio.Scanner` transport this mirrors.
    for line in reader.split(b'\n') {
        let line = line?;
        let line = String::from_utf8_lossy(&line);
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Request>(line) {
            Ok(req) => {
                if req.id.is_none() {
                    continue;
                }
                // Isolate handler panics so one malformed request cannot crash the
                // whole stdio server; reply with -32603 (internal error) instead.
                // The handler holds only `&Server` (read-only), so there is no
                // shared mutable state to be left poisoned by an unwind.
                let id = req.id.clone();
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| server.handle(req)))
                {
                    Ok(response) => response,
                    Err(_) => Response {
                        jsonrpc: "2.0",
                        id,
                        result: None,
                        error: Some(RpcError {
                            code: -32603,
                            message: "internal error".to_string(),
                            data: None,
                        }),
                    },
                }
            }
            Err(err) => Response {
                jsonrpc: "2.0",
                id: None,
                result: None,
                error: Some(RpcError {
                    code: -32700,
                    message: parse_error_message(line, &err),
                    data: None,
                }),
            },
        };
        let mut line = serde_json::to_string(&response)?;
        line = line
            .replace('&', "\\u0026")
            .replace('<', "\\u003c")
            .replace('>', "\\u003e");
        writer.write_all(line.as_bytes())?;
        writer.write_all(b"\n")?;
        writer.flush()?;
    }
    Ok(())
}
