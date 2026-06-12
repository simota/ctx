//! JSON / error response helpers mirroring `internal/web/handlers.go`
//! `writeJSON` / `writeError` / `writeBadPath`.
//!
//! Byte-fidelity rules pinned from the Go server:
//!   * `Content-Type: application/json; charset=utf-8`
//!   * HTML escaping DISABLED (`enc.SetEscapeHTML(false)`) — `<`, `>`, `&`
//!     pass through verbatim.
//!   * a trailing newline is appended (Go's `json.Encoder.Encode`).
//!   * field order follows struct declaration order; we use
//!     `serde_json` with `preserve_order` and ordered structs to match.

use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use crate::safepath::PathError;

/// Serialize `body` to the exact byte form Go's `writeJSON` produces (HTML
/// escaping disabled, trailing newline appended). Exposed so handlers can cache
/// the byte form and replay it via [`json_bytes`] without re-serializing.
pub fn to_json_bytes<T: Serialize>(body: &T) -> Vec<u8> {
    // serde_json does not HTML-escape by default, matching SetEscapeHTML(false).
    let mut buf = serde_json::to_vec(body).unwrap_or_else(|_| b"null".to_vec());
    buf.push(b'\n'); // Go's Encoder.Encode appends '\n'.
    buf
}

/// Wrap pre-serialized JSON bytes (as produced by [`to_json_bytes`]) in an axum
/// response with the canonical content type.
pub fn json_bytes(status: StatusCode, buf: Vec<u8>) -> Response {
    (
        status,
        [(
            header::CONTENT_TYPE,
            "application/json; charset=utf-8",
        )],
        buf,
    )
        .into_response()
}

/// Serialize `body` to the exact byte form Go's `writeJSON` produces and wrap
/// it in an axum response with the given status.
pub fn json<T: Serialize>(status: StatusCode, body: &T) -> Response {
    json_bytes(status, to_json_bytes(body))
}

/// The canonical error envelope: `{"error":{"code":...,"message":...}}`.
#[derive(Serialize)]
struct ErrorEnvelope<'a> {
    error: ErrorPayload<'a>,
}

#[derive(Serialize)]
struct ErrorPayload<'a> {
    code: &'a str,
    message: &'a str,
}

/// Mirror of `writeError`.
pub fn error(status: StatusCode, code: &str, message: &str) -> Response {
    json(status, &ErrorEnvelope {
        error: ErrorPayload { code, message },
    })
}

/// Mirror of `writeBadPath` — maps a [`PathError`] to its status/code/message.
pub fn bad_path(err: PathError) -> Response {
    error(StatusCode::BAD_REQUEST, err.code(), err.message())
}

/// Convert a typed `Query<T>` extraction rejection into the standard JSON
/// error envelope (axum's default rejection body is text/plain).
pub fn bad_query(rej: axum::extract::rejection::QueryRejection) -> Response {
    error(StatusCode::BAD_REQUEST, "bad_query", &rej.body_text())
}
