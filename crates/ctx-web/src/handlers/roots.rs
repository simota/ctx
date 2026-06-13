//! `GET /api/roots` — port of `internal/web/roots_api.go` `handleRootsList`.
//!
//! Returns the list of registered ctx roots from `~/.ctx/roots.toml`,
//! sorted alphabetically by name (case-insensitive), matching Go's
//! `RootsFile.Sorted()` output.
//!
//! Registry isolation: `CTX_ROOTS_FILE` env var overrides the default path,
//! which lets the parity harness point both Go and Rust servers at the same
//! pinned fixture file.
//!
//! IMPORTANT — time.Time JSON serialization: Go's `encoding/json` serializes
//! a zero `time.Time` value as `"0001-01-01T00:00:00Z"` (NOT omitted), because
//! `omitempty` only suppresses nil pointers/maps/slices/interfaces, NOT struct
//! values. Fields like `last_opened_at time.Time \`json:"last_opened_at,omitempty"\``
//! are ALWAYS emitted, including when the underlying TOML field is absent
//! (which yields zero `time.Time{}`). We reproduce this by using "" as the
//! sentinel for "absent" and emitting "0001-01-01T00:00:00Z" in that case.
//!
//! Only GET is handled here; POST/DELETE are mutation endpoints restricted to
//! loopback and are DEFERRED — they are not registered in the router so the
//! Go server's method-routing behaviour (returns 405 for POST) is preserved
//! on the Rust side by axum's default method-not-allowed response.

use std::path::{Path, PathBuf};

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;
use serde::{Deserialize, Serialize};

use crate::response;
use crate::AppState;

/// The zero-value RFC3339 timestamp Go uses for an absent `time.Time` field.
const GO_ZERO_TIME: &str = "0001-01-01T00:00:00Z";

// ---------------------------------------------------------------------------
// On-disk TOML schema (mirrors ctx-cli's RootsFile / RootEntry)
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
struct RootsFile {
    #[serde(default)]
    roots: Vec<RootEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct RootEntry {
    name: String,
    path: String,
    // Stored as RFC3339 string from TOML (go-toml writes time.Time as RFC3339).
    // Absent field → None → serialized as "0001-01-01T00:00:00Z".
    #[serde(default, deserialize_with = "deser_datetime_opt")]
    added_at: Option<String>,
    #[serde(default, deserialize_with = "deser_datetime_opt")]
    last_opened_at: Option<String>,
}

/// Deserialize a TOML datetime or string field into an `Option<String>`.
fn deser_datetime_opt<'de, D>(d: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v: Option<toml::Value> = Option::deserialize(d)?;
    match v {
        None => Ok(None),
        Some(toml::Value::Datetime(dt)) => Ok(Some(dt.to_string())),
        Some(toml::Value::String(s)) => Ok(Some(s)),
        Some(other) => Ok(Some(other.to_string())),
    }
}

// ---------------------------------------------------------------------------
// Wire types (JSON)
// ---------------------------------------------------------------------------

/// Mirrors `web.RootEntry`. Go serializes `time.Time` fields as RFC3339 always
/// (even zero value → "0001-01-01T00:00:00Z"). Both `added_at` and
/// `last_opened_at` are always emitted.
#[derive(Serialize)]
struct RootEntryWire {
    name: String,
    path: String,
    /// Always emitted — zero time when absent from TOML.
    added_at: String,
    /// Always emitted — zero time when absent from TOML.
    last_opened_at: String,
}

/// Mirrors `web.RootsListResponse`.
#[derive(Serialize)]
struct RootsListResponse {
    roots: Vec<RootEntryWire>,
    #[serde(skip_serializing_if = "Option::is_none")]
    warning: Option<String>,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

pub async fn handle(State(_state): State<AppState>) -> Response {
    roots_response_from_file(roots_path())
}

fn roots_response_from_file(registry_path: Result<PathBuf, String>) -> Response {
    let registry_path = match registry_path {
        Ok(p) => p,
        Err(e) => {
            return roots_warning_response(&e);
        }
    };

    let rf = match load_roots(&registry_path) {
        Ok(rf) => rf,
        Err(e) => {
            return roots_warning_response(&e);
        }
    };

    // Sort by name, case-insensitive — mirrors Go `RootsFile.Sorted()`.
    let mut sorted = rf.roots;
    sorted.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
    });

    let roots: Vec<RootEntryWire> = sorted
        .into_iter()
        .map(|r| RootEntryWire {
            name: r.name,
            path: r.path,
            // Absent in TOML → None → emit Go zero time
            added_at: r.added_at.unwrap_or_else(|| GO_ZERO_TIME.to_string()),
            last_opened_at: r.last_opened_at.unwrap_or_else(|| GO_ZERO_TIME.to_string()),
        })
        .collect();

    response::json(
        StatusCode::OK,
        &RootsListResponse {
            roots,
            warning: None,
        },
    )
}

fn roots_warning_response(_detail: &str) -> Response {
    response::json(
        StatusCode::OK,
        &RootsListResponse {
            roots: Vec::new(),
            warning: Some(
                "roots registry could not be loaded; run `ctx roots list` to inspect it"
                    .to_string(),
            ),
        },
    )
}

// ---------------------------------------------------------------------------
// Registry loading (mirrors ctx-cli `roots_path` + `load_roots`)
// ---------------------------------------------------------------------------

/// Mirrors Go `config.RootsPath` / ctx-cli `roots_path`.
/// `CTX_ROOTS_FILE` env var overrides the default `~/.ctx/roots.toml`.
fn roots_path() -> Result<PathBuf, String> {
    if let Ok(path) = std::env::var("CTX_ROOTS_FILE") {
        let path = path.trim().to_string();
        if !path.is_empty() {
            return Ok(expand_home(&path));
        }
    }
    let home = std::env::var("HOME").map_err(|e| format!("roots: locate home dir: {e}"))?;
    Ok(PathBuf::from(home).join(".ctx").join("roots.toml"))
}

fn expand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(path)
}

/// Mirrors Go `config.LoadRootsFrom` / ctx-cli `load_roots`.
fn load_roots(path: &Path) -> Result<RootsFile, String> {
    if !path.exists() {
        return Ok(RootsFile::default());
    }
    let raw = std::fs::read_to_string(path)
        .map_err(|e| format!("roots: read {}: {e}", path.display()))?;
    toml::from_str(&raw).map_err(|e| format!("roots: decode {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use http_body_util::BodyExt;

    use super::*;

    #[tokio::test]
    async fn malformed_roots_registry_returns_warning_not_500() {
        let mut path = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("ctx-web-bad-roots-{unique}.toml"));
        std::fs::write(&path, "roots = [").unwrap();

        let response = roots_response_from_file(Ok(path.clone()));
        let status = response.status();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body = String::from_utf8(body.to_vec()).unwrap();

        let _ = std::fs::remove_file(&path);

        assert_eq!(status, StatusCode::OK);
        assert!(body.contains(r#""roots":[]"#), "body: {body}");
        assert!(
            body.contains(r#""warning":"roots registry could not be loaded;"#),
            "body: {body}",
        );
        assert!(!body.contains(&path.display().to_string()), "body: {body}");
    }
}
