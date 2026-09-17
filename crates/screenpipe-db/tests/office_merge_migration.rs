// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

//! Verifies the office→connector merge migration moves data faithfully:
//! scope revisions survive (OCC continuity), office-specific fields land in
//! `metadata`, FTS rows carry over, and the legacy tables are gone.

use sqlx::sqlite::SqlitePoolOptions;

const MERGE_VERSION: i64 = 20260917110000;

/// Migration 20241108202826 uses `vec_length()` in a CHECK constraint, which
/// requires the sqlite-vec extension. Register the same auto-extension the
/// production `DatabaseManager::new` registers before any connection opens.
fn register_sqlite_vec() {
    unsafe {
        type SqliteExtensionInit = unsafe extern "C" fn(
            *mut libsqlite3_sys::sqlite3,
            *mut *mut std::ffi::c_char,
            *const libsqlite3_sys::sqlite3_api_routines,
        ) -> std::ffi::c_int;
        let init = std::mem::transmute::<unsafe extern "C" fn(), SqliteExtensionInit>(
            sqlite_vec::sqlite3_vec_init,
        );
        let rc = libsqlite3_sys::sqlite3_auto_extension(Some(init));
        assert_eq!(rc, libsqlite3_sys::SQLITE_OK);
    }
}

async fn run_migrations_before_merge(pool: &sqlx::SqlitePool) {
    let migrator = sqlx::migrate!("./src/migrations");
    for m in migrator.migrations.iter() {
        if m.version >= MERGE_VERSION {
            break;
        }
        sqlx::raw_sql(sqlx::AssertSqlSafe(m.sql.as_str())).execute(pool).await.unwrap();
    }
}

async fn run_merge_migration(pool: &sqlx::SqlitePool) {
    let migrator = sqlx::migrate!("./src/migrations");
    for m in migrator.migrations.iter() {
        if m.version >= MERGE_VERSION {
            sqlx::raw_sql(sqlx::AssertSqlSafe(m.sql.as_str())).execute(pool).await.unwrap();
        }
    }
}

#[tokio::test]
async fn office_merge_migration_moves_data_faithfully() {
    register_sqlite_vec();
    let pool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .unwrap();
    run_migrations_before_merge(&pool).await;

    // Legacy shape: one provider with a bumped scope revision (OCC state),
    // an imported object, an FTS row, and a cursor.
    sqlx::query(
        "INSERT INTO office_connections (provider, account_namespace, runtime_status, \
         auth_status, scope_revision, connection_revision) \
         VALUES ('feishu', 'acct-1', 'supported', 'authorized', 7, 2)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO office_scopes (provider, scope, revision) \
         VALUES ('feishu', '{\"chat_ids\":[\"oc1\"]}', 3)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO office_objects (provider, account_namespace, object_kind, object_id, \
         title, body_text, completeness, event_at, fetched_at, activity_anchor, \
         platform_generated) \
         VALUES ('feishu', 'oc1', 'message', 'om_1', '标题', '正文内容', 'partial:truncated', \
         '2026-09-16T00:00:00.000000Z', '2026-09-16T01:00:00.000000Z', 'anchor-1', 1)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO office_objects_fts (body, title, provider, account_namespace, \
         object_kind, object_id) VALUES ('正文内容', '标题', 'feishu', 'oc1', 'message', 'om_1')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO office_cursors (provider, cursor_key, cursor_value) \
         VALUES ('feishu', 'chat:oc1', 'om_9')",
    )
    .execute(&pool)
    .await
    .unwrap();

    run_merge_migration(&pool).await;

    // Legacy tables are gone.
    let legacy: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name LIKE 'office\\_%' ESCAPE '\\'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(legacy, 0, "office_* tables must be dropped");

    // Connection row: revisions carried over, office fields in metadata.
    let row: (String, i64, i64, Option<String>) = sqlx::query_as(
        "SELECT key, scope_revision, connection_revision, metadata \
         FROM connector_connections WHERE connector = 'office:feishu'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0, "feishu");
    assert_eq!(row.1, 7, "scope_revision must survive (OCC continuity)");
    assert_eq!(row.2, 2);
    let meta: serde_json::Value = serde_json::from_str(row.3.unwrap().as_str()).unwrap();
    assert_eq!(meta["runtime_status"], "supported");
    assert_eq!(meta["account_namespace"], "acct-1");

    // Scope revision carried over.
    let (revision,): (i64,) = sqlx::query_as(
        "SELECT revision FROM connector_scopes WHERE connector = 'office:feishu'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(revision, 3);

    // Object: metadata holds the office-only fields.
    let obj: (String, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT metadata, title, body_text FROM connector_objects \
         WHERE connector = 'office:feishu' AND object_id = 'om_1'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let obj_meta: serde_json::Value = serde_json::from_str(obj.0.as_str()).unwrap();
    assert_eq!(obj_meta["completeness"], "partial:truncated");
    assert_eq!(obj_meta["platform_generated"], true);
    assert_eq!(obj_meta["activity_anchor"], "anchor-1");
    assert_eq!(obj.1.as_deref(), Some("标题"));

    // FTS row copied with the per-provider connector id.
    let fts: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM connector_objects_fts \
         WHERE connector = 'office:feishu' AND object_id = 'om_1'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(fts, 1);

    // Cursor moved under the provider key.
    let cursor: (String, String) = sqlx::query_as(
        "SELECT key, cursor_value FROM connector_cursors WHERE connector = 'office:feishu'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(cursor.0, "feishu");
    assert_eq!(cursor.1, "om_9");
}
