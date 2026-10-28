//! `/api/*` paths always answer with the JSON error envelope: an unknown
//! endpoint is `404 not_found` (not the SPA's index.html, which the frontend
//! reported as "Invalid JSON"), and the roots-picker mutations that are not
//! ported yet are `501 not_supported` with the equivalent CLI command.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
        let port = port_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("server bind");
        Running {
            port,
            shutdown_tx: Some(shutdown_tx),
            handle: Some(handle),
        }
    }

    /// Send `method path` and return `(status, content-type, body)`.
    fn request(&self, method: &str, path: &str) -> (u16, String, String) {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).expect("connect");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let req = format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}",
            self.port
        );
        stream.write_all(req.as_bytes()).unwrap();
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).unwrap();
        let text = String::from_utf8_lossy(&raw).into_owned();
        let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
        let status = head
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let content_type = head
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-type")
                    .then(|| value.trim().to_string())
            })
            .unwrap_or_default();
        (status, content_type, body.to_string())
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

fn fixture(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "ctx-web-api-fallback-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.txt"), "hello\n").unwrap();
    dir
}

#[test]
fn unknown_api_paths_get_a_json_404() {
    let dir = fixture("404");
    let srv = Running::start(dir.clone());
    for (method, path) in [
        ("GET", "/api/does-not-exist"),
        ("GET", "/api/git/nope?x=1"),
        ("POST", "/api/nope"),
        ("GET", "/api"),
    ] {
        let (status, content_type, body) = srv.request(method, path);
        assert_eq!(status, 404, "{method} {path}: {body}");
        assert!(
            content_type.starts_with("application/json"),
            "{method} {path}: {content_type}"
        );
        assert!(
            body.contains(r#""code":"not_found""#),
            "{method} {path}: {body}"
        );
    }

    // Non-API paths still get the SPA shell for client-side routing.
    let (status, content_type, _) = srv.request("GET", "/some/client/route");
    assert_eq!(status, 200);
    assert!(content_type.starts_with("text/html"), "{content_type}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn unported_roots_mutations_name_the_cli_command() {
    let dir = fixture("roots");
    let srv = Running::start(dir.clone());
    for (method, path, command) in [
        ("POST", "/api/roots", "ctx roots add"),
        ("DELETE", "/api/roots?name=alpha", "ctx roots remove"),
        ("POST", "/api/roots/open", "ctx roots open"),
    ] {
        let (status, content_type, body) = srv.request(method, path);
        assert_eq!(status, 501, "{method} {path}: {body}");
        assert!(
            content_type.starts_with("application/json"),
            "{method} {path}: {content_type}"
        );
        assert!(
            body.contains(r#""code":"not_supported""#),
            "{method} {path}: {body}"
        );
        assert!(body.contains(command), "{method} {path}: {body}");
    }
    let _ = std::fs::remove_dir_all(dir);
}
