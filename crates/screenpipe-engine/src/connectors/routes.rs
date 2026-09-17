// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

//! Generic connector channel routes (`/connections/rss/...`), same verb set
//! as the office surface. Handlers construct the service per request from
//! `AppState` (the service is stateless) and talk through the `Connector`
//! trait, so a new channel's routes are this file's verb set — nothing more.

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;

use super::office_adapter::OfficeConnector;
use super::rss::RssService;
use super::{channels_index, Connector, ConnectorError};
use crate::server::AppState;

pub(crate) fn rss_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(status))
        .route("/refresh", post(refresh))
        .route("/scope", put(save_scope))
        .route("/sync", post(start_sync))
        .route("/control", post(control))
        .route("/disconnect", post(disconnect))
        .route("/search", get(search))
}

/// The office surface addresses sub-providers via `/:provider`; RSS has one
/// entry, so its routes are flat and the key is fixed.
const KEY: &str = "rss";

fn service(state: &Arc<AppState>) -> Arc<dyn Connector> {
    Arc::new(RssService::new(state.db.clone()))
}

fn err_response(e: ConnectorError) -> Response {
    let status = StatusCode::from_u16(e.http).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    (status, Json(json!({ "code": e.code, "message": e.message }))).into_response()
}

async fn status(State(state): State<Arc<AppState>>) -> Response {
    match service(&state).status(KEY).await {
        Ok(status) => Json(status).into_response(),
        Err(e) => err_response(e),
    }
}

async fn refresh(State(state): State<Arc<AppState>>) -> Response {
    match service(&state).refresh(KEY).await {
        Ok(status) => Json(status).into_response(),
        Err(e) => err_response(e),
    }
}

async fn save_scope(
    State(state): State<Arc<AppState>>,
    Json(scope): Json<serde_json::Value>,
) -> Response {
    match service(&state).save_scope(KEY, &scope).await {
        Ok(scope_revision) => Json(json!({ "scope_revision": scope_revision })).into_response(),
        Err(e) => err_response(e),
    }
}

#[derive(Deserialize)]
struct SyncRequest {
    expected_revision: i64,
}

async fn start_sync(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SyncRequest>,
) -> Response {
    match service(&state).start_sync(KEY, req.expected_revision).await {
        Ok(run_id) => Json(json!({ "run_id": run_id })).into_response(),
        Err(e) => err_response(e),
    }
}

#[derive(Deserialize)]
struct ControlRequest {
    action: String,
}

async fn control(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ControlRequest>,
) -> Response {
    match service(&state).control(KEY, &req.action).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => err_response(e),
    }
}

#[derive(Deserialize)]
struct DisconnectRequest {
    local_data: String,
}

async fn disconnect(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DisconnectRequest>,
) -> Response {
    match service(&state).disconnect(KEY, &req.local_data).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => err_response(e),
    }
}

#[derive(Deserialize)]
struct SearchQuery {
    query: String,
    limit: Option<u32>,
}

async fn search(
    State(state): State<Arc<AppState>>,
    Query(q): Query<SearchQuery>,
) -> Response {
    match service(&state).search(&q.query, q.limit.unwrap_or(20)).await {
        Ok(hits) => Json(hits).into_response(),
        Err(e) => err_response(e),
    }
}

/// GET /connections/channels — aggregate status of every connector channel,
/// built from the shared registry (office adapter + native channels).
pub(crate) fn channels_routes() -> Router<Arc<AppState>> {
    Router::new().route("/", get(channels))
}

async fn channels(State(state): State<Arc<AppState>>) -> Response {
    let registry: Vec<Arc<dyn Connector>> = vec![
        Arc::new(OfficeConnector::new(
            state.db.clone(),
            state.screenpipe_dir.join("office-cli"),
        )),
        Arc::new(RssService::new(state.db.clone())),
    ];
    Json(channels_index(&registry).await).into_response()
}
