// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

//! Local document manual import persistence (`source_documents` layer).
//!
//! One row per unique file content, keyed by SHA-256: a re-import of the same
//! bytes is a visible no-op, not a second search result. Extracted text is
//! split into stable chunks and indexed in `source_documents_fts`, mirroring
//! the connector FTS shape. Raw binaries never enter SQLite — only metadata,
//! derived text, and the managed copy path.
//!
//! These methods live on the public `DatabaseManager`, but the row types stay
//! module-local: callers consume the returned rows by field access, so the
//! crate root needs no extra re-exports (kept out deliberately — the task's
//! allowed paths exclude `lib.rs`).

use chrono::Utc;
use sha2::Digest;

use super::DatabaseManager;

#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct LocalDocumentRow {
    pub sha256: String,
    pub file_name: String,
    pub ext: String,
    pub size_bytes: i64,
    pub original_path: Option<String>,
    pub managed_path: Option<String>,
    pub state: String,
    pub truncated: bool,
    pub chunk_count: i64,
    pub error_message: Option<String>,
    pub imported_at: String,
    pub updated_at: String,
}

/// A user-chosen watch directory for auto-ingest. Explicit consent only.
#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct DocumentSourceRow {
    pub id: String,
    pub path: String,
    pub enabled: bool,
    pub include_exts: String,
    pub exclude_globs: String,
    pub created_at: String,
    pub updated_at: String,
}

/// One watched file location under a source directory. Identity is the path,
/// not the content: the same bytes at two paths keep two location rows that
/// point at the same `source_documents` sha256.
#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct DocumentLocationRow {
    pub source_id: String,
    pub path: String,
    pub sha256: String,
    pub file_name: String,
    pub ext: String,
    pub size_bytes: i64,
    pub modified_ms: i64,
    pub state: String,
    pub error_message: Option<String>,
    pub imported_at: Option<String>,
    pub last_seen_at: String,
    pub updated_at: String,
}

/// Result of reconciling an on-disk scan against stored locations. Files whose
/// size changed (or that were never seen) need a (re-)import; everything else
/// is either untouched or was just marked missing.
#[derive(Debug, Default, PartialEq)]
pub struct DocumentScanDiff {
    /// Paths that are new or changed and must be imported.
    pub to_import: Vec<String>,
    /// Paths that existed in the DB but not on disk (now marked missing).
    pub missing: Vec<String>,
}

/// One scanned file as reported by the native watcher/reconciler.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ScannedFile {
    pub path: String,
    pub file_name: String,
    pub ext: String,
    pub size_bytes: i64,
    /// Persisted mtime fingerprint (unix epoch millis). Size alone misses
    /// same-length edits; this is the stable per-file change signal.
    #[serde(default)]
    pub modified_ms: i64,
}

/// One search hit: the document plus the matching chunk with an FTS snippet.
#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct LocalDocumentHit {
    pub sha256: String,
    pub file_name: String,
    pub ext: String,
    pub original_path: Option<String>,
    pub managed_path: Option<String>,
    pub imported_at: String,
    pub ordinal: i64,
    pub snippet: String,
}

/// Target chunk size in characters. Small enough for readable snippets,
/// large enough that a typical page lands in one chunk.
const CHUNK_CHARS: usize = 1200;

fn now_ts() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

/// Split extracted text into stable chunks: prefer paragraph breaks, then
/// line breaks, then hard-split when a single unit exceeds the target.
fn chunk_text(text: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for unit in text.split("\n\n") {
        for piece in split_long(unit) {
            if current.len() + piece.len() + 2 > CHUNK_CHARS && !current.is_empty() {
                chunks.push(current.trim_end().to_string());
                current.clear();
            }
            if !current.is_empty() {
                current.push_str("\n\n");
            }
            current.push_str(&piece);
        }
    }
    if !current.trim_end().is_empty() {
        chunks.push(current.trim_end().to_string());
    }
    chunks
}

/// Split a paragraph longer than the chunk size on line breaks, then on
/// hard character boundaries.
fn split_long(unit: &str) -> Vec<String> {
    if unit.len() <= CHUNK_CHARS {
        return vec![unit.to_string()];
    }
    let mut out = Vec::new();
    let mut current = String::new();
    for line in unit.split('\n') {
        if current.len() + line.len() + 1 > CHUNK_CHARS && !current.is_empty() {
            out.push(current.trim_end().to_string());
            current.clear();
        }
        // A single line longer than the target is hard-split so no chunk
        // can grow unbounded (minified JSON, base64 blobs).
        let mut rest = line;
        while rest.len() > CHUNK_CHARS {
            let mut cut = CHUNK_CHARS;
            while !rest.is_char_boundary(cut) {
                cut -= 1;
            }
            out.push(rest[..cut].to_string());
            rest = &rest[cut..];
        }
        if !rest.is_empty() {
            if !current.is_empty() {
                current.push('\n');
            }
            current.push_str(rest);
        }
    }
    if !current.trim_end().is_empty() {
        out.push(current.trim_end().to_string());
    }
    out
}

/// Quote each whitespace-separated token as an FTS5 phrase so user input
/// containing FTS syntax (`AND`, `*`, parentheses) stays literal.
fn fts_match_expression(query: &str) -> String {
    query
        .split_whitespace()
        .map(|token| format!("\"{}\"", token.replace('"', " ")))
        .collect::<Vec<_>>()
        .join(" ")
}

impl DatabaseManager {
    /// Record that a document's bytes are stored. Content-addressed: the
    /// second import of identical bytes returns `false` (duplicate) and
    /// leaves all state (including prior failure reasons) untouched.
    pub async fn document_import_stored(
        &self,
        sha256: &str,
        file_name: &str,
        ext: &str,
        size_bytes: i64,
        original_path: Option<&str>,
        managed_path: Option<&str>,
    ) -> Result<bool, sqlx::Error> {
        let mut tx = self.begin_immediate_with_retry().await?;
        let existing: Option<(String,)> =
            sqlx::query_as("SELECT sha256 FROM source_documents WHERE sha256 = ?1")
                .bind(sha256)
                .fetch_optional(&mut **tx.conn())
                .await?;
        if existing.is_some() {
            tx.commit().await?;
            return Ok(false);
        }
        sqlx::query(
            "INSERT INTO source_documents (sha256, file_name, ext, size_bytes, \
             original_path, managed_path, state) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'stored')",
        )
        .bind(sha256)
        .bind(file_name)
        .bind(ext)
        .bind(size_bytes)
        .bind(original_path)
        .bind(managed_path)
        .execute(&mut **tx.conn())
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    /// Store extracted text: chunk it, rebuild the FTS entries, and mark the
    /// document `ready`. Empty/unparseable text marks the document `failed`
    /// with a visible reason instead — a stored file without text is a real
    /// import failure the user must see. Returns the resulting state.
    pub async fn document_mark_ready(
        &self,
        sha256: &str,
        text: &str,
        truncated: bool,
    ) -> Result<&'static str, sqlx::Error> {
        let exists: Option<(String,)> =
            sqlx::query_as("SELECT sha256 FROM source_documents WHERE sha256 = ?1")
                .bind(sha256)
                .fetch_optional(&self.pool)
                .await?;
        if exists.is_none() {
            return Err(sqlx::Error::RowNotFound);
        }

        if text.trim().is_empty() {
            let mut tx = self.begin_immediate_with_retry().await?;
            sqlx::query(
                "UPDATE source_documents SET state = 'failed', \
                 error_message = ?2, chunk_count = 0, truncated = 0, updated_at = ?3 \
                 WHERE sha256 = ?1",
            )
            .bind(sha256)
            .bind("未解析出可搜索的文字（文件为空或仅含图像/扫描内容）")
            .bind(now_ts())
            .execute(&mut **tx.conn())
            .await?;
            tx.commit().await?;
            return Ok("failed");
        }

        let chunks = chunk_text(text.trim());
        let mut tx = self.begin_immediate_with_retry().await?;
        sqlx::query("DELETE FROM source_document_chunks WHERE sha256 = ?1")
            .bind(sha256)
            .execute(&mut **tx.conn())
            .await?;
        sqlx::query("DELETE FROM source_documents_fts WHERE sha256 = ?1")
            .bind(sha256)
            .execute(&mut **tx.conn())
            .await?;
        for (ordinal, body) in chunks.iter().enumerate() {
            sqlx::query(
                "INSERT INTO source_document_chunks (sha256, ordinal, body) VALUES (?1, ?2, ?3)",
            )
            .bind(sha256)
            .bind(ordinal as i64)
            .bind(body)
            .execute(&mut **tx.conn())
            .await?;
            let fts_body = crate::text_normalizer::chinese_project(body);
            sqlx::query(
                "INSERT INTO source_documents_fts (body, sha256, ordinal) VALUES (?1, ?2, ?3)",
            )
            .bind(&fts_body)
            .bind(sha256)
            .bind(ordinal as i64)
            .execute(&mut **tx.conn())
            .await?;
        }
        sqlx::query(
            "UPDATE source_documents SET state = 'ready', truncated = ?2, \
             chunk_count = ?3, error_message = NULL, updated_at = ?4 WHERE sha256 = ?1",
        )
        .bind(sha256)
        .bind(truncated)
        .bind(chunks.len() as i64)
        .bind(now_ts())
        .execute(&mut **tx.conn())
        .await?;
        tx.commit().await?;
        Ok("ready")
    }

    /// Record a failed import (unsupported ext, oversized, unreadable,
    /// parser error). Creates a `failed` row even when no bytes were stored,
    /// so the failure is visible and the user can see why nothing appeared.
    pub async fn document_mark_failed(
        &self,
        sha256: Option<&str>,
        file_name: &str,
        ext: &str,
        size_bytes: i64,
        original_path: Option<&str>,
        reason: &str,
    ) -> Result<(), sqlx::Error> {
        let sha = match sha256 {
            Some(s) => s.to_string(),
            // No content hash available (e.g. read failure before hashing):
            // derive a stable row key from the path+name so the failure is
            // still recorded exactly once per file.
            None => {
                let mut hasher = sha2::Sha256::new();
                Digest::update(
                    &mut hasher,
                    format!("path:{:?}:{}", original_path, file_name),
                );
                format!("path-{}", hex::encode(Digest::finalize(hasher)))
            }
        };
        let mut tx = self.begin_immediate_with_retry().await?;
        sqlx::query(
            "INSERT INTO source_documents (sha256, file_name, ext, size_bytes, \
             original_path, state, error_message) \
             VALUES (?1, ?2, ?3, ?4, ?5, 'failed', ?6) \
             ON CONFLICT(sha256) DO UPDATE SET state = 'failed', error_message = ?6, \
             updated_at = ?7",
        )
        .bind(&sha)
        .bind(file_name)
        .bind(ext)
        .bind(size_bytes)
        .bind(original_path)
        .bind(reason)
        .bind(now_ts())
        .execute(&mut **tx.conn())
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Unified-search query over imported documents. Empty query browses the
    /// most recent `ready` documents; non-empty queries FTS with snippets.
    pub async fn document_search(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<LocalDocumentHit>, sqlx::Error> {
        let limit = limit.clamp(1, 200) as i64;
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return sqlx::query_as::<_, LocalDocumentHit>(
                "SELECT d.sha256, d.file_name, d.ext, d.original_path, d.managed_path, \
                 d.imported_at, 0 AS ordinal, '' AS snippet \
                 FROM source_documents d WHERE d.state = 'ready' \
                 ORDER BY d.imported_at DESC LIMIT ?1",
            )
            .bind(limit)
            .fetch_all(&self.pool)
            .await;
        }
        let match_expr = fts_match_expression(&crate::text_normalizer::chinese_project(trimmed));
        sqlx::query_as::<_, LocalDocumentHit>(
            "SELECT d.sha256, d.file_name, d.ext, d.original_path, d.managed_path, \
             d.imported_at, f.ordinal, \
             snippet(source_documents_fts, 0, '[', ']', ' … ', 16) AS snippet \
             FROM source_documents_fts f \
             JOIN source_documents d ON d.sha256 = f.sha256 \
             WHERE d.state = 'ready' AND source_documents_fts MATCH ?1 \
             ORDER BY bm25(source_documents_fts) LIMIT ?2",
        )
        .bind(&match_expr)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
    }

    /// Search ready documents in an optional imported-at time range.
    pub async fn document_search_in_range(
        &self,
        query: &str,
        limit: u32,
        start_time: Option<chrono::DateTime<chrono::Utc>>,
        end_time: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<Vec<LocalDocumentHit>, sqlx::Error> {
        let limit = limit.clamp(1, 200) as i64;
        let trimmed = query.trim();
        let time_pred = "AND (?1 IS NULL OR strftime('%s', d.imported_at) >= strftime('%s', ?1)) AND (?2 IS NULL OR strftime('%s', d.imported_at) <= strftime('%s', ?2))";
        if trimmed.is_empty() {
            let sql = format!("SELECT d.sha256, d.file_name, d.ext, d.original_path, d.managed_path, d.imported_at, 0 AS ordinal, '' AS snippet FROM source_documents d WHERE d.state = 'ready' {time_pred} ORDER BY strftime('%s', d.imported_at) DESC LIMIT ?3");
            return sqlx::query_as::<_, LocalDocumentHit>(sqlx::AssertSqlSafe(sql.as_str()))
                .bind(start_time)
                .bind(end_time)
                .bind(limit)
                .fetch_all(&self.pool)
                .await;
        }
        let match_expr = fts_match_expression(&crate::text_normalizer::chinese_project(trimmed));
        let sql = format!("SELECT d.sha256, d.file_name, d.ext, d.original_path, d.managed_path, d.imported_at, f.ordinal, snippet(source_documents_fts, 0, '[', ']', ' … ', 16) AS snippet FROM source_documents_fts f JOIN source_documents d ON d.sha256 = f.sha256 WHERE d.state = 'ready' AND source_documents_fts MATCH ?3 {time_pred} ORDER BY bm25(source_documents_fts) LIMIT ?4");
        sqlx::query_as::<_, LocalDocumentHit>(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(start_time)
            .bind(end_time)
            .bind(match_expr)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
    }

    /// Count top-level ready documents matching the same query and imported-at range.
    /// FTS is chunked, but the unified search surface exposes one result per sha256.
    pub async fn document_search_count_in_range(
        &self,
        query: &str,
        start_time: Option<chrono::DateTime<chrono::Utc>>,
        end_time: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<i64, sqlx::Error> {
        let trimmed = query.trim();
        let time_pred = "AND (?1 IS NULL OR strftime('%s', d.imported_at) >= strftime('%s', ?1)) AND (?2 IS NULL OR strftime('%s', d.imported_at) <= strftime('%s', ?2))";
        if trimmed.is_empty() {
            let sql = format!("SELECT COUNT(*) FROM source_documents d WHERE d.state = 'ready' {time_pred}");
            return sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql.as_str()))
                .bind(start_time)
                .bind(end_time)
                .fetch_one(&self.pool)
                .await;
        }
        let match_expr = fts_match_expression(&crate::text_normalizer::chinese_project(trimmed));
        let sql = format!("SELECT COUNT(DISTINCT d.sha256) FROM source_documents_fts f JOIN source_documents d ON d.sha256 = f.sha256 WHERE d.state = 'ready' AND source_documents_fts MATCH ?3 {time_pred}");
        sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(start_time)
            .bind(end_time)
            .bind(match_expr)
            .fetch_one(&self.pool)
            .await
    }

    pub async fn document_get(
        &self,
        sha256: &str,
    ) -> Result<Option<LocalDocumentRow>, sqlx::Error> {
        sqlx::query_as::<_, LocalDocumentRow>(
            "SELECT sha256, file_name, ext, size_bytes, original_path, managed_path, \
             state, truncated, chunk_count, error_message, imported_at, updated_at \
             FROM source_documents WHERE sha256 = ?1",
        )
        .bind(sha256)
        .fetch_optional(&self.pool)
        .await
    }

    /// Import status listing (all states) for observability surfaces.
    pub async fn document_list(&self, limit: u32) -> Result<Vec<LocalDocumentRow>, sqlx::Error> {
        sqlx::query_as::<_, LocalDocumentRow>(
            "SELECT sha256, file_name, ext, size_bytes, original_path, managed_path, \
             state, truncated, chunk_count, error_message, imported_at, updated_at \
             FROM source_documents ORDER BY imported_at DESC LIMIT ?1",
        )
        .bind(limit.clamp(1, 500) as i64)
        .fetch_all(&self.pool)
        .await
    }

    /// Register a watch directory. Returns `false` when the path is already
    /// watched (the caller then updates the existing source instead).
    pub async fn document_source_add(
        &self,
        id: &str,
        path: &str,
        include_exts: &str,
        exclude_globs: &str,
    ) -> Result<bool, sqlx::Error> {
        let mut tx = self.begin_immediate_with_retry().await?;
        let exists: Option<(String,)> =
            sqlx::query_as("SELECT id FROM document_sources WHERE path = ?1")
                .bind(path)
                .fetch_optional(&mut **tx.conn())
                .await?;
        if exists.is_some() {
            tx.commit().await?;
            return Ok(false);
        }
        sqlx::query(
            "INSERT INTO document_sources (id, path, include_exts, exclude_globs) \
             VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(id)
        .bind(path)
        .bind(include_exts)
        .bind(exclude_globs)
        .execute(&mut **tx.conn())
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn document_source_list(&self) -> Result<Vec<DocumentSourceRow>, sqlx::Error> {
        sqlx::query_as::<_, DocumentSourceRow>(
            "SELECT id, path, enabled, include_exts, exclude_globs, created_at, updated_at \
             FROM document_sources ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await
    }

    /// Update enable flag and/or filters. `None` leaves a field unchanged.
    pub async fn document_source_update(
        &self,
        id: &str,
        enabled: Option<bool>,
        include_exts: Option<&str>,
        exclude_globs: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.begin_immediate_with_retry().await?;
        sqlx::query(
            "UPDATE document_sources SET \
             enabled = COALESCE(?2, enabled), \
             include_exts = COALESCE(?3, include_exts), \
             exclude_globs = COALESCE(?4, exclude_globs), \
             updated_at = ?5 \
             WHERE id = ?1",
        )
        .bind(id)
        .bind(enabled)
        .bind(include_exts)
        .bind(exclude_globs)
        .bind(now_ts())
        .execute(&mut **tx.conn())
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Remove a source and its location rows. The managed document copies and
    /// their searchable text stay — auto-ingest only ever adds.
    pub async fn document_source_remove(&self, id: &str) -> Result<(), sqlx::Error> {
        let mut tx = self.begin_immediate_with_retry().await?;
        sqlx::query("DELETE FROM document_locations WHERE source_id = ?1")
            .bind(id)
            .execute(&mut **tx.conn())
            .await?;
        sqlx::query("DELETE FROM document_sources WHERE id = ?1")
            .bind(id)
            .execute(&mut **tx.conn())
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Reconcile one source's on-disk scan against stored locations:
    /// mark vanished files missing (delete keeps the managed copy), refresh
    /// `last_seen_at` for files still there, and return the paths that need
    /// importing (never seen, or size changed since the last import).
    pub async fn document_source_scan_diff(
        &self,
        source_id: &str,
        files: &[ScannedFile],
    ) -> Result<DocumentScanDiff, sqlx::Error> {
        let now = now_ts();
        let mut diff = DocumentScanDiff::default();
        let mut tx = self.begin_immediate_with_retry().await?;

        let stored: Vec<(String, String, i64, i64, String)> = sqlx::query_as(
            "SELECT path, sha256, size_bytes, modified_ms, state FROM document_locations \
             WHERE source_id = ?1 AND state != 'missing'",
        )
        .bind(source_id)
        .fetch_all(&mut **tx.conn())
        .await?;
        let on_disk: std::collections::HashSet<&str> =
            files.iter().map(|f| f.path.as_str()).collect();
        for (path, _sha, _size, _modified, _state) in &stored {
            if !on_disk.contains(path.as_str()) {
                sqlx::query(
                    "UPDATE document_locations SET state = 'missing', updated_at = ?3 \
                     WHERE source_id = ?1 AND path = ?2",
                )
                .bind(source_id)
                .bind(path)
                .bind(&now)
                .execute(&mut **tx.conn())
                .await?;
                diff.missing.push(path.clone());
            }
        }

        for file in files {
            let existing: Option<(String, String, i64, i64)> = sqlx::query_as(
                "SELECT sha256, state, size_bytes, modified_ms FROM document_locations \
                 WHERE source_id = ?1 AND path = ?2",
            )
            .bind(source_id)
            .bind(&file.path)
            .fetch_optional(&mut **tx.conn())
            .await?;
            match existing {
                None => {
                    sqlx::query(
                        "INSERT INTO document_locations \
                         (source_id, path, file_name, ext, size_bytes, modified_ms, state, last_seen_at) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending', ?7)",
                    )
                    .bind(source_id)
                    .bind(&file.path)
                    .bind(&file.file_name)
                    .bind(&file.ext)
                    .bind(file.size_bytes)
                    .bind(file.modified_ms)
                    .bind(&now)
                    .execute(&mut **tx.conn())
                    .await?;
                    diff.to_import.push(file.path.clone());
                }
                Some((_sha256, state, size_bytes, modified_ms)) => {
                    sqlx::query(
                        "UPDATE document_locations SET last_seen_at = ?3, updated_at = ?3, \
                         file_name = ?4, ext = ?5, size_bytes = ?6, modified_ms = ?7 \
                         WHERE source_id = ?1 AND path = ?2",
                    )
                    .bind(source_id)
                    .bind(&file.path)
                    .bind(&now)
                    .bind(&file.file_name)
                    .bind(&file.ext)
                    .bind(file.size_bytes)
                    .bind(file.modified_ms)
                    .execute(&mut **tx.conn())
                    .await?;
                    // Re-import when the last import never finished or the
                    // file changed on disk. Size alone misses same-length
                    // edits, so the persisted mtime fingerprint decides.
                    let changed = size_bytes != file.size_bytes || modified_ms != file.modified_ms;
                    if state != "imported" || changed {
                        diff.to_import.push(file.path.clone());
                    }
                }
            }
        }

        tx.commit().await?;
        Ok(diff)
    }

    /// Record a location's import outcome. A missing content hash (import
    /// failed before hashing) leaves the sha empty so a later attempt can
    /// still fill it in; the state records what happened for observability.
    pub async fn document_location_set_import_state(
        &self,
        source_id: &str,
        path: &str,
        sha256: Option<&str>,
        state: &str,
        error_message: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let now = now_ts();
        let mut tx = self.begin_immediate_with_retry().await?;
        match sha256 {
            Some(sha) => {
                sqlx::query(
                    "UPDATE document_locations SET sha256 = ?3, state = ?4, \
                     error_message = ?5, imported_at = CASE WHEN ?4 = 'imported' THEN ?6 \
                     ELSE imported_at END, updated_at = ?6 \
                     WHERE source_id = ?1 AND path = ?2",
                )
                .bind(source_id)
                .bind(path)
                .bind(sha)
                .bind(state)
                .bind(error_message)
                .bind(&now)
                .execute(&mut **tx.conn())
                .await?;
            }
            None => {
                sqlx::query(
                    "UPDATE document_locations SET state = ?3, error_message = ?4, \
                     updated_at = ?5 WHERE source_id = ?1 AND path = ?2",
                )
                .bind(source_id)
                .bind(path)
                .bind(state)
                .bind(error_message)
                .bind(&now)
                .execute(&mut **tx.conn())
                .await?;
            }
        }
        tx.commit().await?;
        Ok(())
    }

    /// Location rows for one source (status surface for the settings UI).
    pub async fn document_location_list(
        &self,
        source_id: &str,
        limit: u32,
    ) -> Result<Vec<DocumentLocationRow>, sqlx::Error> {
        sqlx::query_as::<_, DocumentLocationRow>(
            "SELECT source_id, path, sha256, file_name, ext, size_bytes, modified_ms, state, \
             error_message, imported_at, last_seen_at, updated_at \
             FROM document_locations WHERE source_id = ?1 \
             ORDER BY updated_at DESC LIMIT ?2",
        )
        .bind(source_id)
        .bind(limit.clamp(1, 500) as i64)
        .fetch_all(&self.pool)
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunking_is_stable_and_bounded() {
        let long = "word ".repeat(2000);
        let chunks = chunk_text(&long);
        assert!(chunks.len() > 1, "long text must split");
        for chunk in &chunks {
            assert!(chunk.len() <= 1300, "chunks must stay bounded");
        }
        assert_eq!(
            chunks,
            chunk_text(&long),
            "same text must split identically"
        );
    }
}
