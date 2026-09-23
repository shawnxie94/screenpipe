// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

//! Local document manual import routes (`/documents/...`).
//!
//! The renderer reads the file, extracts text with the existing chat-attachment
//! parsers, and drives a three-step flow: upload bytes (content-addressed
//! managed copy), submit extracted text (chunked + FTS indexed), or record a
//! failure. Raw binaries are never persisted in SQLite — only metadata and
//! derived text.

use std::path::PathBuf;
use std::process::Command;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use sha2::Digest;

use crate::server::AppState;

/// Mirrors the renderer accept-list in
/// `apps/screenpipe-app-tauri/lib/pi/extract-document.ts`.
const SUPPORTED_EXTS: &[&str] = &[
    "pdf", "docx", "xlsx", "xls", "txt", "md", "markdown", "csv", "tsv",
    "json", "log", "yaml", "yml", "xml", "html", "htm", "rtf", "ini", "toml",
];

/// Same guard as the chat attachment path: refuse to slurp huge files.
const MAX_DOC_BYTES: i64 = 25 * 1024 * 1024;

pub(crate) fn documents_routes() -> Router<std::sync::Arc<AppState>> {
    Router::new()
        .route("/import", post(import_document))
        .route("/import-text", post(import_document_text))
        .route("/import-failed", post(import_document_failed))
        .route("/search", get(search_documents))
        .route("/list", get(list_documents))
        .route("/content", get(document_content))
        .route("/reveal", post(reveal_document))
        // Directory auto-ingest: watch-source CRUD plus the scan/location
        // reconciliation the native watcher drives.
        .route("/sources", get(list_sources).post(add_source))
        .route("/sources/update", post(update_source))
        .route("/sources/remove", post(remove_source))
        .route("/sources/scan", post(scan_source))
        .route("/locations/imported", post(location_imported))
        .route("/locations", get(list_locations))
}

#[derive(Deserialize)]
struct ImportQuery {
    filename: String,
    #[serde(default)]
    original_path: Option<String>,
}

fn ext_of(filename: &str) -> String {
    filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_lowercase()
}

fn err_json(status: StatusCode, code: &str, message: String) -> Response {
    (status, Json(json!({ "code": code, "message": message }))).into_response()
}

/// POST /documents/import?filename=&original_path=
/// Body: raw file bytes. Stores a content-addressed managed copy under
/// `<screenpipe_dir>/documents/<aa>/<sha256>.<ext>` and records metadata.
async fn import_document(
    State(state): State<std::sync::Arc<AppState>>,
    Query(q): Query<ImportQuery>,
    body: axum::body::Bytes,
) -> Response {
    let filename = q.filename.trim().to_string();
    if filename.is_empty() {
        return err_json(
            StatusCode::BAD_REQUEST,
            "missing_filename",
            "缺少文件名".to_string(),
        );
    }
    let ext = ext_of(&filename);
    if !SUPPORTED_EXTS.contains(&ext.as_str()) {
        return err_json(
            StatusCode::UNPROCESSABLE_ENTITY,
            "unsupported_type",
            format!("不支持的文件类型 .{ext}"),
        );
    }
    let size = body.len() as i64;
    if size == 0 {
        return err_json(
            StatusCode::BAD_REQUEST,
            "empty_file",
            format!("{filename} 是空文件"),
        );
    }
    if size > MAX_DOC_BYTES {
        let mb = format!("{:.1}", size as f64 / (1024.0 * 1024.0));
        return err_json(
            StatusCode::PAYLOAD_TOO_LARGE,
            "too_large",
            format!("{filename} 过大（{mb} MB），最大支持 25 MB"),
        );
    }

    let mut hasher = sha2::Sha256::new();
    Digest::update(&mut hasher, &body[..]);
    let sha256 = hex::encode(Digest::finalize(hasher));

    let managed_path =
        match store_managed_copy(&state.screenpipe_dir, &sha256, &ext, &body) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("document managed copy failed for {filename}: {e}");
                return err_json(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "store_failed",
                    format!("保存托管副本失败：{e}"),
                );
            }
        };

    match state
        .db
        .document_import_stored(
            &sha256,
            &filename,
            &ext,
            size,
            q.original_path.as_deref(),
            Some(managed_path.display().to_string()).as_deref(),
        )
        .await
    {
        Ok(created) => {
            let status_str = if created { "imported" } else { "duplicate" };
            (
                StatusCode::OK,
                Json(json!({
                    "sha256": sha256,
                    "status": status_str,
                    "managed_path": managed_path.display().to_string(),
                })),
            )
                .into_response()
        }
        Err(e) => {
            tracing::warn!("document metadata write failed: {e}");
            err_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                "db_failed",
                format!("写入文档记录失败：{e}"),
            )
        }
    }
}

fn store_managed_copy(
    screenpipe_dir: &std::path::Path,
    sha256: &str,
    ext: &str,
    bytes: &[u8],
) -> std::io::Result<PathBuf> {
    let dir = screenpipe_dir
        .join("documents")
        .join(&sha256[..2.min(sha256.len())]);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{sha256}.{ext}"));
    // Content-addressed: identical bytes always map to the same path, so an
    // existing file is by definition the same content — skip the rewrite.
    if !path.exists() {
        std::fs::write(&path, bytes)?;
    }
    Ok(path)
}

#[derive(Deserialize)]
struct ImportTextBody {
    sha256: String,
    text: String,
    #[serde(default)]
    truncated: bool,
}

/// POST /documents/import-text — submit renderer-extracted text for a stored
/// document. Empty text is recorded as a visible failure, not dropped.
async fn import_document_text(
    State(state): State<std::sync::Arc<AppState>>,
    Json(body): Json<ImportTextBody>,
) -> Response {
    if state.db.document_get(&body.sha256).await.ok().flatten().is_none() {
        return err_json(
            StatusCode::NOT_FOUND,
            "not_found",
            "文档尚未上传，请先调用 /documents/import".to_string(),
        );
    }
    match state
        .db
        .document_mark_ready(&body.sha256, &body.text, body.truncated)
        .await
    {
        Ok(new_state) => (
            StatusCode::OK,
            Json(json!({ "sha256": body.sha256, "state": new_state })),
        )
            .into_response(),
        Err(e) => {
            tracing::warn!("document text index failed: {e}");
            err_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                "db_failed",
                format!("写入可搜索文本失败：{e}"),
            )
        }
    }
}

#[derive(Deserialize)]
struct ImportFailedBody {
    #[serde(default)]
    sha256: Option<String>,
    filename: String,
    #[serde(default)]
    ext: Option<String>,
    #[serde(default)]
    size_bytes: Option<i64>,
    #[serde(default)]
    original_path: Option<String>,
    reason: String,
}

/// POST /documents/import-failed — record a visible, retryable failure
/// (unsupported type, oversize, unreadable, parser error).
async fn import_document_failed(
    State(state): State<std::sync::Arc<AppState>>,
    Json(body): Json<ImportFailedBody>,
) -> Response {
    let ext = body.ext.clone().unwrap_or_else(|| ext_of(&body.filename));
    match state
        .db
        .document_mark_failed(
            body.sha256.as_deref(),
            &body.filename,
            &ext,
            body.size_bytes.unwrap_or(0),
            body.original_path.as_deref(),
            &body.reason,
        )
        .await
    {
        Ok(()) => (StatusCode::OK, Json(json!({ "recorded": true }))).into_response(),
        Err(e) => {
            tracing::warn!("document failure record failed: {e}");
            err_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                "db_failed",
                format!("记录失败状态时出错：{e}"),
            )
        }
    }
}

#[derive(Deserialize)]
struct SearchQuery {
    #[serde(default)]
    q: String,
    #[serde(default)]
    limit: Option<u32>,
}

/// GET /documents/search?q=&limit= — FTS over imported document chunks with
/// snippets; empty query browses the most recent ready documents.
async fn search_documents(
    State(state): State<std::sync::Arc<AppState>>,
    Query(q): Query<SearchQuery>,
) -> Response {
    match state
        .db
        .document_search(&q.q, q.limit.unwrap_or(30))
        .await
    {
        Ok(rows) => (StatusCode::OK, Json(json!({ "data": rows }))).into_response(),
        Err(e) => {
            tracing::warn!("document search failed: {e}");
            err_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                "search_failed",
                format!("文档搜索失败：{e}"),
            )
        }
    }
}

/// GET /documents/list?limit= — all import states for observability.
async fn list_documents(
    State(state): State<std::sync::Arc<AppState>>,
    Query(q): Query<SearchQuery>,
) -> Response {
    match state.db.document_list(q.limit.unwrap_or(100)).await {
        Ok(rows) => (StatusCode::OK, Json(json!({ "data": rows }))).into_response(),
        Err(e) => err_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            "list_failed",
            format!("读取文档列表失败：{e}"),
        ),
    }
}

#[derive(Deserialize)]
struct ContentQuery {
    sha256: String,
}

/// GET /documents/content?sha256= — derived text chunks of one imported
/// document, in order, plus its metadata. Backs the renderer's in-app
/// document preview; the original file never has to exist for this to work.
async fn document_content(
    State(state): State<std::sync::Arc<AppState>>,
    Query(q): Query<ContentQuery>,
) -> Response {
    let doc = match state.db.document_get(&q.sha256).await {
        Ok(Some(doc)) => doc,
        Ok(None) => {
            return err_json(StatusCode::NOT_FOUND, "not_found", "文档不存在".to_string());
        }
        Err(e) => {
            return err_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                "db_failed",
                format!("读取文档记录失败：{e}"),
            );
        }
    };
    let chunks = match state.db.document_chunks(&q.sha256).await {
        Ok(chunks) => chunks,
        Err(e) => {
            return err_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                "db_failed",
                format!("读取文档内容失败：{e}"),
            );
        }
    };
    (
        StatusCode::OK,
        Json(json!({
            "data": {
                "sha256": doc.sha256,
                "file_name": doc.file_name,
                "ext": doc.ext,
                "original_path": doc.original_path,
                "managed_path": doc.managed_path,
                "imported_at": doc.imported_at,
                "chunks": chunks.into_iter()
                    .map(|(ordinal, body)| json!({ "ordinal": ordinal, "body": body }))
                    .collect::<Vec<_>>(),
            }
        })),
    )
        .into_response()
}

#[derive(Deserialize)]
struct RevealBody {
    sha256: String,
}

/// POST /documents/reveal — show the original file when it still exists,
/// otherwise reveal the managed copy. Keeps "回到原始文件" working after the
/// original moves, at the cost of revealing a duplicate.
async fn reveal_document(
    State(state): State<std::sync::Arc<AppState>>,
    Json(body): Json<RevealBody>,
) -> Response {
    let doc = match state.db.document_get(&body.sha256).await {
        Ok(Some(doc)) => doc,
        Ok(None) => {
            return err_json(StatusCode::NOT_FOUND, "not_found", "文档不存在".to_string());
        }
        Err(e) => {
            return err_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                "db_failed",
                format!("读取文档记录失败：{e}"),
            );
        }
    };

    let original = doc
        .original_path
        .as_deref()
        .map(std::path::Path::new)
        .filter(|p| p.is_file());
    let target = original
        .map(|p| p.to_path_buf())
        .or_else(|| doc.managed_path.as_deref().map(std::path::PathBuf::from));
    let Some(target) = target else {
        return err_json(
            StatusCode::NOT_FOUND,
            "no_source",
            "没有可打开的来源文件".to_string(),
        );
    };

    match reveal_in_file_manager(&target) {
        Ok(()) => (
            StatusCode::OK,
            Json(json!({ "revealed": target.display().to_string() })),
        )
            .into_response(),
        Err(e) => err_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            "reveal_failed",
            format!("打开文件失败：{e}"),
        ),
    }
}

fn reveal_in_file_manager(path: &std::path::Path) -> Result<(), String> {
    let result = cfg!(target_os = "macos")
        .then(|| Command::new("open").arg("-R").arg(path).status())
        .or_else(|| {
            cfg!(target_os = "windows")
                .then(|| {
                    Command::new("explorer")
                        .arg(format!("/select,{}", path.display()))
                        .status()
                })
        })
        .unwrap_or_else(|| {
            // Linux and other Unix: reveal the parent directory.
            let dir = path.parent().unwrap_or(path);
            Command::new("xdg-open").arg(dir).status()
        });
    match result {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(format!("文件管理器退出码 {status}")),
        Err(e) => Err(e.to_string()),
    }
}

// --- Directory auto-ingest (Roadmap step 2) --------------------------------
//
// The desktop app owns the watcher and the scanning; the engine owns the
// truth about which sources exist and which locations still need importing.
// The native side POSTs a full directory scan, the engine diffs it against
// `document_locations` (marking vanishances missing, never deleting managed
// copies) and answers with the paths that need importing.

/// GET /documents/sources — the user-chosen watch directories.
async fn list_sources(State(state): State<std::sync::Arc<AppState>>) -> Response {
    match state.db.document_source_list().await {
        Ok(rows) => (StatusCode::OK, Json(json!({ "data": rows }))).into_response(),
        Err(e) => err_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            "list_failed",
            format!("读取目录源失败：{e}"),
        ),
    }
}

#[derive(Deserialize)]
struct AddSourceBody {
    path: String,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    include_exts: Option<String>,
    #[serde(default)]
    exclude_globs: Option<String>,
}

/// POST /documents/sources — register a watch directory (idempotent per path).
async fn add_source(
    State(state): State<std::sync::Arc<AppState>>,
    Json(body): Json<AddSourceBody>,
) -> Response {
    let path = body.path.trim().to_string();
    if path.is_empty() {
        return err_json(StatusCode::BAD_REQUEST, "missing_path", "缺少目录路径".to_string());
    }
    let id = body
        .id
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    match state
        .db
        .document_source_add(
            &id,
            &path,
            body.include_exts.as_deref().unwrap_or(""),
            body.exclude_globs.as_deref().unwrap_or(""),
        )
        .await
    {
        Ok(true) => (
            StatusCode::OK,
            Json(json!({ "id": id, "path": path, "created": true })),
        )
            .into_response(),
        Ok(false) => (
            StatusCode::OK,
            Json(json!({ "id": id, "path": path, "created": false })),
        )
            .into_response(),
        Err(e) => err_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            "db_failed",
            format!("写入目录源失败：{e}"),
        ),
    }
}

#[derive(Deserialize)]
struct UpdateSourceBody {
    id: String,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    include_exts: Option<String>,
    #[serde(default)]
    exclude_globs: Option<String>,
}

/// POST /documents/sources/update — flip enable or change filters.
async fn update_source(
    State(state): State<std::sync::Arc<AppState>>,
    Json(body): Json<UpdateSourceBody>,
) -> Response {
    match state
        .db
        .document_source_update(
            &body.id,
            body.enabled,
            body.include_exts.as_deref(),
            body.exclude_globs.as_deref(),
        )
        .await
    {
        Ok(()) => (StatusCode::OK, Json(json!({ "updated": true }))).into_response(),
        Err(e) => err_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            "db_failed",
            format!("更新目录源失败：{e}"),
        ),
    }
}

#[derive(Deserialize)]
struct RemoveSourceBody {
    id: String,
}

/// POST /documents/sources/remove — stop watching; managed copies stay.
async fn remove_source(
    State(state): State<std::sync::Arc<AppState>>,
    Json(body): Json<RemoveSourceBody>,
) -> Response {
    match state.db.document_source_remove(&body.id).await {
        Ok(()) => (StatusCode::OK, Json(json!({ "removed": true }))).into_response(),
        Err(e) => err_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            "db_failed",
            format!("删除目录源失败：{e}"),
        ),
    }
}

#[derive(Deserialize)]
struct ScanSourceBody {
    source_id: String,
    files: Vec<screenpipe_db::ScannedFile>,
}

/// POST /documents/sources/scan — apply one reconcile/scan for a source and
/// answer which paths need importing now.
async fn scan_source(
    State(state): State<std::sync::Arc<AppState>>,
    Json(body): Json<ScanSourceBody>,
) -> Response {
    match state
        .db
        .document_source_scan_diff(&body.source_id, &body.files)
        .await
    {
        Ok(diff) => (
            StatusCode::OK,
            Json(json!({
                "to_import": diff.to_import,
                "missing": diff.missing,
            })),
        )
            .into_response(),
        Err(e) => err_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            "db_failed",
            format!("目录扫描对账失败：{e}"),
        ),
    }
}

#[derive(Deserialize)]
struct LocationImportedBody {
    source_id: String,
    path: String,
    #[serde(default)]
    sha256: Option<String>,
    /// "imported" or "failed" (a duplicate upload still counts as imported —
    /// the content is managed and searchable).
    status: String,
    #[serde(default)]
    error: Option<String>,
}

/// POST /documents/locations/imported — the importer page reports one file's
/// outcome so the location row stops showing as pending.
async fn location_imported(
    State(state): State<std::sync::Arc<AppState>>,
    Json(body): Json<LocationImportedBody>,
) -> Response {
    let status = match body.status.as_str() {
        "imported" => "imported",
        "failed" => "failed",
        other => {
            return err_json(
                StatusCode::BAD_REQUEST,
                "bad_status",
                format!("未知状态 {other}"),
            )
        }
    };
    match state
        .db
        .document_location_set_import_state(
            &body.source_id,
            &body.path,
            body.sha256.as_deref(),
            status,
            body.error.as_deref(),
        )
        .await
    {
        Ok(()) => (StatusCode::OK, Json(json!({ "recorded": true }))).into_response(),
        Err(e) => err_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            "db_failed",
            format!("记录导入状态失败：{e}"),
        ),
    }
}

#[derive(Deserialize)]
struct LocationsQuery {
    source_id: String,
    #[serde(default)]
    limit: Option<u32>,
}

/// GET /documents/locations?source_id=&limit= — per-source location states.
async fn list_locations(
    State(state): State<std::sync::Arc<AppState>>,
    Query(q): Query<LocationsQuery>,
) -> Response {
    match state
        .db
        .document_location_list(&q.source_id, q.limit.unwrap_or(200))
        .await
    {
        Ok(rows) => (StatusCode::OK, Json(json!({ "data": rows }))).into_response(),
        Err(e) => err_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            "list_failed",
            format!("读取文件位置失败：{e}"),
        ),
    }
}
