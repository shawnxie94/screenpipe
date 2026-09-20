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
        .route("/reveal", post(reveal_document))
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
