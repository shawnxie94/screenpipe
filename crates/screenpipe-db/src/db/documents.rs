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
        let existing: Option<(String,)> =
            sqlx::query_as("SELECT sha256 FROM source_documents WHERE sha256 = ?1")
                .bind(sha256)
                .fetch_optional(&self.pool)
                .await?;
        if existing.is_some() {
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
        .execute(&self.pool)
        .await?;
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
            sqlx::query(
                "UPDATE source_documents SET state = 'failed', \
                 error_message = ?2, chunk_count = 0, truncated = 0, updated_at = ?3 \
                 WHERE sha256 = ?1",
            )
            .bind(sha256)
            .bind("未解析出可搜索的文字（文件为空或仅含图像/扫描内容）")
            .bind(now_ts())
            .execute(&self.pool)
            .await?;
            return Ok("failed");
        }

        let chunks = chunk_text(text.trim());
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM source_document_chunks WHERE sha256 = ?1")
            .bind(sha256)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM source_documents_fts WHERE sha256 = ?1")
            .bind(sha256)
            .execute(&mut *tx)
            .await?;
        for (ordinal, body) in chunks.iter().enumerate() {
            sqlx::query(
                "INSERT INTO source_document_chunks (sha256, ordinal, body) VALUES (?1, ?2, ?3)",
            )
            .bind(sha256)
            .bind(ordinal as i64)
            .bind(body)
            .execute(&mut *tx)
            .await?;
            let fts_body = crate::text_normalizer::chinese_project(body);
            sqlx::query(
                "INSERT INTO source_documents_fts (body, sha256, ordinal) VALUES (?1, ?2, ?3)",
            )
            .bind(&fts_body)
            .bind(sha256)
            .bind(ordinal as i64)
            .execute(&mut *tx)
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
        .execute(&mut *tx)
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
                Digest::update(&mut hasher, format!("path:{:?}:{}", original_path, file_name));
                format!("path-{}", hex::encode(Digest::finalize(hasher)))
            }
        };
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
        .execute(&self.pool)
        .await?;
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
    pub async fn document_list(
        &self,
        limit: u32,
    ) -> Result<Vec<LocalDocumentRow>, sqlx::Error> {
        sqlx::query_as::<_, LocalDocumentRow>(
            "SELECT sha256, file_name, ext, size_bytes, original_path, managed_path, \
             state, truncated, chunk_count, error_message, imported_at, updated_at \
             FROM source_documents ORDER BY imported_at DESC LIMIT ?1",
        )
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
        assert_eq!(chunks, chunk_text(&long), "same text must split identically");
    }
}
