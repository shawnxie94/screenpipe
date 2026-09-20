-- screenpipe — AI that knows everything you've seen, said, or heard
-- https://screenpipe.com
--
-- Connector fetch hygiene + observability:
-- - connector_objects.content_hash: SHA-256 of the normalized body. Backs
--   cross-identity dedup (same content under a different guid/URL skips) and
--   no-op upserts (unchanged content skips the row update and FTS rewrite,
--   removing the per-sync write amplification on unchanged feeds).
-- - connector_sync_runs: one row per sync attempt with processed/skipped/
--   failed counts, so "what happened in past syncs" is answerable from
--   history instead of only the last error on the connection row.

ALTER TABLE connector_objects ADD COLUMN content_hash TEXT;
CREATE INDEX idx_connector_objects_hash ON connector_objects(connector, content_hash);

CREATE TABLE connector_sync_runs (
    id INTEGER PRIMARY KEY,
    connector TEXT NOT NULL,
    key TEXT NOT NULL DEFAULT '',
    started_at TEXT NOT NULL,
    finished_at TEXT,
    status TEXT NOT NULL DEFAULT 'running'
        CHECK (status IN ('running', 'succeeded', 'partial', 'failed', 'cancelled')),
    processed INTEGER NOT NULL DEFAULT 0,
    skipped INTEGER NOT NULL DEFAULT 0,
    failed INTEGER NOT NULL DEFAULT 0,
    error_code TEXT,
    error_message TEXT
);
CREATE INDEX idx_connector_sync_runs_channel ON connector_sync_runs(connector, key, started_at);
