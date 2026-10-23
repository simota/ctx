//! DNS-rebinding guard.
//!
//! A server bound to loopback is reachable from any page the user's browser
//! loads: an attacker domain that re-resolves to 127.0.0.1 becomes
//! same-origin with `ctx browse` and could read the repository through
//! `/raw/*` and `/api/*`. Such a page still sends its own name as `Host`, so
//! a loopback-bound server only answers requests whose `Host` is a loopback
//! IP literal or `localhost` (any port). Requests without a `Host` (HTTP/1.0
//! tooling) are not browser requests and pass.
//!
//! A non-loopback bind (`--allow-nonlocal`) is exposed on purpose and reached
//! through whatever name the operator gives the machine, so it is not
//! filtered.

use std::net::{Ipv4Addr, Ipv6Addr};

use axum::extract::{Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::Response;

use crate::response;
use crate::AppState;

/// Middleware: reject requests whose `Host` does not name the loopback
/// interface when the server itself is bound to loopback.
pub async fn guard(State(state): State<AppState>, req: Request, next: Next) -> Response {
    if state.loopback_only && !request_host_allowed(&req) {
        return response::error(
            StatusCode::FORBIDDEN,
            "forbidden_host",
            "Host is not a loopback name; open the URL printed by ctx browse",
        );
    }
    next.run(req).await
}

fn request_host_allowed(req: &Request) -> bool {
    let mut hosts = req.headers().get_all(header::HOST).iter().peekable();
    if hosts.peek().is_none() {
        // HTTP/2 carries the authority in the URI instead of a Host header.
        return match req.uri().authority() {
            Some(authority) => is_loopback_authority(authority.as_str()),
            None => true,
        };
    }
    hosts.all(|value| value.to_str().is_ok_and(is_loopback_authority))
}

/// True when `authority` (`host` or `host:port`) names the loopback interface.
pub fn is_loopback_authority(authority: &str) -> bool {
    if let Some(rest) = authority.strip_prefix('[') {
        let Some((ip, after)) = rest.split_once(']') else {
            return false;
        };
        let port_ok = after.is_empty() || after.strip_prefix(':').is_some_and(is_port);
        return port_ok && ip.parse::<Ipv6Addr>().is_ok_and(|ip| ip.is_loopback());
    }
    let host = match authority.rsplit_once(':') {
        Some((host, port)) if is_port(port) => host,
        Some(_) => return false,
        None => authority,
    };
    host.eq_ignore_ascii_case("localhost")
        || host.parse::<Ipv4Addr>().is_ok_and(|ip| ip.is_loopback())
}

fn is_port(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) && s.parse::<u16>().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_authorities_are_allowed() {
        for ok in [
            "127.0.0.1",
            "127.0.0.1:8080",
            "127.9.9.9:1",
            "localhost",
            "LOCALHOST:65535",
            "[::1]",
            "[::1]:8080",
        ] {
            assert!(is_loopback_authority(ok), "{ok}");
        }
    }

    #[test]
    fn other_authorities_are_rejected() {
        for bad in [
            "",
            "attacker.example",
            "attacker.example:8080",
            "localhost.attacker.example",
            "127.0.0.1.nip.io:8080",
            "0.0.0.0:8080",
            "10.0.0.1:8080",
            "[::]:8080",
            "[::ffff:127.0.0.1]:8080",
            "::1",
            "[::1",
            "[::1]x",
            "[::1]:",
            "127.0.0.1:",
            "127.0.0.1:99999",
            "127.0.0.1:+80",
            "127.0.0.1:80:80",
            "localhost.",
        ] {
            assert!(!is_loopback_authority(bad), "{bad}");
        }
    }
}
