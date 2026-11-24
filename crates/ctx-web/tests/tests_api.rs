//! `GET /api/tests` outside Go: a source file lists the tests named after
//! it or importing it, and a test file lists the sources it exercises.

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

fn fixture() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("ctx-web-tests-api-{}-{nanos}", std::process::id()));
    for (path, body) in [
        ("web/src/session.ts", "export const ttl = 1;\n"),
        (
            "web/src/__tests__/session.test.ts",
            "import { ttl } from '../session';\n",
        ),
        ("app/db.py", "def connect():\n    pass\n"),
        ("tests/test_db.py", "from app.db import connect\n"),
        ("notes.txt", "hello\n"),
    ] {
        let full = dir.join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, body).unwrap();
    }
    dir
}

#[test]
fn tests_api_pairs_sources_and_tests_in_any_language() {
    let srv = Running::start(fixture());

    let (status, body) = srv.json("GET", "/api/tests?path=web/src/session.ts", "");
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["kind"], "paired", "{body}");
    assert_eq!(
        body["tests"][0]["path"],
        "web/src/__tests__/session.test.ts"
    );
    assert_eq!(body["tests"][0]["score"], 3);
    assert_eq!(body["tests"][0]["reasons"][0], "named after it");
    assert_eq!(body["total_tests"], 1);

    let (status, body) = srv.json("GET", "/api/tests?path=tests/test_db.py", "");
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["kind"], "paired-test", "{body}");
    assert_eq!(body["tests"].as_array().unwrap().len(), 0, "{body}");
    assert_eq!(body["sources"][0]["path"], "app/db.py", "{body}");

    // Non-code files keep the empty, kind-less envelope.
    let (status, body) = srv.json("GET", "/api/tests?path=notes.txt", "");
    assert_eq!(status, 200, "{body}");
    assert!(body.get("kind").is_none(), "{body}");
}
