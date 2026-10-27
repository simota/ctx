//! In-process robustness checks for `ctx_mcp::serve`: root containment of the
//! directory walkers and survival of malformed frames. Unlike `parity.rs`
//! these need no Go oracle.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use ctx_mcp::{serve, ServeOptions};
use serde_json::{json, Value};

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ctx-mcp-robust-{tag}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::canonicalize(dir).unwrap()
}

/// Feed raw NDJSON bytes to the server and return one parsed value per reply.
fn run_raw(root: &Path, input: &[u8]) -> Vec<Value> {
    let mut out = Vec::new();
    serve(
        Cursor::new(input.to_vec()),
        &mut out,
        ServeOptions {
            root: root.to_path_buf(),
            allow_outside_root: false,
        },
    )
    .expect("serve must not fail on a malformed frame");
    String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn call_tool(root: &Path, name: &str, args: Value) -> String {
    let req = json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":name,"arguments":args}});
    let replies = run_raw(root, format!("{req}\n").as_bytes());
    assert_eq!(replies.len(), 1, "{replies:?}");
    replies[0].to_string()
}

#[cfg(unix)]
#[test]
fn walkers_do_not_follow_symlinks_out_of_root() {
    let outside = temp_dir("outside");
    std::fs::write(outside.join("secret.rs"), "fn leaked_outside() {}\n").unwrap();
    let root = temp_dir("root");
    std::fs::write(root.join("main.rs"), "fn inside_root() {}\n").unwrap();
    std::os::unix::fs::symlink(outside.join("secret.rs"), root.join("leak.rs")).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("outdir")).unwrap();
    // An in-root file link stays visible.
    std::os::unix::fs::symlink(root.join("main.rs"), root.join("alias.rs")).unwrap();

    for (tool, args) in [
        ("ctx_pack", json!({})),
        ("ctx_where", json!({"query": "leaked_outside"})),
        ("ctx_symbols", json!({})),
        ("ctx_budget", json!({})),
    ] {
        let body = call_tool(&root, tool, args);
        assert!(!body.contains("leaked_outside"), "{tool} leaked: {body}");
        assert!(
            !body.contains("secret.rs"),
            "{tool} listed outside file: {body}"
        );
    }
    let pack = call_tool(&root, "ctx_pack", json!({}));
    assert!(pack.contains("inside_root"), "{pack}");
    assert!(pack.contains("alias.rs"), "in-root symlink dropped: {pack}");

    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(outside);
}

#[cfg(unix)]
#[test]
fn walkers_do_not_recurse_into_symlink_cycles() {
    let root = temp_dir("cycle");
    std::fs::write(root.join("a.rs"), "fn only_once() {}\n").unwrap();
    std::os::unix::fs::symlink(".", root.join("loop")).unwrap();

    let body = call_tool(&root, "ctx_where", json!({"query": "only_once"}));
    assert!(!body.contains("loop/"), "walked through cycle: {body}");
    assert_eq!(body.matches("anchor: only_once@").count(), 1, "{body}");
    for tool in ["ctx_pack", "ctx_symbols", "ctx_budget"] {
        let body = call_tool(&root, tool, json!({}));
        assert!(
            !body.contains("loop/"),
            "{tool} walked through cycle: {body}"
        );
    }

    let _ = std::fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn subdirectory_walk_keeps_links_inside_server_root() {
    let root = temp_dir("subdir");
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("shared.rs"), "fn shared_helper() {}\n").unwrap();
    std::os::unix::fs::symlink(root.join("shared.rs"), root.join("src/link.rs")).unwrap();

    // The link leaves the walked directory but not the server root, so the
    // result must not depend on where the walk starts.
    for path in [".", "src"] {
        let body = call_tool(
            &root,
            "ctx_where",
            json!({"query": "shared_helper", "path": path}),
        );
        assert!(body.contains("link.rs"), "path={path}: {body}");
    }

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn invalid_utf8_frame_does_not_end_the_session() {
    let root = temp_dir("utf8");
    let mut input = b"\xff\xfe\n".to_vec();
    input.extend_from_slice(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n");

    let replies = run_raw(&root, &input);
    assert_eq!(replies.len(), 2, "{replies:?}");
    assert_eq!(replies[0]["error"]["code"], json!(-32700));
    assert_eq!(replies[1]["id"], json!(2));
    assert!(replies[1].get("error").is_none(), "{:?}", replies[1]);

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn long_multibyte_prompt_argument_is_truncated_not_panicked() {
    let root = temp_dir("prompt");
    let focus = format!("a{}", "é".repeat(600));
    let req = json!({"jsonrpc":"2.0","id":3,"method":"prompts/get",
        "params":{"name":"onboard-codebase","arguments":{"focus_area":focus}}});

    let replies = run_raw(&root, format!("{req}\n").as_bytes());
    assert_eq!(replies.len(), 1);
    assert!(replies[0].get("error").is_none(), "{:?}", replies[0]);
    let text = replies[0]["result"]["messages"][0]["content"]["text"]
        .as_str()
        .unwrap();
    assert!(text.contains("aéé"), "{text}");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn pack_redacts_secrets_in_file_contents() {
    let token = ["ghp_", "123456789012345678901234567890123456"].concat();
    let root = temp_dir("redact");
    std::fs::write(
        root.join("main.rs"),
        format!("const T: &str = \"{token}\";\nfn kept() {{}}\n"),
    )
    .unwrap();
    let pack = call_tool(&root, "ctx_pack", json!({}));
    assert!(!pack.contains(&token), "token leaked: {pack}");
    assert!(pack.contains("kind=github_pat"), "{pack}");
    assert!(pack.contains("fn kept()"), "{pack}");
}
