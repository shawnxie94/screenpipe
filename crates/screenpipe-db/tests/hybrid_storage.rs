// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

use screenpipe_db::storage::{PrivacyPolicy, Projection, StorageInitOptions};
use screenpipe_db::{ContentType, DatabaseManager};

async fn seed(db: &DatabaseManager, id: i64, text: Option<&str>, accessibility: Option<&str>) {
    let mut tx = db.begin_immediate_with_retry().await.unwrap();
    sqlx::query("INSERT INTO frames(id,timestamp,full_text,accessibility_text,text_json,accessibility_tree_json,app_name,window_name,device_name) VALUES(?,'2026-09-11T12:00:00Z',?,?,'[{\"text\":\"hello\",\"left\":0.5}]','{\"text\":\"hello\"}','Editor','notes','display')")
        .bind(id).bind(text).bind(accessibility).execute(&mut **tx.conn()).await.unwrap();
    tx.commit().await.unwrap();
}

async fn search(db: &DatabaseManager, query: &str) -> serde_json::Value {
    let results = db
        .search(
            query,
            ContentType::OCR,
            100,
            0,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
    serde_json::to_value(results).unwrap()
}

#[tokio::test]
async fn staged_sealed_reopened_search_and_full_payloads_match() {
    let root = tempfile::tempdir().unwrap();
    let db = DatabaseManager::new_hybrid(root.path(), Default::default(), Default::default())
        .await
        .unwrap();
    seed(&db, 1, Some("hello café 東京"), Some("accessibility")).await;
    seed(&db, 2, None, Some("fallback")).await;
    seed(&db, 3, Some(""), Some("must remain empty")).await;
    let before = db
        .frame_payloads(&[1, 2, 3], Projection::All)
        .await
        .unwrap();
    let results = search(&db, "hello").await;
    assert_eq!(db.seal_frame_payloads().await.unwrap(), 3);
    assert_eq!(
        before,
        db.frame_payloads(&[1, 2, 3], Projection::All)
            .await
            .unwrap()
    );
    assert_eq!(results, search(&db, "hello").await);
    let stored:i64=sqlx::query_scalar("SELECT count(*) FROM frames WHERE full_text IS NOT NULL OR text_json IS NOT NULL OR accessibility_tree_json IS NOT NULL OR accessibility_text IS NOT NULL").fetch_one(&db.pool).await.unwrap();
    assert_eq!(stored, 0);
    let durability: i64 = sqlx::query_scalar("PRAGMA synchronous")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(durability, 2);
    db.close().await;
    let reopened = DatabaseManager::new(
        root.path().join("db.sqlite").to_str().unwrap(),
        Default::default(),
    )
    .await
    .unwrap();
    assert!(reopened.storage_descriptor().is_some());
    assert_eq!(
        before,
        reopened
            .frame_payloads(&[1, 2, 3], Projection::All)
            .await
            .unwrap()
    );
    assert_eq!(results, search(&reopened, "hello").await);
    reopened.close().await;
    let relative = std::process::Command::new(env!("CARGO_BIN_EXE_screenpipe-storage"))
        .current_dir(root.path())
        .arg("verify")
        .arg(".")
        .output()
        .unwrap();
    assert!(
        relative.status.success(),
        "{}",
        String::from_utf8_lossy(&relative.stderr)
    );
}

#[tokio::test]
async fn privacy_completion_and_reader_leases_own_original_file_removal() {
    let root = tempfile::tempdir().unwrap();
    let options = StorageInitOptions {
        privacy: PrivacyPolicy {
            identity: "policy-1".into(),
            required_surfaces: 15,
        },
        ..Default::default()
    };
    let db = DatabaseManager::new_hybrid(root.path(), Default::default(), options)
        .await
        .unwrap();
    seed(&db, 1, Some("secret"), Some("secret")).await;
    seed(&db, 2, Some("keep"), Some("keep")).await;
    assert_eq!(db.seal_frame_payloads().await.unwrap(), 0);
    for p in db
        .frame_payloads(&[1, 2], Projection::All)
        .await
        .unwrap()
        .into_values()
    {
        assert!(db
            .replace_frame_payload(&p, "policy-1", 15, None, None)
            .await
            .unwrap());
    }
    assert_eq!(db.seal_frame_payloads().await.unwrap(), 2);
    let original: String =
        sqlx::query_scalar("SELECT search_path FROM payload_files WHERE state='published'")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let token = db.storage_read_token().await.unwrap();
    let mut payload = db
        .frame_payloads(&[1], Projection::All)
        .await
        .unwrap()
        .remove(&1)
        .unwrap();
    payload.full_text = Some("[REDACTED]".into());
    payload.accessibility_text = Some("[REDACTED]".into());
    assert!(db
        .replace_frame_payload(&payload, "policy-1", 15, None, None)
        .await
        .unwrap());
    assert!(!db
        .replace_frame_payload(&payload, "policy-1", 15, None, None)
        .await
        .unwrap());
    assert!(token.admit(&db.pool).await.is_err());
    db.reclaim_frame_payloads().await.unwrap();
    assert!(root.path().join(&original).exists());
    drop(token);
    db.reclaim_frame_payloads().await.unwrap();
    assert!(!root.path().join(&original).exists());
    assert_eq!(
        db.frame_payloads(&[2], Projection::Search).await.unwrap()[&2].text(),
        "keep"
    );
    assert!(search(&db, "secret").await.as_array().unwrap().is_empty());
    db.close().await;
}

#[tokio::test]
async fn corruption_fails_reads_without_returning_partial_payloads() {
    let root = tempfile::tempdir().unwrap();
    let db = DatabaseManager::new_hybrid(root.path(), Default::default(), Default::default())
        .await
        .unwrap();
    seed(&db, 1, Some("hello"), None).await;
    db.seal_frame_payloads().await.unwrap();
    let path: String =
        sqlx::query_scalar("SELECT detail_path FROM payload_files WHERE state='published'")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    std::fs::write(root.path().join(path), b"corrupt").unwrap();
    assert!(db
        .frame_payloads(&[1], Projection::All)
        .await
        .unwrap_err()
        .to_string()
        .contains("checksum"));
    assert_eq!(
        db.frame_payloads(&[1], Projection::Search).await.unwrap()[&1].text(),
        "hello"
    );
    db.close().await;
}

#[tokio::test]
async fn backup_restores_staging_files_and_source_identity() {
    // Unix permits URL delimiters in filenames; Windows canonicalization
    // itself supplies the verbatim prefix containing a question mark.
    let parent = tempfile::Builder::new()
        .prefix(if cfg!(unix) {
            "history?mode=ro"
        } else {
            "history"
        })
        .tempdir()
        .unwrap();
    let root = parent.path().join("source");
    let db = DatabaseManager::new_hybrid(&root, Default::default(), Default::default())
        .await
        .unwrap();
    seed(&db, 1, Some("archived"), None).await;
    db.seal_frame_payloads().await.unwrap();
    seed(&db, 2, Some("staged"), Some("other")).await;
    let identity = db.upload_source_id().await.unwrap().to_owned();
    let before = db.frame_payloads(&[1, 2], Projection::All).await.unwrap();
    let backup = parent.path().join("backup");
    db.backup_to(backup.to_str().unwrap()).await.unwrap();
    let restored = parent.path().join("restored");
    screenpipe_db::storage::restore(&backup, &restored, Default::default())
        .await
        .unwrap();
    let copy = DatabaseManager::new(
        restored.join("db.sqlite").to_str().unwrap(),
        Default::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        before,
        copy.frame_payloads(&[1, 2], Projection::All).await.unwrap()
    );
    assert_eq!(identity, copy.upload_source_id().await.unwrap());
    copy.close().await;
    db.close().await;
}

#[tokio::test]
async fn lean_retention_replaces_archived_details_and_preserves_search() {
    let root = tempfile::tempdir().unwrap();
    let db = DatabaseManager::new_hybrid(root.path(), Default::default(), Default::default())
        .await
        .unwrap();
    seed(&db, 1, Some("retained text"), Some("retained a11y")).await;
    db.seal_frame_payloads().await.unwrap();
    db.strip_heavy_text_in_range(
        "2026-09-11T00:00:00Z".parse().unwrap(),
        "2026-09-12T00:00:00Z".parse().unwrap(),
    )
    .await
    .unwrap();
    let payload = db
        .frame_payloads(&[1], Projection::All)
        .await
        .unwrap()
        .remove(&1)
        .unwrap();
    assert_eq!(payload.text(), "retained text");
    assert!(payload.text_json.is_none());
    assert!(payload.accessibility_tree_json.is_none());
    assert!(!search(&db, "retained").await.as_array().unwrap().is_empty());
    db.close().await;
}

#[tokio::test]
async fn resident_sql_resolves_wildcards_aliases_and_views() {
    let root = tempfile::tempdir().unwrap();
    let db = DatabaseManager::new_hybrid(root.path(), Default::default(), Default::default())
        .await
        .unwrap();
    seed(&db, 1, Some("private payload"), None).await;
    db.execute_raw_sql_write("CREATE VIEW payload_view AS SELECT full_text AS words FROM frames")
        .await
        .unwrap();
    for query in [
        "SELECT f.* FROM frames f LIMIT 1",
        "SELECT words FROM payload_view LIMIT 1",
        "SELECT full_text AS t FROM frames LIMIT 1",
    ] {
        assert!(
            db.query_raw_sql(query)
                .await
                .unwrap_err()
                .to_string()
                .contains("unsupported-storage-query"),
            "{query}"
        );
    }
    assert!(db
        .query_raw_sql("SELECT 'full_text' AS label, id FROM frames LIMIT 1")
        .await
        .is_ok());
    db.seal_frame_payloads().await.unwrap();
    assert_eq!(search(&db, "private").await.as_array().unwrap().len(), 1);
    db.close().await;
}

#[tokio::test]
async fn compact_preserves_logical_records() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("source");
    let db = DatabaseManager::new_hybrid(&root, Default::default(), Default::default())
        .await
        .unwrap();
    seed(&db, 1, Some("old text"), None).await;
    db.seal_frame_payloads().await.unwrap();
    let mut payload = db
        .frame_payloads(&[1], Projection::All)
        .await
        .unwrap()
        .remove(&1)
        .unwrap();
    payload.full_text = Some("replacement café".into());
    db.replace_frame_payload(&payload, "", 0, None, None)
        .await
        .unwrap();
    db.seal_frame_payloads().await.unwrap();
    db.reclaim_frame_payloads().await.unwrap();
    let before = search(&db, "replacement").await;
    let old = screenpipe_db::storage::StorageDescriptor::read(&root)
        .unwrap()
        .unwrap();
    db.close().await;
    screenpipe_db::storage::compact(&root, Default::default())
        .await
        .unwrap();
    let new = screenpipe_db::storage::StorageDescriptor::read(&root)
        .unwrap()
        .unwrap();
    assert_ne!(old.generation, new.generation);
    assert_eq!(old.payloads, new.payloads);
    assert!(!root.join(old.index).exists());
    let db = DatabaseManager::new(root.join("db.sqlite").to_str().unwrap(), Default::default())
        .await
        .unwrap();
    assert_eq!(before, search(&db, "replacement").await);
    assert!(search(&db, "old").await.as_array().unwrap().is_empty());
    db.close().await;
}

#[tokio::test]
async fn archive_record_budget_keeps_large_captures_resident_and_archives_later_work() {
    let root = tempfile::tempdir().unwrap();
    let mut options = StorageInitOptions::default();
    options.budget.record_bytes = 1024;
    options.budget.staging_bytes = options.budget.record_bytes as u64;
    let limit = options.budget.record_bytes;
    let db = DatabaseManager::new_hybrid(root.path(), Default::default(), options)
        .await
        .unwrap();
    let large = "x".repeat(limit + 1);
    let mut tx = db.begin_immediate_with_retry().await.unwrap();
    sqlx::query("INSERT INTO frames(id,timestamp,full_text) VALUES(1,'2026-09-11',?)")
        .bind(&large)
        .execute(&mut **tx.conn())
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM frames")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        1
    );
    seed(&db, 2, Some("complete payload"), None).await;
    assert_eq!(db.seal_frame_payloads().await.unwrap(), 1);
    assert_eq!(
        db.frame_payloads(&[1], Projection::All).await.unwrap()[&1]
            .full_text
            .as_deref(),
        Some(large.as_str())
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT staging_bytes FROM storage_metadata")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        0
    );
    assert!(db
        .admit_consumer_sync()
        .unwrap_err()
        .to_string()
        .contains("binding unavailable"));
    db.close().await;
}

#[tokio::test]
async fn current_format_initialization_rejects_legacy_sqlite_without_modifying_it() {
    let root = tempfile::tempdir().unwrap();
    let legacy = b"legacy sqlite bytes";
    std::fs::write(root.path().join("db.sqlite"), legacy).unwrap();

    assert!(
        DatabaseManager::ensure_hybrid_storage(root.path(), Default::default())
            .await
            .is_err()
    );
    assert_eq!(
        std::fs::read(root.path().join("db.sqlite")).unwrap(),
        legacy
    );
    assert!(!root.path().join("storage.json").exists());
}

#[cfg(feature = "storage-fault-injection")]
#[tokio::test]
async fn interrupted_initialization_resumes_its_recorded_generation() {
    for point in [
        "initialization_schema_step",
        "initialization_ready",
        "initialization_activated",
    ] {
        let root = tempfile::tempdir().unwrap();
        let killed = std::process::Command::new(env!("CARGO_BIN_EXE_screenpipe-storage"))
            .arg("init")
            .arg(root.path())
            .env("SCREENPIPE_STORAGE_CRASH_AT", point)
            .output()
            .unwrap();
        assert_eq!(
            killed.status.code(),
            Some(86),
            "{}",
            String::from_utf8_lossy(&killed.stderr)
        );
        let expected: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.path().join("storage-init.json")).unwrap())
                .unwrap();
        let db = DatabaseManager::new_hybrid(root.path(), Default::default(), Default::default())
            .await
            .unwrap();
        let active = screenpipe_db::storage::StorageDescriptor::read(root.path())
            .unwrap()
            .unwrap();
        assert_eq!(active.generation, expected["generation"].as_str().unwrap());
        seed(&db, 1, Some("ready after interruption"), None).await;
        db.close().await;
    }
}

#[cfg(feature = "storage-fault-injection")]
#[tokio::test]
async fn descriptor_and_catalog_paths_stay_inside_their_generation() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let db = DatabaseManager::new_hybrid(root.path(), Default::default(), Default::default())
        .await
        .unwrap();
    seed(&db, 1, Some("path ownership"), None).await;
    db.seal_frame_payloads().await.unwrap();
    let original: String =
        sqlx::query_scalar("SELECT search_path FROM payload_files WHERE state='published'")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let original_path = root.path().join(&original);
    std::fs::rename(&original_path, outside.path().join("search.parquet")).unwrap();
    std::os::unix::fs::symlink(outside.path().join("search.parquet"), &original_path).unwrap();
    assert!(db.frame_payloads(&[1], Projection::Search).await.is_err());
    db.close().await;
    assert!(DatabaseManager::new(
        root.path().join("db.sqlite").to_str().unwrap(),
        Default::default()
    )
    .await
    .is_err());
    let mut descriptor = screenpipe_db::storage::StorageDescriptor::read(root.path())
        .unwrap()
        .unwrap();
    descriptor.index = "elsewhere/index.sqlite".into();
    std::fs::write(
        root.path().join("storage.json"),
        serde_json::to_vec(&descriptor).unwrap(),
    )
    .unwrap();
    assert!(DatabaseManager::new(
        root.path().join("db.sqlite").to_str().unwrap(),
        Default::default()
    )
    .await
    .is_err());
}
