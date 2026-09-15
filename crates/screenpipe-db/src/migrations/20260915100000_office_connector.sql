-- screenpipe — AI that knows everything you've seen, said, or heard
-- https://screenpipe.com
--
-- office connector schema (independent of the retired knowledge domain).
-- Feishu / Tencent Meeting imports land here as self-contained objects with
-- their own full-text search, so the connector keeps full read/write/search
-- capability without any knowledge-layer table.

CREATE TABLE office_connections (
    provider TEXT PRIMARY KEY CHECK (provider IN ('feishu', 'tencent-meeting')),
    account_namespace TEXT,
    cli_path TEXT,
    cli_version TEXT,
    credential_ref TEXT,
    runtime_status TEXT NOT NULL DEFAULT 'missing' CHECK (runtime_status IN (
        'missing', 'supported', 'unsupported')),
    auth_status TEXT NOT NULL DEFAULT 'disconnected' CHECK (auth_status IN (
        'disconnected', 'authorizing', 'authorized', 'expired', 'capability_missing')),
    sync_status TEXT NOT NULL DEFAULT 'idle' CHECK (sync_status IN (
        'idle', 'queued', 'running', 'partial', 'paused', 'failed')),
    connection_revision INTEGER NOT NULL DEFAULT 0,
    scope_revision INTEGER NOT NULL DEFAULT 0,
    enabled INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1)),
    auto_sync INTEGER NOT NULL DEFAULT 0 CHECK (auto_sync IN (0, 1)),
    last_sync_at TEXT,
    last_success_at TEXT,
    last_error_code TEXT,
    last_error_message TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE office_scopes (
    provider TEXT PRIMARY KEY REFERENCES office_connections(provider) ON DELETE CASCADE,
    scope TEXT NOT NULL,
    revision INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

-- Self-contained imported objects: body text lives here (no separate source
-- table), identity is provider/account/kind/object_id.
CREATE TABLE office_objects (
    provider TEXT NOT NULL,
    account_namespace TEXT NOT NULL,
    object_kind TEXT NOT NULL CHECK (object_kind IN (
        'message', 'document', 'meeting', 'transcript', 'summary')),
    object_id TEXT NOT NULL,
    revision TEXT,
    title TEXT,
    body_text TEXT NOT NULL DEFAULT '',
    completeness TEXT NOT NULL DEFAULT 'full',
    event_at TEXT,
    fetched_at TEXT NOT NULL,
    source_url TEXT,
    activity_anchor TEXT,
    platform_generated INTEGER NOT NULL DEFAULT 0 CHECK (platform_generated IN (0, 1)),
    state TEXT NOT NULL DEFAULT 'active' CHECK (state IN ('active', 'disabled', 'deleted')),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (provider, account_namespace, object_kind, object_id)
);
CREATE INDEX idx_office_objects_state ON office_objects(state);
CREATE INDEX idx_office_objects_fetched ON office_objects(fetched_at);

CREATE VIRTUAL TABLE office_objects_fts USING fts5(
    body,
    title,
    provider UNINDEXED,
    account_namespace UNINDEXED,
    object_kind UNINDEXED,
    object_id UNINDEXED,
    tokenize='unicode61'
);

CREATE TABLE office_cursors (
    provider TEXT NOT NULL,
    cursor_key TEXT NOT NULL,
    cursor_value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (provider, cursor_key)
);