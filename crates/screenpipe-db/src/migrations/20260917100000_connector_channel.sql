-- screenpipe — AI that knows everything you've seen, said, or heard
-- https://screenpipe.com
--
-- Generic connector channel schema for non-office sources (RSS first).
-- Same shape as the office connector tables but keyed by `connector` +
-- `key` so any channel registers without new migrations. Object identity is
-- connector/namespace/kind/object_id; body text lives here with its own FTS
-- index (same Chinese unigram/bigram projection as office), so each channel
-- keeps full read/write/search capability in the single local store.

CREATE TABLE connector_connections (
    connector TEXT NOT NULL,
    key TEXT NOT NULL DEFAULT '',
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    auto_sync INTEGER NOT NULL DEFAULT 0 CHECK (auto_sync IN (0, 1)),
    auth_status TEXT NOT NULL DEFAULT 'disconnected',
    sync_status TEXT NOT NULL DEFAULT 'idle',
    connection_revision INTEGER NOT NULL DEFAULT 0,
    scope_revision INTEGER NOT NULL DEFAULT 0,
    last_sync_at TEXT,
    last_success_at TEXT,
    last_error_code TEXT,
    last_error_message TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (connector, key)
);

CREATE TABLE connector_scopes (
    connector TEXT NOT NULL,
    key TEXT NOT NULL DEFAULT '',
    scope TEXT NOT NULL,
    revision INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (connector, key)
);

CREATE TABLE connector_objects (
    connector TEXT NOT NULL,
    namespace TEXT NOT NULL DEFAULT '',
    object_kind TEXT NOT NULL,
    object_id TEXT NOT NULL,
    revision TEXT,
    title TEXT,
    body_text TEXT NOT NULL DEFAULT '',
    completeness TEXT NOT NULL DEFAULT 'full',
    event_at TEXT,
    fetched_at TEXT NOT NULL,
    source_url TEXT,
    state TEXT NOT NULL DEFAULT 'active' CHECK (state IN ('active', 'disabled', 'deleted')),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (connector, namespace, object_kind, object_id)
);
CREATE INDEX idx_connector_objects_state ON connector_objects(connector, state);
CREATE INDEX idx_connector_objects_fetched ON connector_objects(connector, fetched_at);

CREATE VIRTUAL TABLE connector_objects_fts USING fts5(
    body,
    title,
    connector UNINDEXED,
    namespace UNINDEXED,
    object_kind UNINDEXED,
    object_id UNINDEXED,
    tokenize='unicode61'
);

CREATE TABLE connector_cursors (
    connector TEXT NOT NULL,
    key TEXT NOT NULL DEFAULT '',
    cursor_key TEXT NOT NULL,
    cursor_value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (connector, key, cursor_key)
);
