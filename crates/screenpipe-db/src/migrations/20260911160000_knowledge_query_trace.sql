-- Query trace for opt-in reproducible reads (X-Screenpipe-Trace).
-- One row per traced read: what was queried and a fingerprint over the
-- serialized data rows only (never generated_at / time_range envelopes).
-- Bounded by application policy: at most 200 rows per trace_key, enforced
-- before the read is served (429 beyond that). No cleanup job in this batch.

CREATE TABLE IF NOT EXISTS knowledge_query_trace (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    trace_key TEXT NOT NULL,
    method TEXT NOT NULL,
    path TEXT NOT NULL,
    query_string TEXT,
    result_fingerprint TEXT NOT NULL,
    row_count INTEGER NOT NULL,
    status_code INTEGER NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_knowledge_query_trace_key_id
    ON knowledge_query_trace(trace_key, id);
