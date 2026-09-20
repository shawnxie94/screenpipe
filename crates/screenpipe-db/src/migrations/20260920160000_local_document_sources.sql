-- screenpipe — AI that knows everything you've seen, said, or heard
-- https://screenpipe.com
--
-- Local document directory auto-ingest (Roadmap step 2):
-- - document_sources: user-chosen watch directories with enable/disable and
--   per-source include/exclude configuration. Explicit user consent only —
--   nothing is watched that the user did not add here.
-- - document_locations: one row per (source, watched file path). Identity is
--   the location, NOT the content: identical bytes in two paths each keep
--   their own location row pointing at the same source_documents sha256.
--   Deletes only move the row to 'missing' — the managed copy survives.
--   Moves/renames surface as missing(old) + imported(new).

CREATE TABLE document_sources (
    id TEXT PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    include_exts TEXT NOT NULL DEFAULT '',
    exclude_globs TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE document_locations (
    source_id TEXT NOT NULL,
    path TEXT NOT NULL,
    sha256 TEXT NOT NULL DEFAULT '',
    file_name TEXT NOT NULL,
    ext TEXT NOT NULL DEFAULT '',
    size_bytes INTEGER NOT NULL DEFAULT 0,
    modified_ms INTEGER NOT NULL DEFAULT 0,
    state TEXT NOT NULL DEFAULT 'pending'
        CHECK (state IN ('pending', 'imported', 'failed', 'missing')),
    error_message TEXT,
    imported_at TEXT,
    last_seen_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (source_id, path)
);

CREATE INDEX idx_document_locations_sha256 ON document_locations(sha256);
CREATE INDEX idx_document_locations_state ON document_locations(state);
