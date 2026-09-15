-- screenpipe — AI that knows everything you've seen, said, or heard
-- https://screenpipe.com

CREATE TABLE IF NOT EXISTS task_definitions (
    definition_id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL,
    origin TEXT NOT NULL CHECK (origin IN ('builtin', 'user', 'connection')),
    schema_version INTEGER NOT NULL DEFAULT 1,
    config_revision TEXT NOT NULL,
    config_ref TEXT,
    trigger_json TEXT NOT NULL DEFAULT '{}',
    enabled INTEGER NOT NULL DEFAULT 1,
    resource_class TEXT NOT NULL,
    retry_policy_json TEXT NOT NULL DEFAULT '{}',
    model_binding_policy TEXT,
    revision INTEGER NOT NULL DEFAULT 1,
    owner_generation INTEGER NOT NULL DEFAULT 0,
    migration_state TEXT NOT NULL DEFAULT 'unified',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TABLE IF NOT EXISTS task_runs (
    run_id TEXT PRIMARY KEY NOT NULL,
    definition_id TEXT,
    definition_revision TEXT NOT NULL,
    root_run_id TEXT NOT NULL,
    parent_run_id TEXT,
    retry_of TEXT,
    trigger_key TEXT NOT NULL,
    input_hash TEXT NOT NULL,
    input_refs TEXT NOT NULL DEFAULT '{}',
    config_snapshot TEXT NOT NULL DEFAULT '{}',
    state TEXT NOT NULL CHECK (state IN ('queued','running','succeeded','failed','timed_out','cancelling','cancelled','paused','needs_attention')),
    revision INTEGER NOT NULL DEFAULT 1,
    priority INTEGER NOT NULL DEFAULT 100,
    not_before TEXT,
    deadline TEXT,
    cursor TEXT,
    lease_owner TEXT,
    lease_token TEXT,
    lease_expires_at TEXT,
    owner_generation INTEGER NOT NULL DEFAULT 0,
    output_refs TEXT NOT NULL DEFAULT '{}',
    error_code TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE UNIQUE INDEX IF NOT EXISTS task_runs_active_dedupe
    ON task_runs(definition_id, input_hash, trigger_key)
    WHERE state IN ('queued','running','paused','cancelling');
CREATE INDEX IF NOT EXISTS task_runs_claim_idx
    ON task_runs(state, not_before, priority, created_at);
CREATE INDEX IF NOT EXISTS task_runs_root_idx ON task_runs(root_run_id, created_at);

CREATE TABLE IF NOT EXISTS task_attempts (
    attempt_id TEXT PRIMARY KEY NOT NULL,
    run_id TEXT NOT NULL,
    attempt_no INTEGER NOT NULL,
    started_at TEXT,
    finished_at TEXT,
    runtime_binding TEXT,
    model_calls_used INTEGER NOT NULL DEFAULT 0,
    retry_reason TEXT,
    outcome TEXT,
    owner_generation INTEGER NOT NULL DEFAULT 0,
    lease_token TEXT,
    UNIQUE(run_id, attempt_no)
);
CREATE INDEX IF NOT EXISTS task_attempts_run_idx ON task_attempts(run_id, attempt_no);

CREATE TABLE IF NOT EXISTS task_events (
    run_id TEXT NOT NULL,
    seq INTEGER NOT NULL,
    attempt_id TEXT,
    phase TEXT NOT NULL,
    event_type TEXT NOT NULL,
    timestamp TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    safe_metadata TEXT NOT NULL DEFAULT '{}',
    payload_ref TEXT,
    output_refs TEXT NOT NULL DEFAULT '{}',
    PRIMARY KEY(run_id, seq)
);
CREATE INDEX IF NOT EXISTS task_events_timestamp_idx ON task_events(timestamp);

CREATE TABLE IF NOT EXISTS task_legacy_map (
    legacy_namespace TEXT NOT NULL,
    legacy_id TEXT NOT NULL,
    definition_id TEXT,
    run_id TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    PRIMARY KEY(legacy_namespace, legacy_id)
);

CREATE TABLE IF NOT EXISTS task_owner_state (
    kind TEXT PRIMARY KEY NOT NULL,
    owner_generation INTEGER NOT NULL DEFAULT 0,
    migration_state TEXT NOT NULL DEFAULT 'legacy',
    checkpoint TEXT,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

