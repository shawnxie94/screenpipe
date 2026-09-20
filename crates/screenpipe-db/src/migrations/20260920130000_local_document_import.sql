-- screenpipe — AI that knows everything you've seen, said, or heard
-- https://screenpipe.com
--
-- Local document manual import (source_documents layer):
-- - source_documents: one row per unique file content, keyed by SHA-256 so a
--   re-import of identical bytes is a no-op. Metadata only — the raw binary
--   lives in the content-addressed managed copy under ~/.screenpipe/documents.
-- - source_document_chunks: stable split of the extracted text so citations
--   can reference a chunk ordinal.
-- - source_documents_fts: FTS index over chunk bodies, mirroring the
--   connector_objects_fts shape (unicode61 + chinese projection at write time).

CREATE TABLE source_documents (
    sha256 TEXT PRIMARY KEY,
    file_name TEXT NOT NULL,
    ext TEXT NOT NULL DEFAULT '',
    size_bytes INTEGER NOT NULL DEFAULT 0,
    original_path TEXT,
    managed_path TEXT,
    state TEXT NOT NULL DEFAULT 'stored' CHECK (state IN ('stored', 'ready', 'failed')),
    truncated INTEGER NOT NULL DEFAULT 0,
    chunk_count INTEGER NOT NULL DEFAULT 0,
    error_message TEXT,
    imported_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE INDEX idx_source_documents_state ON source_documents(state, imported_at);

CREATE TABLE source_document_chunks (
    sha256 TEXT NOT NULL,
    ordinal INTEGER NOT NULL,
    body TEXT NOT NULL,
    PRIMARY KEY (sha256, ordinal)
);

CREATE VIRTUAL TABLE source_documents_fts USING fts5(
    body,
    sha256 UNINDEXED,
    ordinal UNINDEXED,
    tokenize='unicode61'
);
