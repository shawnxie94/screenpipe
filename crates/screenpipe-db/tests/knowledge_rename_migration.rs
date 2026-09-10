// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com
// Acceptance probes for the brain → knowledge database rename (migration
// 20260911120000): object names, integrity, persisted-value normalization and
// the search FTS rebuild. Synthetic SQLite only.

use screenpipe_db::DatabaseManager;

const RENAME_MIGRATION_SQL: &str =
    include_str!("../src/migrations/20260911120000_rename_brain_to_knowledge.sql");

const NEW_TABLES: &[&str] = &[
    "knowledge_state",
    "knowledge_sources",
    "knowledge_source_revisions",
    "knowledge_work_units",
    "knowledge_work_unit_revisions",
    "knowledge_items",
    "knowledge_item_versions",
    "knowledge_rejections",
    "knowledge_dependencies",
    "knowledge_jobs",
    "knowledge_answers",
    "knowledge_feedback",
    "knowledge_deletions",
    "knowledge_cleanup_items",
    "knowledge_tombstones",
    "knowledge_history_entries",
    "knowledge_history_coverage",
    "knowledge_migrations",
    "knowledge_office_connections",
    "knowledge_office_scopes",
    "knowledge_office_objects",
    "knowledge_office_cursors",
    "knowledge_search_documents",
];

const NEW_INDEXES: &[&str] = &[
    "idx_knowledge_sources_locator",
    "idx_knowledge_sources_office",
    "idx_knowledge_sources_state",
    "idx_knowledge_sources_captured",
    "idx_knowledge_wu_scope",
    "idx_knowledge_item_state",
    "idx_knowledge_deps_source",
    "idx_knowledge_deps_consumer",
    "idx_knowledge_jobs_active_input",
    "idx_knowledge_jobs_claim",
    "idx_knowledge_jobs_batch",
    "idx_knowledge_feedback_target",
    "idx_knowledge_cleanup_deletion",
    "idx_knowledge_search_docs_ref",
    "idx_knowledge_deletions_journal_seq",
    "idx_knowledge_history_coverage_span",
];

async fn user_objects(pool: &sqlx::SqlitePool) -> Vec<(String, String)> {
    sqlx::query_as::<_, (String, String)>(
        "SELECT type, name FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'",
    )
    .fetch_all(pool)
    .await
    .expect("sqlite_master listing")
}

fn assert_rename_complete(objects: &[(String, String)]) {
    for name in NEW_TABLES {
        assert!(
            objects.iter().any(|(_, n)| n == name),
            "missing renamed table {name}"
        );
    }
    for name in NEW_INDEXES {
        assert!(
            objects.iter().any(|(_, n)| n == name),
            "missing rebuilt index {name}"
        );
    }
    assert!(
        objects
            .iter()
            .any(|(ty, n)| ty == "table" && n == "knowledge_search_fts"),
        "missing rebuilt FTS table knowledge_search_fts"
    );
    let stale: Vec<&(String, String)> = objects
        .iter()
        .filter(|(_, name)| name.contains("brain"))
        .collect();
    assert!(stale.is_empty(), "stale brain_* objects remain: {stale:?}");
}

async fn assert_integrity(pool: &sqlx::SqlitePool) {
    let violations: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM pragma_foreign_key_check")
            .fetch_one(pool)
            .await
            .expect("foreign_key_check");
    assert_eq!(violations, 0, "foreign_key_check reported violations");
    let results: Vec<(String,)> = sqlx::query_as("PRAGMA integrity_check")
        .fetch_all(pool)
        .await
        .expect("integrity_check");
    assert!(
        results.iter().all(|(r,)| r == "ok"),
        "integrity_check failed: {results:?}"
    );
}

/// A fully migrated fresh database must expose only the new object names.
#[tokio::test]
async fn fresh_database_has_only_knowledge_object_names() {
    let db = DatabaseManager::new("sqlite::memory:", Default::default())
        .await
        .expect("migrated in-memory database");
    let objects = user_objects(&db.pool).await;
    assert_rename_complete(&objects);
    assert_integrity(&db.pool).await;
}

/// Replay the pre-rename schema with legacy persisted values, then run the
/// rename migration directly: values normalize, the FTS rebuilds from the
/// projection table, and table data survives the renames.
#[tokio::test]
async fn rename_migration_normalizes_legacy_values_and_rebuilds_fts() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("in-memory pool");

    // Historical migration chain exactly as a pre-rename database has it.
    for sql in [
        include_str!("../src/migrations/20260907120000_create_brain.sql"),
        include_str!("../src/migrations/20260908130000_brain_deletion_journal_seq.sql"),
        include_str!("../src/migrations/20260908150000_create_tasks.sql"),
        include_str!("../src/migrations/20260908153000_brain_history_coverage_dedupe.sql"),
    ] {
        sqlx::raw_sql(sql).execute(&pool).await.expect("seed migration");
    }

    // Persisted legacy values: the seeded builtin definitions, a legacy owner
    // state row, and the bare 'brain' owner kind (merged into 'knowledge').
    let seeded: Vec<(String, String)> = sqlx::query_as(
        "SELECT definition_id, kind FROM task_definitions ORDER BY definition_id",
    )
    .fetch_all(&pool)
    .await
    .expect("seeded definitions");
    assert!(seeded.contains(&("brain.extract".into(), "brain_extract".into())));
    assert!(seeded.contains(&("office.sync".into(), "office_sync".into())));
    sqlx::query("INSERT INTO task_owner_state (kind) VALUES ('brain_extract')")
        .execute(&pool)
        .await
        .expect("legacy owner state");
    sqlx::query("INSERT INTO task_owner_state (kind, owner_generation) VALUES ('brain', 7)")
        .execute(&pool)
        .await
        .expect("legacy bare owner kind");

    // Table data must survive the renames; the FTS row is deliberately stale
    // (missing doc-2) so the rebuild has real work to do.
    sqlx::query("INSERT INTO brain_jobs (kind, scope_key, revision) VALUES ('extract', 'scope-a', 'r1')")
        .execute(&pool)
        .await
        .expect("legacy job row");
    sqlx::query(
        "INSERT INTO brain_search_documents (doc_id, kind, ref_uid, ref_revision, body) \
         VALUES ('doc-1', 'frame', 'f-1', 'r1', 'the unique needle survives the rebuild')",
    )
    .execute(&pool)
    .await
    .expect("search doc 1");
    sqlx::query(
        "INSERT INTO brain_search_documents (doc_id, kind, ref_uid, ref_revision, body) \
         VALUES ('doc-2', 'frame', 'f-2', 'r1', 'second searchable document')",
    )
    .execute(&pool)
    .await
    .expect("search doc 2");
    sqlx::query("INSERT INTO brain_search_fts (doc_id, body) VALUES ('doc-1', 'stale fts row')")
        .execute(&pool)
        .await
        .expect("stale fts row");

    sqlx::raw_sql(RENAME_MIGRATION_SQL)
        .execute(&pool)
        .await
        .expect("rename migration");

    let objects = user_objects(&pool).await;
    assert_rename_complete(&objects);
    assert_integrity(&pool).await;

    // Persisted task values are normalized to the new prefixes; the bare legacy
    // 'brain' owner kind is merged into 'knowledge', office.sync stays as is.
    let definitions: Vec<(String, String)> = sqlx::query_as(
        "SELECT definition_id, kind FROM task_definitions ORDER BY definition_id",
    )
    .fetch_all(&pool)
    .await
    .expect("definitions after rename");
    assert_eq!(
        definitions,
        vec![
            ("knowledge.backfill".into(), "knowledge_backfill".into()),
            ("knowledge.compile".into(), "knowledge_compile".into()),
            ("knowledge.extract".into(), "knowledge_extract".into()),
            ("office.sync".into(), "office_sync".into()),
        ]
    );
    let owner_kinds: Vec<(String,)> =
        sqlx::query_as("SELECT kind FROM task_owner_state ORDER BY kind")
            .fetch_all(&pool)
            .await
            .expect("owner kinds");
    assert_eq!(
        owner_kinds,
        vec![("knowledge".into(),), ("knowledge_extract".into(),)]
    );

    // Renames move rows, they do not drop them.
    let jobs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM knowledge_jobs")
        .fetch_one(&pool)
        .await
        .expect("renamed job rows");
    assert_eq!(jobs, 1);

    // The FTS projection matches the documents table exactly (stale doc-1
    // content was replaced by the backfill) and answers MATCH queries.
    let docs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM knowledge_search_documents")
        .fetch_one(&pool)
        .await
        .expect("document count");
    let fts_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM knowledge_search_fts")
        .fetch_one(&pool)
        .await
        .expect("fts count");
    assert_eq!(fts_rows, docs);
    assert_eq!(docs, 2);
    let hits: Vec<(String,)> =
        sqlx::query_as("SELECT doc_id FROM knowledge_search_fts WHERE knowledge_search_fts MATCH 'needle'")
            .fetch_all(&pool)
            .await
            .expect("fts match");
    assert_eq!(hits, vec![("doc-1".into(),)]);
}
