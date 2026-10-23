//! DNS-rebinding guard: a loopback-bound `ctx browse` server must only answer
//! requests whose `Host` names the loopback interface. A page on an attacker
//! domain that re-resolves to 127.0.0.1 sends its own name as `Host`; serving
//! it would let that page read the repository through `/raw/*` and `/api/*`.

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
    fn start(root: PathBuf, bind: &'static str) -> Self {
        let (port_tx, port_rx) = mpsc::channel();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let handle = thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async move {
                let mut server =
                    ctx_web::Server::new(root.to_string_lossy().into_owned(), bind, false);
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

    /// Send `GET path` with the given raw `Host` header value (or none) and
    /// return `(status, body)`.
    fn get(&self, path: &str, host: Option<&str>) -> (u16, String) {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).expect("connect");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let host_line = host.map(|h| format!("Host: {h}\r\n")).unwrap_or_default();
        let req = match host {
            Some(_) => format!("GET {path} HTTP/1.1\r\n{host_line}Connection: close\r\n\r\n"),
            // HTTP/1.0 does not require Host (non-browser clients).
            None => format!("GET {path} HTTP/1.0\r\n\r\n"),
        };
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
        "ctx-web-host-guard-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("secret.txt"), "repository contents\n").unwrap();
    dir
}

#[test]
fn loopback_server_rejects_rebound_host() {
    let dir = fixture("reject");
    let srv = Running::start(dir.clone(), "127.0.0.1:0");
    let port = srv.port;

    for host in [
        format!("attacker.example:{port}"),
        "attacker.example".to_string(),
        format!("localhost.attacker.example:{port}"),
        format!("127.0.0.1.attacker.example:{port}"),
        format!("ATTACKER.example:{port}"),
        // Unspecified / LAN addresses are not the loopback interface.
        format!("0.0.0.0:{port}"),
        format!("192.168.1.10:{port}"),
        // Malformed values must not slip through the port split.
        "[::1".to_string(),
        format!("127.0.0.1:{port}:{port}"),
        String::new(),
    ] {
        for path in ["/raw/secret.txt", "/api/file?path=secret.txt", "/"] {
            let (status, body) = srv.get(path, Some(&host));
            assert_eq!(status, 403, "Host {host:?} {path}: body {body}");
            assert!(
                !body.contains("repository contents"),
                "Host {host:?} {path} leaked file contents"
            );
            assert!(
                body.contains("\"code\":\"forbidden_host\""),
                "Host {host:?} {path}: unexpected body {body}"
            );
        }
    }

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn loopback_server_accepts_loopback_hosts() {
    let dir = fixture("accept");
    let srv = Running::start(dir.clone(), "127.0.0.1:0");
    let port = srv.port;

    for host in [
        format!("127.0.0.1:{port}"),
        "127.0.0.1".to_string(),
        format!("localhost:{port}"),
        format!("LocalHost:{port}"),
        format!("[::1]:{port}"),
        format!("127.0.0.2:{port}"),
    ] {
        let (status, body) = srv.get("/raw/secret.txt", Some(&host));
        assert_eq!(status, 200, "Host {host:?}: body {body}");
        assert!(body.contains("repository contents"), "Host {host:?}");
    }

    // No Host at all (HTTP/1.0 tooling) is not a browser request.
    let (status, body) = srv.get("/raw/secret.txt", None);
    assert_eq!(status, 200, "no Host: body {body}");

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn localhost_bind_is_guarded() {
    // `--bind localhost` resolves to a loopback address; the guard is decided
    // from that bound address, not the spelling.
    let dir = fixture("localhost-bind");
    let srv = Running::start(dir.clone(), "localhost:0");
    let port = srv.port;
    let (status, _) = srv.get("/raw/secret.txt", Some(&format!("attacker.example:{port}")));
    assert_eq!(status, 403);
    let (status, _) = srv.get("/raw/secret.txt", Some(&format!("localhost:{port}")));
    assert_eq!(status, 200);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn non_loopback_bind_does_not_filter_hosts() {
    // `--allow-nonlocal` exposes the server on purpose and is reached through
    // whatever name the operator gives the machine.
    let dir = fixture("nonlocal");
    let srv = Running::start(dir.clone(), "0.0.0.0:0");
    let (status, body) = srv.get("/raw/secret.txt", Some("devbox.lan:8080"));
    assert_eq!(status, 200, "body {body}");
    let _ = std::fs::remove_dir_all(dir);
}
