-- screenpipe — AI that knows everything you've seen, said, or heard
-- https://screenpipe.com
--
-- Unified hybrid retrieval (SPEC sqlite://artifact/spec/unified-hybrid-retrieval):
-- - retrieval_index_meta: watermarks and progress for the background indexer
--   and the CJK FTS projection backfill.
-- - retrieval_embeddings: dense chunk vectors, plain table + sqlite-vec
--   scalar functions (same pattern as speaker_embeddings). Candidate
--   metadata is denormalized here so the fusion layer never joins five
--   origin tables. Vector is little-endian f32 blob; cosine distance via
--   vec_distance_cosine(embedding, vec_f32(?)).
-- - retrieval_embed_queue: content-hash idempotent work queue drained by the
--   background embedder. Text snapshot lives here only while pending.
-- - CJK companion FTS tables: bodies are write-time chinese_project()
--   output, rowid = origin row id. The legacy FTS tables keep raw text
--   (frames_fts is external-content and cannot hold projected bodies), so
--   CJK queries route here once the backfill marks the table covered.

CREATE TABLE IF NOT EXISTS retrieval_index_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS retrieval_embeddings (
    chunk_uid TEXT PRIMARY KEY,
    source_type TEXT NOT NULL,
    source_pk TEXT NOT NULL,
    chunk_seq INTEGER NOT NULL DEFAULT 0,
    model_id TEXT NOT NULL,
    dim INTEGER NOT NULL,
    content_hash TEXT NOT NULL,
    embedding BLOB NOT NULL CHECK (typeof(embedding) = 'blob'),
    ts TEXT,
    app TEXT NOT NULL DEFAULT '',
    window_name TEXT NOT NULL DEFAULT '',
    url TEXT,
    speaker_id INTEGER,
    frame_id INTEGER,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE(source_type, source_pk, chunk_seq, model_id)
);
CREATE INDEX IF NOT EXISTS idx_retrieval_embeddings_model
    ON retrieval_embeddings(model_id, source_type);

CREATE TABLE IF NOT EXISTS retrieval_embed_queue (
    chunk_uid TEXT PRIMARY KEY,
    source_type TEXT NOT NULL,
    source_pk TEXT NOT NULL,
    chunk_seq INTEGER NOT NULL DEFAULT 0,
    content_hash TEXT NOT NULL,
    text TEXT NOT NULL,
    ts TEXT,
    app TEXT NOT NULL DEFAULT '',
    window_name TEXT NOT NULL DEFAULT '',
    url TEXT,
    speaker_id INTEGER,
    frame_id INTEGER,
    state TEXT NOT NULL DEFAULT 'pending' CHECK (state IN ('pending', 'processing')),
    attempts INTEGER NOT NULL DEFAULT 0,
    enqueued_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE INDEX IF NOT EXISTS idx_retrieval_embed_queue_state
    ON retrieval_embed_queue(state, enqueued_at);

CREATE VIRTUAL TABLE IF NOT EXISTS frames_cjk_fts USING fts5(body, tokenize='unicode61');
CREATE VIRTUAL TABLE IF NOT EXISTS outputs_cjk_fts USING fts5(body, tokenize='unicode61');
CREATE VIRTUAL TABLE IF NOT EXISTS connector_cjk_fts USING fts5(body, tokenize='unicode61');
CREATE VIRTUAL TABLE IF NOT EXISTS semantic_cjk_fts USING fts5(body, tokenize='unicode61');
