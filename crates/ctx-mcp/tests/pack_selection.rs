//! `ctx_pack` with an explicit `files` selection or a saved `mix`.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use ctx_mcp::{serve, ServeOptions};
use serde_json::{json, Value};

fn temp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ctx-mcp-sel-{tag}-{}-{}",
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

fn call(root: &Path, args: Value) -> Value {
    let req = json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":"ctx_pack","arguments":args}});
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

#[test]
fn files_are_packed_in_order_and_skips_are_reported() {
    let root = repo("files");
    let body = text(&call(
        &root,
        json!({"path": ".", "files": ["src/b.rs", "src/a.rs", "../x", "gone.rs"], "goal": "nothing matches"}),
    ));
    let b = body.find("### src/b.rs").expect(&body);
    let a = body.find("### src/a.rs").expect(&body);
    assert!(b < a, "{body}");
    assert!(!body.contains("# readme"));
    assert!(body.contains("- ../x (outside the repository)"), "{body}");
    assert!(body.contains("- gone.rs (not found)"), "{body}");

    let json_body = text(&call(
        &root,
        json!({"path": ".", "files": ["README.md", "src/a.rs"], "budget": 5, "format": "json"}),
    ));
    let pack: Value = serde_json::from_str(&json_body).unwrap();
    assert_eq!(pack["included"].as_array().unwrap().len(), 1, "{json_body}");
    assert!(pack["skipped"][0]["reason"]
        .as_str()
        .unwrap()
        .starts_with("budget exceeded"));
}

#[test]
fn files_cannot_be_combined_with_changed_or_mix() {
    let root = repo("conflict");
    let reply = call(
        &root,
        json!({"path": ".", "files": ["src/a.rs"], "changed": true}),
    );
    assert!(reply["error"].is_object(), "{reply}");
    let reply = call(
        &root,
        json!({"path": ".", "files": ["src/a.rs"], "mix": "x"}),
    );
    assert!(reply["error"].is_object(), "{reply}");
    let reply = call(
        &root,
        json!({"path": ".", "files": ["src/a.rs"], "format": "xml"}),
    );
    assert!(
        reply["error"].is_object() || reply["result"]["isError"] == true,
        "{reply}"
    );
}

#[test]
fn mix_packs_a_saved_recipe_with_its_goal_and_budget() {
    // The only test in this binary that touches the mix store.
    let state = temp("state");
    std::env::set_var("XDG_STATE_HOME", &state);
    let root = repo("mix");
    let store = ctx_mix::Store::open_default().unwrap();
    store
        .create(ctx_mix::Draft {
            name: "Alpha work".to_string(),
            goal: "alpha".to_string(),
            files: vec!["src/a.rs".to_string(), "README.md".to_string()],
            budget: 7777,
            ..Default::default()
        })
        .unwrap();
    let body = text(&call(
        &root,
        json!({"path": ".", "mix": "Alpha work", "format": "json"}),
    ));
    let pack: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(pack["budget"], 7777);
    assert_eq!(pack["goal"], "alpha");
    assert_eq!(pack["mix"], "Alpha work (alpha-work)");
    let paths: Vec<&str> = pack["included"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, vec!["src/a.rs", "README.md"]);

    // An explicit budget overrides the recipe's.
    let body = text(&call(
        &root,
        json!({"path": ".", "mix": "alpha-work", "budget": 3000, "format": "json"}),
    ));
    let pack: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(pack["budget"], 3000);

    let reply = call(&root, json!({"path": ".", "mix": "nope"}));
    let err = reply.to_string();
    assert!(err.contains("recipe not found"), "{err}");
}

#[test]
fn include_and_exclude_scope_a_directory_walk() {
    let root = repo("filters");
    std::fs::create_dir_all(root.join("src/tests")).unwrap();
    std::fs::write(root.join("src/tests/t.rs"), "fn t() {}\n").unwrap();
    let body = text(&call(
        &root,
        json!({"path": ".", "include": ["*.rs"], "exclude": ["tests", "b.rs"], "format": "json"}),
    ));
    let pack: Value = serde_json::from_str(&body).unwrap();
    let included: Vec<&str> = pack["included"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["path"].as_str().unwrap())
        .collect();
    assert_eq!(included, vec!["src/a.rs"], "{body}");

    let reply = call(&root, json!({"path": ".", "include": ["*.{rs"]}));
    assert!(reply["error"].is_object(), "{reply}");
    let reply = call(
        &root,
        json!({"path": ".", "files": ["src/a.rs"], "include": ["*.rs"]}),
    );
    assert!(reply["error"].is_object(), "{reply}");
}

#[test]
fn overflow_signatures_packs_declarations_of_a_file_over_budget() {
    let root = repo("overflow");
    let big: String = (0..200)
        .map(|i| format!("pub fn handler_{i}() -> u32 {{\n    let v = {i};\n    v * 2 + 1\n}}\n\n"))
        .collect();
    std::fs::write(root.join("src/handlers.rs"), big).unwrap();
    let args = |overflow: &str| json!({"path": ".", "goal": "handlers", "budget": 2500, "overflow": overflow, "format": "markdown"});
    let dropped = text(&call(&root, args("drop")));
    assert!(
        dropped.contains("- src/handlers.rs (budget exceeded)"),
        "{dropped}"
    );
    let kept = text(&call(&root, args("signatures")));
    assert!(
        kept.contains("- src/handlers.rs (2.4k tokens, signatures only)"),
        "{kept}"
    );
    assert!(kept.contains("1: pub fn handler_0() -> u32 {\n"), "{kept}");
    assert!(!kept.contains("let v = 0;"), "{kept}");
    assert!(!kept.contains("(budget exceeded)"), "{kept}");

    let reply = call(&root, args("trim"));
    assert!(reply["error"].is_object(), "{reply}");
}

#[test]
fn files_line_ranges_pack_the_body_a_signature_digest_pointed_at() {
    let root = repo("excerpt");
    let big: String = (0..200)
        .map(|i| format!("pub fn handler_{i}() -> u32 {{\n    let v = {i};\n    v * 2 + 1\n}}\n\n"))
        .collect();
    std::fs::write(root.join("src/handlers.rs"), big).unwrap();

    // The digest names the declaration line and the hint says how to ask.
    let digest = text(&call(
        &root,
        json!({"path": ".", "goal": "handlers", "budget": 2500, "overflow": "signatures"}),
    ));
    assert!(
        digest.contains("16: pub fn handler_3() -> u32 {\n"),
        "{digest}"
    );
    assert!(
        digest.contains(r#"ctx_pack {"files":["src/handlers.rs:START-END"]}"#),
        "{digest}"
    );

    // Asking for lines 16-19 packs exactly that body.
    let body = text(&call(
        &root,
        json!({"path": ".", "files": ["src/handlers.rs:16-19", "src/a.rs", "src/handlers.rs#L1200"]}),
    ));
    assert!(
        body.contains("- src/handlers.rs (") && body.contains("lines 16-19 of 1000)"),
        "{body}"
    );
    assert!(body.contains("    let v = 3;\n"), "{body}");
    assert!(!body.contains("let v = 4;"), "{body}");
    assert!(body.contains("pub fn alpha()"), "{body}");
    assert!(
        body.contains(
            "- src/handlers.rs:1200 (line range 1200 is past the end of the file (1000 lines))"
        ),
        "{body}"
    );

    let json_body = text(&call(
        &root,
        json!({"path": ".", "files": ["src/handlers.rs:16-19"], "format": "json"}),
    ));
    let pack: Value = serde_json::from_str(&json_body).unwrap();
    assert_eq!(
        pack["included"][0]["lines"], "lines 16-19 of 1000",
        "{json_body}"
    );
}
