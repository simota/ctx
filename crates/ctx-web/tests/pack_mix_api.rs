//! `POST /api/pack` and the `/api/mix` mutations used by the pinned-files
//! board: pack a selection, save it as a mix, pack the mix, delete it.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;

struct Running {
    port: u16,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    handle: Option<thread::JoinHandle<()>>,
}

impl Running {
    fn start(root: PathBuf) -> Self {
        let (port_tx, port_rx) = mpsc::channel();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let handle = thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async move {
                let mut server =
                    ctx_web::Server::new(root.to_string_lossy().into_owned(), "127.0.0.1:0", false);
                server.listen().await.unwrap();
                port_tx.send(server.addr().unwrap().port()).unwrap();
                let _ = server
                    .serve(async move {
                        let _ = shutdown_rx.await;
                    })
                    .await;
            });
        });
        let port = port_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        Running {
            port,
            shutdown_tx: Some(shutdown_tx),
            handle: Some(handle),
        }
    }

    fn request(&self, method: &str, path: &str, extra_headers: &str, body: &str) -> (u16, String) {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let req = format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{extra_headers}Content-Length: {len}\r\nConnection: close\r\n\r\n{body}",
            port = self.port,
            len = body.len()
        );
        stream.write_all(req.as_bytes()).unwrap();
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).unwrap();
        let text = String::from_utf8_lossy(&raw).into_owned();
        let status = text
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let body = text
            .split_once("\r\n\r\n")
            .map(|(_, b)| b.to_string())
            .unwrap_or_default();
        (status, body)
    }

    fn json(&self, method: &str, path: &str, body: &str) -> (u16, Value) {
        let (status, body) = self.request(method, path, "Content-Type: application/json\r\n", body);
        let value = serde_json::from_str(&body).unwrap_or(Value::Null);
        (status, value)
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn temp(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("ctx-web-pack-{tag}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn fixture() -> PathBuf {
    let dir = temp("repo");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/login.rs"), "pub fn login() {}\n").unwrap();
    std::fs::write(dir.join("README.md"), "# Demo\n").unwrap();
    std::fs::write(
        dir.join(".env"),
        "TOKEN=ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n",
    )
    .unwrap();
    dir
}

#[test]
fn pins_pack_save_and_reuse_as_a_mix() {
    // The only test in this binary that touches the mix store.
    let state = temp("state");
    std::env::set_var("XDG_STATE_HOME", &state);
    let root = fixture();
    let srv = Running::start(root.clone());

    // Pack the board's selection, in order.
    let (status, body) = srv.json(
        "POST",
        "/api/pack",
        r#"{"files":["README.md","src/login.rs","../outside","nope.rs"],"goal":"login","budget":1000}"#,
    );
    assert_eq!(status, 200, "{body}");
    let included: Vec<&str> = body["included"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["path"].as_str().unwrap())
        .collect();
    assert_eq!(included, vec!["README.md", "src/login.rs"]);
    assert_eq!(body["skipped"].as_array().unwrap().len(), 2);
    let content = body["content"].as_str().unwrap();
    assert!(content.contains("**Goal**: login"));
    assert!(content.find("### README.md").unwrap() < content.find("### src/login.rs").unwrap());

    // Save it as a mix.
    let (status, mix) = srv.json(
        "POST",
        "/api/mix",
        r#"{"name":"Login work","goal":"login","files":["README.md","src/login.rs"],"budget":1000}"#,
    );
    assert_eq!(status, 201, "{mix}");
    assert_eq!(mix["id"], "login-work");
    assert_eq!(mix["budget"]["limit"], 1000);

    // A file that is not in the served root is refused.
    let (status, err) = srv.json(
        "POST",
        "/api/mix",
        r#"{"name":"x","files":["../etc/passwd"]}"#,
    );
    assert_eq!(status, 400, "{err}");
    let (status, err) = srv.json("POST", "/api/mix", r#"{"name":"x","files":["missing.rs"]}"#);
    assert_eq!(status, 400, "{err}");
    // ...unless the caller asks to drop them.
    let (status, partial) = srv.json(
        "POST",
        "/api/mix",
        r#"{"name":"partial","files":["missing.rs","README.md"],"skip_missing":true}"#,
    );
    assert_eq!(status, 201, "{partial}");
    assert_eq!(partial["files"], serde_json::json!(["README.md"]));
    assert_eq!(partial["skipped"], serde_json::json!(["missing.rs"]));
    let (status, _) = srv.request("DELETE", "/api/mix/partial", "", "");
    assert_eq!(status, 204);

    let (status, list) = srv.json("GET", "/api/mix", "");
    assert_eq!(status, 200);
    assert_eq!(list["mixes"][0]["id"], "login-work");
    assert_eq!(list["mixes"][0]["file_count"], 2);

    // Pack by mix: the recipe's goal and budget apply.
    let (status, packed) = srv.json(
        "POST",
        "/api/pack",
        r#"{"mix":"login-work","format":"plain","contract":false}"#,
    );
    assert_eq!(status, 200, "{packed}");
    assert_eq!(packed["budget"], 1000);
    assert!(packed["content"]
        .as_str()
        .unwrap()
        .contains("=== src/login.rs ==="));

    // The CLI store sees the same recipe.
    assert!(state.join("ctx/mixes/login-work.mix.json").is_file());

    let (status, _) = srv.request("DELETE", "/api/mix/login-work", "", "");
    assert_eq!(status, 204);
    let (status, _) = srv.request("DELETE", "/api/mix/login-work", "", "");
    assert_eq!(status, 404);
    let (status, _) = srv.json("POST", "/api/pack", r#"{"mix":"login-work"}"#);
    assert_eq!(status, 404);
}

#[test]
fn pack_redacts_secrets_and_validates_input() {
    let root = fixture();
    let srv = Running::start(root);
    let (status, body) = srv.json("POST", "/api/pack", r#"{"files":[".env"]}"#);
    assert_eq!(status, 200, "{body}");
    assert!(
        !body["content"].as_str().unwrap().contains("ghp_aaaa"),
        "{body}"
    );

    let (status, _) = srv.json("POST", "/api/pack", r#"{"files":[]}"#);
    assert_eq!(status, 400);
    let (status, _) = srv.json(
        "POST",
        "/api/pack",
        r#"{"files":["README.md"],"format":"xml"}"#,
    );
    assert_eq!(status, 400);
    let (status, _) = srv.json(
        "POST",
        "/api/pack",
        r#"{"files":["README.md"],"budget":-1}"#,
    );
    assert_eq!(status, 400);
    let (status, _) = srv.json("POST", "/api/pack", r#"{"files":["README.md"],"bogus":1}"#);
    assert_eq!(status, 400);
}

#[test]
fn state_changing_requests_must_be_same_origin_json() {
    let root = fixture();
    let srv = Running::start(root);
    let body = r#"{"files":["README.md"]}"#;
    // A cross-site form post cannot set a JSON content type.
    let (status, _) = srv.request("POST", "/api/pack", "Content-Type: text/plain\r\n", body);
    assert_eq!(status, 415);
    // Cross-site fetches are labelled by the browser.
    let (status, out) = srv.request(
        "POST",
        "/api/mix",
        "Content-Type: application/json\r\nOrigin: https://attacker.example\r\n",
        r#"{"name":"x","files":["README.md"]}"#,
    );
    assert_eq!(status, 403, "{out}");
    assert!(out.contains("forbidden_origin"));
    let (status, _) = srv.request(
        "DELETE",
        "/api/mix/anything",
        "Sec-Fetch-Site: cross-site\r\n",
        "",
    );
    assert_eq!(status, 403);
    let (status, _) = srv.request(
        "POST",
        "/api/pack",
        "Content-Type: application/json\r\nOrigin: null\r\n",
        body,
    );
    assert_eq!(status, 403);
    // The served page itself passes.
    let origin = format!("Content-Type: application/json\r\nOrigin: http://127.0.0.1:{}\r\nSec-Fetch-Site: same-origin\r\n", srv.port);
    let (status, out) = srv.request("POST", "/api/pack", &origin, body);
    assert_eq!(status, 200, "{out}");
    // GETs are unaffected.
    let (status, _) = srv.request(
        "GET",
        "/api/mix",
        "Origin: https://attacker.example\r\n",
        "",
    );
    assert_eq!(status, 200);
}
