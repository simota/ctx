//! `GET /api/relations` — port of `internal/web/handlers.go` `handleRelations`.
//!
//! Returns import/importer edges for a file. Reuses `ctx-relations::build_cached`
//! to build the repository-wide import graph, then queries edges for the target.
//!
//! Response envelope mirrors `web.RelationsResponse`:
//!   { path, module_path?, imports: [{path}], importers: [{path}] }
//!
//! Unsupported file extensions return an empty (non-error) envelope, matching Go.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use serde::{Deserialize, Serialize};

use crate::handlers::file::relative_to_root;
use crate::response;
use crate::safepath;
use crate::AppState;

#[derive(Deserialize)]
pub struct RelationsParams {
    #[serde(default)]
    path: String,
}

/// RelationItem mirrors `web.RelationItem`.
#[derive(Serialize)]
struct RelationItem {
    path: String,
}

/// RelationsResponse mirrors `web.RelationsResponse`. Field order matches Go struct.
/// `module_path` is omitted when empty (`omitempty`).
#[derive(Serialize)]
struct RelationsResponse {
    path: String,
    #[serde(skip_serializing_if = "str::is_empty")]
    module_path: String,
    imports: Vec<RelationItem>,
    importers: Vec<RelationItem>,
}

pub async fn handle(
    State(state): State<AppState>,
    Query(params): Query<RelationsParams>,
) -> Response {
    // build_cached walks + parses the whole repo on a cold cache; keep it off
    // the tokio workers.
    crate::blocking::run(move || handle_sync(state, params)).await
}

fn handle_sync(state: AppState, params: RelationsParams) -> Response {
    if params.path.is_empty() {
        return response::error(StatusCode::BAD_REQUEST, "bad_request", "path is required");
    }

    let target = match safepath::resolve(&state.root, &params.path) {
        Ok(t) => t,
        Err(e) => return response::bad_path(e),
    };

    // Stat the target — mirror Go's os.Stat behaviour.
    let info = match std::fs::metadata(&target) {
        Ok(m) => m,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                return response::error(
                    StatusCode::NOT_FOUND,
                    "not_found",
                    &format!("stat {}: no such file or directory", target.display()),
                );
            }
            return response::error(StatusCode::INTERNAL_SERVER_ERROR, "stat", &e.to_string());
        }
    };
    if info.is_dir() {
        return response::error(StatusCode::BAD_REQUEST, "not_a_file", "path is a directory");
    }

    let rel_slash = relative_to_root(&state.root, &target);

    // Build the empty response first; returned as-is when not supported.
    let mut resp = RelationsResponse {
        path: rel_slash.clone(),
        module_path: String::new(),
        imports: vec![],
        importers: vec![],
    };

    if !ctx_relations::supported(&rel_slash) {
        return response::json(StatusCode::OK, &resp);
    }

    // build_cached is process-local, mirrors Go's RelationsPool.RoutedEdges.
    let index = match ctx_relations::build_cached(&state.root) {
        Ok(idx) => idx,
        Err(e) => {
            return response::error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "relations",
                &e.to_string(),
            );
        }
    };

    resp.module_path = index.module_path.clone();

    let edges = index.edges(&rel_slash);
    resp.imports = edges
        .imports
        .into_iter()
        .map(|p| RelationItem { path: p })
        .collect();
    resp.importers = edges
        .importers
        .into_iter()
        .map(|p| RelationItem { path: p })
        .collect();

    response::json(StatusCode::OK, &resp)
}
