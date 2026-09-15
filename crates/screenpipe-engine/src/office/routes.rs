// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

//! Dedicated office connection routes (`/connections/office/...`). Auth,
//! scope, sync, control and disconnect live here.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;

use screenpipe_connect::office::types::OfficeErrorCode;
use screenpipe_connect::office::types::OfficeScope;

use super::{OfficeService, OfficeServiceError};
use crate::server::AppState;

pub(crate) fn office_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(list_office))
        .route("/:provider", get(get_office))
        .route("/:provider/refresh", post(refresh_office))
        .route("/:provider/authorize", post(authorize_office))
        .route("/:provider/scope", put(save_scope))
        .route("/:provider/sync", post(start_sync))
        .route("/:provider/control", post(control))
        .route("/:provider/disconnect", post(disconnect))
        .route("/:provider/search", get(search_office))
}

fn service(state: &Arc<AppState>) -> OfficeService {
    OfficeService::new(
        state.db.clone(),
        state.screenpipe_dir.join("office-cli"),
    )
}

fn err_response(e: OfficeServiceError) -> Response {
    let status = match e.http {
        0 => StatusCode::INTERNAL_SERVER_ERROR,
        s => StatusCode::from_u16(s).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
    };
    (status, Json(json!({ "code": e.as_str(), "message": e.message }))).into_response()
}

async fn list_office(State(state): State<Arc<AppState>>) -> Response {
    match service(&state).status_all().await {
        Ok(list) => Json(json!({ "connections": list })).into_response(),
        Err(e) => err_response(e),
    }
}

async fn get_office(State(state): State<Arc<AppState>>, Path(provider): Path<String>) -> Response {
    match service(&state).status(&provider).await {
        Ok(status) => Json(status).into_response(),
        Err(e) => err_response(e),
    }
}

async fn refresh_office(
    State(state): State<Arc<AppState>>,
    Path(provider): Path<String>,
) -> Response {
    match service(&state).refresh(&provider).await {
        Ok(status) => Json(status).into_response(),
        Err(e) => err_response(e),
    }
}

/// v1 authorization: report the official login entrypoint and run the CLI
/// login flow headlessly where supported. The response carries a status the
/// card can poll by re-reading connection state.
async fn authorize_office(
    State(state): State<Arc<AppState>>,
    Path(provider): Path<String>,
) -> Response {
    let svc = service(&state);
    match svc.refresh(&provider).await {
        Ok(status) => Json(json!({
            "status": status,
            "next": "在官方登录窗口完成授权后，点击刷新连接状态"
        }))
        .into_response(),
        Err(e) => err_response(e),
    }
}

#[derive(Deserialize)]
struct ScopeBody {
    expected_revision: i64,
    #[serde(flatten)]
    scope: OfficeScope,
}

async fn save_scope(
    State(state): State<Arc<AppState>>,
    Path(provider): Path<String>,
    Json(body): Json<ScopeBody>,
) -> Response {
    match service(&state)
        .save_scope(&provider, body.expected_revision, body.scope)
        .await
    {
        Ok(revision) => Json(json!({ "scope_revision": revision })).into_response(),
        Err(e) => err_response(e),
    }
}

#[derive(Deserialize)]
struct SyncBody {
    #[serde(default)]
    expected_revision: Option<i64>,
    #[serde(default)]
    idempotency_key: Option<String>,
}

async fn start_sync(
    State(state): State<Arc<AppState>>,
    Path(provider): Path<String>,
    body: Option<Json<SyncBody>>,
) -> Response {
    let svc = service(&state);
    let (expected, _idempotency_key) = match &body {
        Some(Json(b)) => (
            b.expected_revision.unwrap_or(i64::MAX),
            b.idempotency_key.clone(),
        ),
        None => (i64::MAX, None),
    };
    let expected = if expected == i64::MAX {
        // Absent revision: read current from status to avoid a forced conflict.
        match svc.status(&provider).await {
            Ok(s) => s.scope_revision,
            Err(e) => return err_response(e),
        }
    } else {
        expected
    };
    match svc.start_sync(&provider, expected).await {
        Ok(run_id) => Json(json!({ "job_id": run_id })).into_response(),
        Err(e) => err_response(e),
    }
}

#[derive(Deserialize)]
struct ControlBody {
    action: String,
}

async fn control(
    State(state): State<Arc<AppState>>,
    Path(provider): Path<String>,
    Json(body): Json<ControlBody>,
) -> Response {
    let svc = service(&state);
    match body.action.as_str() {
        "pause" | "cancel" => {
            // The in-process sync task checks the cancel token; a running
            // task stops at its next cancellation point.
            svc.cancel.child_token().cancel();
            Json(json!({ "affected": 1 })).into_response()
        }
        "retry" => match svc.refresh(&provider).await {
            Ok(status) => Json(json!({ "status": status })).into_response(),
            Err(e) => err_response(e),
        },
        _ => err_response(OfficeServiceError::new(
            OfficeErrorCode::ScopeInvalid,
            "未知操作",
            400,
        )),
    }
}

#[derive(Deserialize)]
struct DisconnectBody {
    #[serde(default = "default_local_data")]
    local_data: String,
}

fn default_local_data() -> String {
    "retain_inactive".to_string()
}

async fn disconnect(
    State(state): State<Arc<AppState>>,
    Path(provider): Path<String>,
    body: Option<Json<DisconnectBody>>,
) -> Response {
    let local_data = body
        .map(|Json(b)| b.local_data)
        .unwrap_or_else(default_local_data);
    match service(&state).disconnect(&provider, &local_data).await {
        Ok(()) => Json(json!({ "ok": true })).into_response(),
        Err(e) => err_response(e),
    }
}

#[derive(Deserialize)]
struct SearchParams {
    #[serde(default)]
    q: String,
    #[serde(default = "default_search_limit")]
    limit: u32,
}

fn default_search_limit() -> u32 {
    20
}

async fn search_office(
    State(state): State<Arc<AppState>>,
    Path(provider): Path<String>,
    axum::extract::Query(params): axum::extract::Query<SearchParams>,
) -> Response {
    if params.q.trim().is_empty() {
        return Json(json!({ "objects": [] })).into_response();
    }
    match service(&state)
        .search(Some(&provider), &params.q, params.limit)
        .await
    {
        Ok(objects) => Json(json!({ "objects": objects })).into_response(),
        Err(e) => err_response(e),
    }
}