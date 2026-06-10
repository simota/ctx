use std::io::{BufRead, Write};
use std::path::PathBuf;

mod budget;
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
    for line in reader.lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Request>(line) {
            Ok(req) => {
                if req.id.is_none() {
                    continue;
                }
                server.handle(req)
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
