use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use ctx_mcp::{serve, ServeOptions};
use serde_json::{json, Value};

fn temp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ctx-mcp-mix-{tag}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::canonicalize(dir).unwrap()
}

fn repo(tag: &str) -> PathBuf {
    let root = temp(tag);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/a.rs"), "pub fn alpha() {}\n").unwrap();
    std::fs::write(root.join("src/b.rs"), "pub fn beta() {}\n").unwrap();
    std::fs::write(root.join("README.md"), "# readme\n").unwrap();
    root
}

fn call(root: &Path, tool: &str, args: Value) -> Value {
    let req = json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":tool,"arguments":args}});
    let mut out = Vec::new();
    serve(
        Cursor::new(format!("{req}\n").into_bytes()),
        &mut out,
        ServeOptions {
            root: root.to_path_buf(),
            allow_outside_root: false,
        },
    )
    .unwrap();
    serde_json::from_slice(&out).unwrap()
}

fn text(reply: &Value) -> String {
    reply["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("no text: {reply}"))
        .to_string()
}

fn is_error(reply: &Value) -> bool {
    reply["result"]["isError"] == json!(true) || reply.get("error").is_some()
}

/// The only test in this binary: it points the mix store at a temp dir.
#[test]
fn an_agent_lists_saves_extends_and_packs_a_mix() {
    let state = temp("state");
    std::env::set_var("XDG_STATE_HOME", &state);
    let root = repo("flow");

    let empty = text(&call(&root, "ctx_mix", json!({})));
    assert!(empty.starts_with("No saved mixes."), "{empty}");

    // Save the working set, with a line range.
    let saved = text(&call(
        &root,
        "ctx_mix",
        json!({"action": "save", "name": "Alpha work", "goal": "fix alpha",
               "budget": 4000, "files": ["src/a.rs", "README.md:1-1"]}),
    ));
    assert!(saved.contains("# Mix Alpha work (alpha-work)"), "{saved}");
    assert!(saved.contains("- src/a.rs\n- README.md:1-1\n"), "{saved}");

    // Listing shows it; adding appends without duplicates.
    let list = text(&call(&root, "ctx_mix", json!({"action": "list"})));
    assert!(
        list.contains("| alpha-work | Alpha work | 2 | fix alpha |"),
        "{list}"
    );
    let added = text(&call(
        &root,
        "ctx_mix",
        json!({"action": "add", "mix": "Alpha work", "files": ["src/b.rs", "src/a.rs"]}),
    ));
    assert!(added.starts_with("Added 1 file(s)."), "{added}");
    assert!(
        added.contains("- src/a.rs\n- README.md:1-1\n- src/b.rs\n"),
        "{added}"
    );

    // The saved mix packs through ctx_pack.
    let pack = text(&call(
        &root,
        "ctx_pack",
        json!({"path": ".", "mix": "alpha-work"}),
    ));
    assert!(pack.contains("src/b.rs"), "{pack}");
    assert!(pack.contains("fix alpha"), "{pack}");

    // A file that is missing, outside the root or a directory is refused,
    // and nothing is saved.
    for bad in [
        json!(["nope.rs"]),
        json!(["../x.rs"]),
        json!(["src"]),
        json!(["/etc/hosts"]),
    ] {
        let reply = call(
            &root,
            "ctx_mix",
            json!({"action": "save", "name": "bad", "files": bad}),
        );
        assert!(is_error(&reply), "{bad}: {reply}");
    }
    let list = text(&call(&root, "ctx_mix", json!({"action": "list"})));
    assert!(!list.contains("| bad |"), "{list}");

    // Missing arguments are invalid params.
    for args in [
        json!({"action": "show"}),
        json!({"action": "save", "files": ["src/a.rs"]}),
        json!({"action": "add", "mix": "alpha-work"}),
        json!({"action": "rm", "mix": "alpha-work"}),
    ] {
        let reply = call(&root, "ctx_mix", args.clone());
        assert_eq!(reply["error"]["code"], -32602, "{args}: {reply}");
    }

    // show marks files missing under another root.
    let other = repo("other");
    std::fs::remove_file(other.join("src/b.rs")).unwrap();
    let shown = text(&call(
        &other,
        "ctx_mix",
        json!({"action": "show", "mix": "alpha-work"}),
    ));
    assert!(shown.contains("- src/b.rs (missing here)"), "{shown}");
}
