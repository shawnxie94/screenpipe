// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com
//
//! Unified hybrid retrieval storage layer (SPEC
//! sqlite://artifact/spec/unified-hybrid-retrieval).
//!
//! Owns three concerns:
//! - the dense-leg store: content-hash idempotent queue + f32-blob embedding
//!   table queried through sqlite-vec scalar functions;
//! - source enqueueing: deterministic transcript chunking and document chunk
//!   passthrough, incremental via watermarks in `retrieval_index_meta`;
//! - the CJK projection backfill: companion FTS tables whose bodies are
//!   `text_normalizer::chinese_project` output, resumable per batch.

use super::DatabaseManager;
use crate::text_normalizer::chinese_project;
use chrono::{DateTime, NaiveDateTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::Row;

/// Max characters in one transcript chunk (SPEC S2 chunking matrix).
const TRANSCRIPT_CHUNK_CHARS: usize = 800;
/// Max wall-clock span of one transcript chunk.
const TRANSCRIPT_CHUNK_SECONDS: f64 = 60.0;
/// Rows join the same chunk when their gap is at most this many seconds.
const TRANSCRIPT_CHUNK_GAP_SECONDS: f64 = 2.0;
/// Give up a queue row to transient embedder failures after this many tries.
const MAX_QUEUE_ATTEMPTS: i64 = 5;

/// One chunk candidate handed to the embed queue. `source_pk` is text so it
/// carries both integer row ids and content hashes (document sha256).
#[derive(Debug, Clone, PartialEq)]
pub struct RetrievalChunkInput {
    pub source_type: &'static str,
    pub source_pk: String,
    pub chunk_seq: i64,
    pub text: String,
    pub ts: Option<String>,
    pub app: String,
    pub window_name: String,
    pub url: Option<String>,
    pub speaker_id: Option<i64>,
    pub frame_id: Option<i64>,
}

impl RetrievalChunkInput {
    pub fn chunk_uid(&self) -> String {
        chunk_uid_key(self.source_type, &self.source_pk, self.chunk_seq)
    }

    pub fn content_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.text.as_bytes());
        hex::encode(hasher.finalize())
    }
}

pub fn chunk_uid_key(source_type: &str, source_pk: &str, chunk_seq: i64) -> String {
    format!("{source_type}:{source_pk}:{chunk_seq}")
}

/// Split a stored chunk uid back into its contract parts.
pub fn chunk_uid_parts(chunk_uid: &str) -> Option<(&str, &str, i64)> {
    let mut parts = chunk_uid.splitn(3, ':');
    let source_type = parts.next()?;
    let source_pk = parts.next()?;
    let chunk_seq = parts.next()?.parse().ok()?;
    Some((source_type, source_pk, chunk_seq))
}

/// Dense candidate key matching the sparse candidate key space.
pub fn dense_key(source_type: &str, source_pk: &str) -> String {
    format!("{source_type}:{source_pk}")
}

/// A claimed queue row handed to the embedder client. Carries the full
/// candidate metadata so the stored vector keeps its projection contract.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct QueuedChunk {
    pub chunk_uid: String,
    pub source_type: String,
    pub source_pk: String,
    pub chunk_seq: i64,
    pub content_hash: String,
    pub text: String,
    pub ts: Option<String>,
    pub app: String,
    pub window_name: String,
    pub url: Option<String>,
    pub speaker_id: Option<i64>,
    pub frame_id: Option<i64>,
}

impl QueuedChunk {
    pub fn meta(&self) -> QueuedMeta {
        QueuedMeta {
            ts: self.ts.clone(),
            app: self.app.clone(),
            window_name: self.window_name.clone(),
            url: self.url.clone(),
            speaker_id: self.speaker_id,
            frame_id: self.frame_id,
        }
    }
}

/// Denormalized candidate metadata captured alongside the vector.
#[derive(Debug, Clone, Default)]
pub struct QueuedMeta {
    pub ts: Option<String>,
    pub app: String,
    pub window_name: String,
    pub url: Option<String>,
    pub speaker_id: Option<i64>,
    pub frame_id: Option<i64>,
}

impl QueuedMeta {
    pub fn from_input(chunk: &RetrievalChunkInput) -> Self {
        Self {
            ts: chunk.ts.clone(),
            app: chunk.app.clone(),
            window_name: chunk.window_name.clone(),
            url: chunk.url.clone(),
            speaker_id: chunk.speaker_id,
            frame_id: chunk.frame_id,
        }
    }
}

/// One dense-leg hit in the candidate projection contract.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DenseHit {
    pub chunk_uid: String,
    pub source_type: String,
    pub source_pk: String,
    pub chunk_seq: i64,
    pub ts: Option<String>,
    pub app: String,
    pub window_name: String,
    pub url: Option<String>,
    pub speaker_id: Option<i64>,
    pub frame_id: Option<i64>,
    /// Cosine distance from the query vector (0 = identical).
    pub distance: f32,
}

const DENSE_HIT_COLUMNS: &str = "chunk_uid, source_type, source_pk, chunk_seq, ts, app, \
     window_name, url, speaker_id, frame_id";

fn dense_hit_from_row(row: sqlx::sqlite::SqliteRow) -> DenseHit {
    DenseHit {
        chunk_uid: row.get("chunk_uid"),
        source_type: row.get("source_type"),
        source_pk: row.get("source_pk"),
        chunk_seq: row.get("chunk_seq"),
        ts: row.get("ts"),
        app: row.get("app"),
        window_name: row.get("window_name"),
        url: row.get("url"),
        speaker_id: row.get("speaker_id"),
        frame_id: row.get("frame_id"),
        distance: row.get::<f64, _>("distance") as f32,
    }
}

pub(crate) fn f32_vec_to_blob(vector: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vector.len() * 4);
    for v in vector {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes
}

fn parse_ts_seconds(ts: &str) -> Option<f64> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(ts) {
        return Some(dt.with_timezone(&Utc).timestamp_millis() as f64 / 1000.0);
    }
    NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S")
        .ok()
        .map(|dt| dt.and_utc().timestamp_millis() as f64 / 1000.0)
}

/// Deterministic transcript chunking: consecutive rows join while the speaker
/// stays, the gap stays within `TRANSCRIPT_CHUNK_GAP_SECONDS`, and the running
/// chunk stays within the char and duration caps. `rows` must be ordered by id
/// as `(id, timestamp, transcription, speaker_id, device, is_input_device)`.
pub fn chunk_transcript_rows(
    rows: &[(i64, String, String, Option<i64>, String, bool)],
) -> Vec<RetrievalChunkInput> {
    #[derive(Default)]
    struct Open {
        row_ids: Vec<i64>,
        texts: Vec<String>,
        first_ts: Option<String>,
        prev_ts: Option<String>,
        speaker_id: Option<i64>,
    }
    let mut open = Open::default();
    let mut open_chars = 0usize;
    let mut chunks = Vec::new();

    fn flush(open: &mut Open, open_chars: &mut usize, chunks: &mut Vec<RetrievalChunkInput>) {
        let text = open
            .texts
            .iter()
            .map(|t| t.trim())
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if let (Some(first_id), false) = (open.row_ids.first().copied(), text.is_empty()) {
            chunks.push(RetrievalChunkInput {
                source_type: "transcript",
                source_pk: first_id.to_string(),
                chunk_seq: 0,
                text,
                ts: open.first_ts.clone(),
                app: "audio".into(),
                window_name: "input".into(),
                url: None,
                speaker_id: open.speaker_id,
                frame_id: None,
            });
        }
        *open = Open::default();
        *open_chars = 0;
    }

    for (id, ts, transcription, speaker_id, _device, _is_input) in rows {
        let split = match open.texts.last() {
            None => false,
            Some(_) => {
                let same_speaker = open.speaker_id == *speaker_id;
                let gap_ok = match (
                    open.prev_ts.as_deref().and_then(parse_ts_seconds),
                    parse_ts_seconds(ts),
                ) {
                    (Some(a), Some(b)) => (b - a) <= TRANSCRIPT_CHUNK_GAP_SECONDS,
                    _ => true,
                };
                let span_ok = match (
                    open.first_ts.as_deref().and_then(parse_ts_seconds),
                    parse_ts_seconds(ts),
                ) {
                    (Some(a), Some(b)) => (b - a) <= TRANSCRIPT_CHUNK_SECONDS,
                    _ => true,
                };
                !same_speaker || !gap_ok || !span_ok
            }
        };
        if open_chars + transcription.len() > TRANSCRIPT_CHUNK_CHARS && !open.row_ids.is_empty() {
            flush(&mut open, &mut open_chars, &mut chunks);
        }
        if split {
            flush(&mut open, &mut open_chars, &mut chunks);
        }
        if open.row_ids.is_empty() {
            open.first_ts = Some(ts.clone());
            open.speaker_id = *speaker_id;
        }
        open.row_ids.push(*id);
        open.texts.push(transcription.clone());
        open.prev_ts = Some(ts.clone());
        open_chars += transcription.len();
    }
    flush(&mut open, &mut open_chars, &mut chunks);
    chunks
}

impl DatabaseManager {
    pub async fn retrieval_meta_get(&self, key: &str) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar("SELECT value FROM retrieval_index_meta WHERE key = ?1")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn retrieval_meta_set(&self, key: &str, value: &str) -> Result<(), sqlx::Error> {
        let mut tx = self.begin_immediate_with_retry().await?;
        sqlx::query(
            "INSERT INTO retrieval_index_meta(key, value) VALUES (?1, ?2) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
        )
        .bind(key)
        .bind(value)
        .execute(&mut **tx.conn())
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Enqueue chunks. Idempotent per chunk_uid: identical content_hash is a
    /// no-op, changed content requeues and drops the stale embedding so the
    /// dense leg never serves an outdated vector.
    pub async fn enqueue_retrieval_chunks(
        &self,
        chunks: &[RetrievalChunkInput],
    ) -> Result<u64, sqlx::Error> {
        let mut tx = self.begin_immediate_with_retry().await?;
        let conn = tx.conn();
        let mut enqueued: u64 = 0;
        for chunk in chunks {
            let chunk_uid = chunk.chunk_uid();
            let content_hash = chunk.content_hash();
            let existing: Option<(String,)> =
                sqlx::query_as("SELECT content_hash FROM retrieval_embed_queue WHERE chunk_uid = ?1")
                    .bind(&chunk_uid)
                    .fetch_optional(&mut **conn)
                    .await?;
            if existing.as_ref().is_some_and(|(hash,)| hash == &content_hash) {
                continue;
            }
            if existing.is_some() {
                sqlx::query("DELETE FROM retrieval_embeddings WHERE chunk_uid = ?1")
                    .bind(&chunk_uid)
                    .execute(&mut **conn)
                    .await?;
            }
            let result = sqlx::query(
                "INSERT INTO retrieval_embed_queue(\
                     chunk_uid, source_type, source_pk, chunk_seq, content_hash, text, \
                     ts, app, window_name, url, speaker_id, frame_id)\
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12) \
                 ON CONFLICT(chunk_uid) DO UPDATE SET \
                     content_hash = excluded.content_hash, text = excluded.text, \
                     ts = excluded.ts, app = excluded.app, \
                     window_name = excluded.window_name, url = excluded.url, \
                     speaker_id = excluded.speaker_id, frame_id = excluded.frame_id, \
                     state = 'pending', attempts = 0",
            )
            .bind(&chunk_uid)
            .bind(chunk.source_type)
            .bind(&chunk.source_pk)
            .bind(chunk.chunk_seq)
            .bind(&content_hash)
            .bind(&chunk.text)
            .bind(&chunk.ts)
            .bind(&chunk.app)
            .bind(&chunk.window_name)
            .bind(&chunk.url)
            .bind(chunk.speaker_id)
            .bind(chunk.frame_id)
            .execute(&mut **conn)
            .await?;
            enqueued += result.rows_affected();
        }
        tx.commit().await?;
        Ok(enqueued)
    }

    /// Enqueue every ready document chunk (small corpus, full pass is cheap;
    /// hash idempotency makes repeated calls no-ops).
    pub async fn enqueue_document_chunks(&self) -> Result<u64, sqlx::Error> {
        let rows: Vec<(String, i64, String, Option<String>)> = sqlx::query_as(
            "SELECT c.sha256, c.ordinal, c.body, d.imported_at \
             FROM source_document_chunks c \
             JOIN source_documents d ON d.sha256 = c.sha256 \
             WHERE d.state = 'ready' ORDER BY c.sha256, c.ordinal",
        )
        .fetch_all(&self.pool)
        .await?;
        let chunks: Vec<RetrievalChunkInput> = rows
            .into_iter()
            .map(|(sha256, ordinal, body, imported_at)| RetrievalChunkInput {
                source_type: "document",
                source_pk: sha256,
                chunk_seq: ordinal,
                text: body,
                ts: imported_at,
                app: "document".into(),
                window_name: String::new(),
                url: None,
                speaker_id: None,
                frame_id: None,
            })
            .collect();
        self.enqueue_retrieval_chunks(&chunks).await
    }

    /// Deterministically chunk new transcript rows and enqueue them.
    /// Incremental via the `retrieval.transcripts.last_id` watermark; the
    /// chunk open at the watermark boundary may split differently than it
    /// would have with future rows (accepted, documented in the SPEC).
    pub async fn enqueue_transcript_chunks(&self, batch_limit: i64) -> Result<u64, sqlx::Error> {
        let watermark: i64 = self
            .retrieval_meta_get("retrieval.transcripts.last_id")
            .await?
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let rows: Vec<(i64, String, String, Option<i64>, String, bool)> = sqlx::query_as(
            "SELECT id, timestamp, transcription, speaker_id, device, is_input_device \
             FROM audio_transcriptions WHERE id > ?1 AND text_length > 0 \
             ORDER BY id LIMIT ?2",
        )
        .bind(watermark)
        .bind(batch_limit)
        .fetch_all(&self.pool)
        .await?;
        if rows.is_empty() {
            return Ok(0);
        }
        let chunks = chunk_transcript_rows(&rows);
        let enqueued = self.enqueue_retrieval_chunks(&chunks).await?;
        let max_id = rows.iter().map(|(id, ..)| *id).max().unwrap_or(watermark);
        self.retrieval_meta_set("retrieval.transcripts.last_id", &max_id.to_string())
            .await?;
        Ok(enqueued)
    }

    /// Claim pending rows for embedding (marked `processing` transactionally).
    pub async fn claim_pending_chunks(&self, limit: i64) -> Result<Vec<QueuedChunk>, sqlx::Error> {
        let mut tx = self.begin_immediate_with_retry().await?;
        let claimed = sqlx::query_as::<_, QueuedChunk>(
            "WITH oldest AS (\
                 SELECT chunk_uid FROM retrieval_embed_queue \
                 WHERE state = 'pending' ORDER BY enqueued_at LIMIT ?1\
             ) \
             UPDATE retrieval_embed_queue SET state = 'processing' \
             WHERE chunk_uid IN (SELECT chunk_uid FROM oldest) \
             RETURNING chunk_uid, source_type, source_pk, chunk_seq, content_hash, text, \
                 ts, app, window_name, url, speaker_id, frame_id",
        )
        .bind(limit)
        .fetch_all(&mut **tx.conn())
        .await?;
        tx.commit().await?;
        Ok(claimed)
    }

    pub async fn complete_queued_chunk(&self, chunk_uid: &str) -> Result<(), sqlx::Error> {
        let mut tx = self.begin_immediate_with_retry().await?;
        sqlx::query("DELETE FROM retrieval_embed_queue WHERE chunk_uid = ?1")
            .bind(chunk_uid)
            .execute(&mut **tx.conn())
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Return a failed row to the queue with a bumped attempt counter; rows
    /// past `MAX_QUEUE_ATTEMPTS` keep pending state but re-enter with a fresh
    /// enqueue time so they retry behind newer work instead of hot-looping.
    pub async fn fail_queued_chunk(&self, chunk_uid: &str) -> Result<(), sqlx::Error> {
        let mut tx = self.begin_immediate_with_retry().await?;
        sqlx::query(
            "UPDATE retrieval_embed_queue \
             SET state = 'pending', attempts = attempts + 1, \
                 enqueued_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
             WHERE chunk_uid = ?1 AND attempts < ?2",
        )
        .bind(chunk_uid)
        .bind(MAX_QUEUE_ATTEMPTS)
        .execute(&mut **tx.conn())
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn store_retrieval_embedding(
        &self,
        chunk: &QueuedChunk,
        model_id: &str,
        dim: usize,
        vector: &[f32],
        meta: &QueuedMeta,
    ) -> Result<(), sqlx::Error> {
        let bytes = f32_vec_to_blob(vector);
        let mut tx = self.begin_immediate_with_retry().await?;
        sqlx::query(
            "INSERT INTO retrieval_embeddings(\
                 chunk_uid, source_type, source_pk, chunk_seq, model_id, dim, \
                 content_hash, embedding, ts, app, window_name, url, speaker_id, frame_id)\
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)\
             ON CONFLICT(chunk_uid) DO UPDATE SET \
                 model_id = excluded.model_id, dim = excluded.dim, \
                 content_hash = excluded.content_hash, embedding = excluded.embedding, \
                 ts = excluded.ts, app = excluded.app, window_name = excluded.window_name, \
                 url = excluded.url, speaker_id = excluded.speaker_id, \
                 frame_id = excluded.frame_id",
        )
        .bind(&chunk.chunk_uid)
        .bind(&chunk.source_type)
        .bind(&chunk.source_pk)
        .bind(chunk.chunk_seq)
        .bind(model_id)
        .bind(dim as i64)
        .bind(&chunk.content_hash)
        .bind(bytes)
        .bind(&meta.ts)
        .bind(&meta.app)
        .bind(&meta.window_name)
        .bind(&meta.url)
        .bind(meta.speaker_id)
        .bind(meta.frame_id)
        .execute(&mut **tx.conn())
        .await?;
        sqlx::query("DELETE FROM retrieval_embed_queue WHERE chunk_uid = ?1")
            .bind(&chunk.chunk_uid)
            .execute(&mut **tx.conn())
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Brute-force cosine KNN over one model's vectors. At the expected
    /// transcript+document corpus scale a full scan beats an ANN index; swap
    /// to a vec0 table if the corpus outgrows that (SPEC follow-up note).
    pub async fn search_retrieval_embeddings(
        &self,
        model_id: &str,
        query_vector: &[f32],
        limit: u32,
        source_type: Option<&str>,
    ) -> Result<Vec<DenseHit>, sqlx::Error> {
        let vec_param = f32_vec_to_blob(query_vector);
        let sql = format!(
            "SELECT {DENSE_HIT_COLUMNS}, \
                    vec_distance_cosine(embedding, vec_f32(?2)) AS distance \
             FROM retrieval_embeddings \
             WHERE model_id = ?1 AND (?4 IS NULL OR source_type = ?4) \
             ORDER BY distance LIMIT ?3"
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(model_id)
            .bind(&vec_param)
            .bind(limit)
            .bind(source_type)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(dense_hit_from_row).collect())
    }

    /// Display metadata for a dense transcript hit: its parent audio chunk,
    /// text and timestamp so the fusion handler can materialize the hit
    /// without a sparse-leg row.
    pub async fn get_transcription_meta(
        &self,
        transcription_id: i64,
    ) -> Result<Option<(i64, String, String)>, sqlx::Error> {
        sqlx::query_as(
            "SELECT audio_chunk_id, transcription, timestamp \
             FROM audio_transcriptions WHERE id = ?1",
        )
        .bind(transcription_id)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn retrieval_queue_depth(&self) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar("SELECT COUNT(*) FROM retrieval_embed_queue")
            .fetch_one(&self.pool)
            .await
    }

    pub async fn retrieval_embedding_count(&self) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar("SELECT COUNT(*) FROM retrieval_embeddings")
            .fetch_one(&self.pool)
            .await
    }

    /// Backfill one batch of projected frame bodies into `frames_cjk_fts`.
    /// Resumable via the `cjk.frames.last_id` watermark; marks the table done
    /// when a batch comes back short. Called every worker cycle even after
    /// done — newly captured frames land beyond the watermark and this tail
    /// catch-up keeps the projection current without touching the frame
    /// insert hot path.
    pub async fn cjk_backfill_frames_step(&self, batch: i64) -> Result<bool, sqlx::Error> {
        let last: i64 = self
            .retrieval_meta_get("cjk.frames.last_id")
            .await?
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let rows: Vec<(i64, Option<String>)> = sqlx::query_as(
            "SELECT id, full_text FROM frames WHERE id > ?1 AND full_text IS NOT NULL \
             ORDER BY id LIMIT ?2",
        )
        .bind(last)
        .bind(batch)
        .fetch_all(&self.pool)
        .await?;
        let done = (rows.len() as i64) < batch;
        let mut max_id = last;
        let mut tx = self.begin_immediate_with_retry().await?;
        for (id, text) in rows {
            max_id = max_id.max(id);
            let Some(text) = text.filter(|t| !t.trim().is_empty()) else {
                continue;
            };
            sqlx::query("INSERT OR REPLACE INTO frames_cjk_fts(rowid, body) VALUES (?1, ?2)")
                .bind(id)
                .bind(chinese_project(&text))
                .execute(&mut **tx.conn())
                .await?;
        }
        tx.commit().await?;
        if max_id > last {
            self.retrieval_meta_set("cjk.frames.last_id", &max_id.to_string())
                .await?;
        }
        if done {
            self.retrieval_meta_set("cjk.frames.done", "1").await?;
        }
        Ok(done)
    }

    /// Backfill projected AI-output bodies. Body text lives in the legacy
    /// standalone `output_search_fts` (not the outputs table), so reproject
    /// from there; rowid = output id.
    pub async fn cjk_backfill_outputs_step(&self, batch: i64) -> Result<bool, sqlx::Error> {
        let last: i64 = self
            .retrieval_meta_get("cjk.outputs.last_id")
            .await?
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let rows: Vec<(i64, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT rowid, title, body FROM output_search_fts \
             WHERE rowid > ?1 ORDER BY rowid LIMIT ?2",
        )
        .bind(last)
        .bind(batch)
        .fetch_all(&self.pool)
        .await?;
        let done = (rows.len() as i64) < batch;
        let mut max_id = last;
        let mut tx = self.begin_immediate_with_retry().await?;
        for (id, title, body) in rows {
            max_id = max_id.max(id);
            let raw = match (title, body) {
                (Some(t), Some(b)) => format!("{t}\n{b}"),
                (Some(t), None) => t,
                (None, Some(b)) => b,
                (None, None) => continue,
            };
            if raw.trim().is_empty() {
                continue;
            }
            sqlx::query("INSERT OR REPLACE INTO outputs_cjk_fts(rowid, body) VALUES (?1, ?2)")
                .bind(id)
                .bind(chinese_project(&raw))
                .execute(&mut **tx.conn())
                .await?;
        }
        tx.commit().await?;
        if max_id > last {
            self.retrieval_meta_set("cjk.outputs.last_id", &max_id.to_string())
                .await?;
        }
        if done {
            self.retrieval_meta_set("cjk.outputs.done", "1").await?;
        }
        Ok(done)
    }

    /// One-shot reprojection of the standalone document FTS so rows written
    /// before the write-time projection landed are covered too.
    pub async fn cjk_reproject_documents_once(&self) -> Result<bool, sqlx::Error> {
        if self.cjk_covered("documents").await? {
            return Ok(true);
        }
        let rows: Vec<(String, i64, String)> = sqlx::query_as(
            "SELECT c.sha256, c.ordinal, c.body FROM source_document_chunks c \
             JOIN source_documents d ON d.sha256 = c.sha256 \
             WHERE d.state = 'ready' ORDER BY c.sha256, c.ordinal",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut tx = self.begin_immediate_with_retry().await?;
        sqlx::query("DELETE FROM source_documents_fts")
            .execute(&mut **tx.conn())
            .await?;
        for (sha256, ordinal, body) in rows {
            sqlx::query(
                "INSERT INTO source_documents_fts(body, sha256, ordinal) VALUES (?1, ?2, ?3)",
            )
            .bind(chinese_project(&body))
            .bind(&sha256)
            .bind(ordinal)
            .execute(&mut **tx.conn())
            .await?;
        }
        tx.commit().await?;
        self.retrieval_meta_set("cjk.documents.done", "1").await?;
        Ok(true)
    }

    pub async fn cjk_covered(&self, table: &str) -> Result<bool, sqlx::Error> {
        Ok(self
            .retrieval_meta_get(&format!("cjk.{table}.done"))
            .await?
            .is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(
        id: i64,
        ts: &str,
        text: &str,
        speaker: Option<i64>,
    ) -> (i64, String, String, Option<i64>, String, bool) {
        (id, ts.to_string(), text.to_string(), speaker, "default".into(), true)
    }

    #[test]
    fn transcript_chunker_respects_speaker_and_caps() {
        let rows = vec![
            row(1, "2026-09-21T10:00:00Z", "你好世界", Some(1)),
            row(2, "2026-09-21T10:00:01Z", "继续说", Some(1)),
            // Speaker change forces a new chunk.
            row(3, "2026-09-21T10:00:02Z", "我不同意", Some(2)),
            // Gap > 60s forces a new chunk.
            row(4, "2026-09-21T10:09:00Z", "回到话题", Some(2)),
        ];
        let chunks = chunk_transcript_rows(&rows);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].source_pk, "1");
        assert_eq!(chunks[0].speaker_id, Some(1));
        assert!(chunks[0].text.contains("你好世界"));
        assert_eq!(chunks[1].source_pk, "3");
        assert_eq!(chunks[2].source_pk, "4");
    }

    #[test]
    fn transcript_chunker_splits_on_char_cap() {
        let long = "字".repeat(600);
        let rows = vec![
            row(1, "2026-09-21T10:00:00Z", &long, Some(1)),
            row(2, "2026-09-21T10:00:01Z", &long, Some(1)),
        ];
        let chunks = chunk_transcript_rows(&rows);
        assert_eq!(chunks.len(), 2);
    }

    #[test]
    fn content_hash_is_stable_and_uid_wellformed() {
        let chunk = RetrievalChunkInput {
            source_type: "transcript",
            source_pk: "42".into(),
            chunk_seq: 0,
            text: "hello".into(),
            ts: None,
            app: String::new(),
            window_name: String::new(),
            url: None,
            speaker_id: None,
            frame_id: None,
        };
        assert_eq!(chunk.content_hash(), chunk.content_hash());
        assert_eq!(chunk.chunk_uid(), "transcript:42:0");
        assert_eq!(chunk_uid_parts("transcript:42:0"), Some(("transcript", "42", 0)));
        assert_eq!(chunk_uid_parts("bogus"), None);
        assert_eq!(dense_key("document", "abc"), "document:abc");
    }

    #[test]
    fn timestamp_parsing_handles_both_store_formats() {
        assert!(parse_ts_seconds("2026-09-21T10:00:00Z").is_some());
        assert!(parse_ts_seconds("2026-09-21 10:00:00").is_some());
        assert!(parse_ts_seconds("not a ts").is_none());
    }
}
