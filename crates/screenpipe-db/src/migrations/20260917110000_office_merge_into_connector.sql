-- screenpipe — AI that knows everything you've seen, said, or heard
-- https://screenpipe.com
--
-- Merge the office connector tables into the generic `connector_*` store
-- (S2 skeleton). Office-specific fields that have no generic column
-- (cli_path / cli_version / credential_ref / runtime_status /
-- account_namespace-at-connection-level / completeness / activity_anchor /
-- platform_generated) travel in a `metadata` JSON column. Scope revisions
-- carry over unchanged so OCC guards stay valid. Same-transaction data move
-- + drop: a failure rolls the whole merge back.

ALTER TABLE connector_connections ADD COLUMN metadata TEXT;
ALTER TABLE connector_objects ADD COLUMN metadata TEXT;

INSERT INTO connector_connections (connector, key, enabled, auto_sync, auth_status,
    sync_status, connection_revision, scope_revision, last_sync_at, last_success_at,
    last_error_code, last_error_message, created_at, updated_at, metadata)
SELECT 'office:' || provider, provider, enabled, auto_sync, auth_status, sync_status,
    connection_revision, scope_revision, last_sync_at, last_success_at,
    last_error_code, last_error_message, created_at, updated_at,
    json_object('account_namespace', account_namespace, 'cli_path', cli_path,
        'cli_version', cli_version, 'credential_ref', credential_ref,
        'runtime_status', runtime_status)
FROM office_connections;

INSERT INTO connector_scopes (connector, key, scope, revision, updated_at)
SELECT 'office:' || provider, provider, scope, revision, updated_at FROM office_scopes;

INSERT INTO connector_objects (connector, namespace, object_kind, object_id, revision,
    title, body_text, event_at, fetched_at, source_url, state, created_at, updated_at,
    metadata)
SELECT 'office:' || provider, account_namespace, object_kind, object_id, revision, title, body_text,
    event_at, fetched_at, source_url, state, created_at, updated_at,
    json_object('completeness', completeness, 'activity_anchor', activity_anchor,
        'platform_generated', json(CASE WHEN platform_generated THEN 'true' ELSE 'false' END))
FROM office_objects;

-- FTS rows already hold the projected token body; copy verbatim.
INSERT INTO connector_objects_fts (body, title, connector, namespace, object_kind, object_id)
SELECT body, title, 'office:' || provider, account_namespace, object_kind, object_id
FROM office_objects_fts;

INSERT INTO connector_cursors (connector, key, cursor_key, cursor_value, updated_at)
SELECT 'office:' || provider, '', cursor_key, cursor_value, updated_at FROM office_cursors;

DROP TABLE office_connections;
DROP TABLE office_scopes;
DROP TABLE office_objects;
DROP TABLE office_objects_fts;
DROP TABLE office_cursors;
