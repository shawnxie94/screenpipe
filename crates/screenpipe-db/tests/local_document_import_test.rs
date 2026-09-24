// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

//! Local document import persistence: content-addressed idempotency, chunked
//! FTS search with snippets, and visible failure states. Chunking stability
//! itself is unit-tested inside `db::documents`.

use screenpipe_db::DatabaseManager;

async fn test_db() -> DatabaseManager {
    DatabaseManager::new("sqlite::memory:", Default::default())
        .await
        .expect("db init")
}

#[tokio::test]
async fn import_is_idempotent_by_content_hash() {
    let db = test_db().await;
    let first = db
        .document_import_stored("aaa", "a.md", "md", 100, Some("/tmp/a.md"), Some("/managed/aaa.md"))
        .await
        .unwrap();
    assert!(first, "first import should create");
    let second = db
        .document_import_stored("aaa", "a.md", "md", 100, Some("/tmp/a.md"), Some("/managed/aaa.md"))
        .await
        .unwrap();
    assert!(!second, "re-import of identical bytes must be a no-op");

    let doc = db.document_get("aaa").await.unwrap().unwrap();
    assert_eq!(doc.state, "stored");
}

#[tokio::test]
async fn text_becomes_searchable_with_snippets() {
    let db = test_db().await;
    db.document_import_stored("bbb", "规格说明.md", "md", 100, None, Some("/managed/bbb.md"))
        .await
        .unwrap();
    let state = db
        .document_mark_ready("bbb", "第一段是背景。\n\n核心技术结论是本地优先检索。", false)
        .await
        .unwrap();
    assert_eq!(state, "ready");

    let hits = db.document_search("核心技术", 10).await.unwrap();
    assert!(!hits.is_empty(), "fts must find the chunk");
    let hit = &hits[0];
    assert_eq!(hit.file_name, "规格说明.md");
    assert!(hit.managed_path.as_deref().unwrap().contains("bbb"));
    assert!(!hit.snippet.is_empty(), "fts hit should carry a snippet");
    // The snippet must quote the ORIGINAL text: the FTS column stores the CJK
    // projection (`cu核 cb核心`), and leaking those tokens into the UI is what
    // made Chinese document hits unreadable.
    assert!(hit.snippet.contains("核心技术"), "snippet: {}", hit.snippet);
    assert!(!hit.snippet.contains("cu核"), "snippet: {}", hit.snippet);
    assert!(!hit.snippet.contains("cb核"), "snippet: {}", hit.snippet);

    // Empty query browses ready documents.
    let browse = db.document_search("", 10).await.unwrap();
    assert_eq!(browse.len(), 1);

    // FTS syntax in user input stays literal instead of erroring.
    assert!(db.document_search("core AND (tech)", 10).await.unwrap().is_empty());
}

#[tokio::test]
async fn unified_document_count_deduplicates_multiple_matching_chunks() {
    let db = test_db().await;
    db.document_import_stored("sha-dedup", "long.md", "md", 2_000, None, None)
        .await
        .unwrap();
    let text = format!(
        "dedup-token {}\n\nThe second chunk also contains dedup-token and is the display candidate.",
        "filler ".repeat(220)
    );
    db.document_mark_ready("sha-dedup", &text, false)
        .await
        .unwrap();

    let chunks = db.document_search_in_range("dedup-token", 20, None, None).await.unwrap();
    assert!(chunks.len() >= 2, "FTS must retain chunk-level recall");
    assert!(chunks.iter().all(|hit| hit.sha256 == "sha-dedup"));
    assert_eq!(
        db.document_search_count_in_range("dedup-token", None, None)
            .await
            .unwrap(),
        1,
        "pagination total counts top-level documents, not matching chunks"
    );

    let page = db.document_search_in_range("dedup-token", 1, None, None).await.unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].sha256, "sha-dedup");
}

#[tokio::test]
async fn empty_text_is_a_visible_failure() {
    let db = test_db().await;
    db.document_import_stored("ccc", "scanned.pdf", "pdf", 10, None, Some("/managed/ccc.pdf"))
        .await
        .unwrap();
    let state = db.document_mark_ready("ccc", "   \n", false).await.unwrap();
    assert_eq!(state, "failed");

    let doc = db.document_get("ccc").await.unwrap().unwrap();
    assert_eq!(doc.state, "failed");
    let reason = doc.error_message.unwrap();
    assert!(reason.contains("文字"), "reason must be user-visible");

    // Failed documents never surface in search or browse.
    assert!(db.document_search("scanned", 10).await.unwrap().is_empty());
    assert!(db.document_search("", 10).await.unwrap().is_empty());
}

#[tokio::test]
async fn failed_import_is_recorded_without_bytes() {
    let db = test_db().await;
    db.document_mark_failed(
        None,
        "huge.bin",
        "bin",
        99_999_999,
        Some("/tmp/huge.bin"),
        "不支持的文件类型 .bin",
    )
    .await
    .unwrap();

    let listed = db.document_list(10).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].state, "failed");
    assert!(listed[0].sha256.starts_with("path-"));
}

#[tokio::test]
async fn reimport_replaces_text_exactly_once() {
    let db = test_db().await;
    db.document_import_stored("ddd", "draft.md", "md", 10, None, None)
        .await
        .unwrap();
    db.document_mark_ready("ddd", "旧版本内容", false).await.unwrap();
    // Same content re-imported, then the text changed: chunks rebuild
    // without duplicating rows.
    let created = db
        .document_import_stored("ddd", "draft.md", "md", 10, None, None)
        .await
        .unwrap();
    assert!(!created, "same bytes must stay a no-op");
    let state = db.document_mark_ready("ddd", "全新内容重写", false).await.unwrap();
    assert_eq!(state, "ready");

    assert!(db.document_search("旧版本", 10).await.unwrap().is_empty());
    let hits = db.document_search("全新内容", 10).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].ordinal, 0);

    let doc = db.document_get("ddd").await.unwrap().unwrap();
    assert_eq!(doc.chunk_count, 1);
    assert_eq!(doc.state, "ready");
}

// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

fn scanned(path: &str, name: &str, ext: &str, size: i64) -> screenpipe_db::ScannedFile {
    scanned_modified(path, name, ext, size, 1_000)
}

fn scanned_modified(
    path: &str,
    name: &str,
    ext: &str,
    size: i64,
    modified_ms: i64,
) -> screenpipe_db::ScannedFile {
    screenpipe_db::ScannedFile {
        path: path.to_string(),
        file_name: name.to_string(),
        ext: ext.to_string(),
        size_bytes: size,
        modified_ms,
    }
}

#[tokio::test]
async fn source_crud_is_path_unique() {
    let db = test_db().await;
    assert!(db
        .document_source_add("s1", "/tmp/docs", "", "node_modules")
        .await
        .unwrap());
    // Same path again is a no-op, not a second source.
    assert!(!db
        .document_source_add("s2", "/tmp/docs", "", "")
        .await
        .unwrap());

    let sources = db.document_source_list().await.unwrap();
    assert_eq!(sources.len(), 1);
    assert!(sources[0].enabled);
    assert_eq!(sources[0].exclude_globs, "node_modules");

    db.document_source_update("s1", Some(false), None, Some("build"))
        .await
        .unwrap();
    let sources = db.document_source_list().await.unwrap();
    assert!(!sources[0].enabled);
    assert_eq!(sources[0].exclude_globs, "build");

    db.document_source_remove("s1").await.unwrap();
    assert!(db.document_source_list().await.unwrap().is_empty());
}

#[tokio::test]
async fn scan_diff_tracks_new_changed_and_missing() {
    let db = test_db().await;
    db.document_source_add("s1", "/tmp/docs", "", "").await.unwrap();

    let diff = db
        .document_source_scan_diff(
            "s1",
            &[
                scanned("/tmp/docs/a.md", "a.md", "md", 10),
                scanned("/tmp/docs/b.pdf", "b.pdf", "pdf", 20),
            ],
        )
        .await
        .unwrap();
    assert_eq!(
        diff,
        screenpipe_db::DocumentScanDiff {
            to_import: vec!["/tmp/docs/a.md".into(), "/tmp/docs/b.pdf".into()],
            missing: vec![],
        }
    );

    // Report imported.
    db.document_location_set_import_state("s1", "/tmp/docs/a.md", Some("sha-a"), "imported", None)
        .await
        .unwrap();

    // b.pdf disappears, a.md stays: only b is missing, a is not re-imported.
    let diff = db
        .document_source_scan_diff("s1", &[scanned("/tmp/docs/a.md", "a.md", "md", 10)])
        .await
        .unwrap();
    assert_eq!(diff.to_import, Vec::<String>::new());
    assert_eq!(diff.missing, vec!["/tmp/docs/b.pdf".to_string()]);

    // a.md edited (size changed) → re-import.
    let diff = db
        .document_source_scan_diff("s1", &[scanned("/tmp/docs/a.md", "a.md", "md", 99)])
        .await
        .unwrap();
    assert_eq!(diff.to_import, vec!["/tmp/docs/a.md".to_string()]);

    // b.pdf reappears → import again, old row resurrected from missing.
    let diff = db
        .document_source_scan_diff("s1", &[scanned("/tmp/docs/b.pdf", "b.pdf", "pdf", 20)])
        .await
        .unwrap();
    assert_eq!(diff.to_import, vec!["/tmp/docs/b.pdf".to_string()]);
}

#[tokio::test]
async fn same_content_at_two_paths_keeps_two_locations() {
    let db = test_db().await;
    db.document_source_add("s1", "/tmp/docs", "", "").await.unwrap();
    db.document_source_scan_diff(
        "s1",
        &[
            scanned("/tmp/docs/one.md", "one.md", "md", 10),
            scanned("/tmp/other/two.md", "two.md", "md", 10),
        ],
    )
    .await
    .unwrap();
    // Identical bytes → one managed document, two location rows.
    db.document_location_set_import_state("s1", "/tmp/docs/one.md", Some("sha-same"), "imported", None)
        .await
        .unwrap();
    db.document_location_set_import_state("s1", "/tmp/other/two.md", Some("sha-same"), "imported", None)
        .await
        .unwrap();

    let one = db
        .document_location_list("s1", 10)
        .await
        .unwrap()
        .into_iter()
        .find(|l| l.path == "/tmp/docs/one.md")
        .unwrap();
    assert_eq!(one.sha256, "sha-same");
    assert_eq!(one.state, "imported");
}

#[tokio::test]
async fn failed_import_state_is_visible_and_retried() {
    let db = test_db().await;
    db.document_source_add("s1", "/tmp/docs", "", "").await.unwrap();
    db.document_source_scan_diff("s1", &[scanned("/tmp/docs/x.pdf", "x.pdf", "pdf", 5)])
        .await
        .unwrap();
    db.document_location_set_import_state(
        "s1",
        "/tmp/docs/x.pdf",
        Some("sha-x"),
        "failed",
        Some("解析失败"),
    )
    .await
    .unwrap();

    let rows = db.document_location_list("s1", 10).await.unwrap();
    assert_eq!(rows[0].state, "failed");
    assert_eq!(rows[0].error_message.as_deref(), Some("解析失败"));

    // Failed files are retried on the next reconcile.
    let diff = db
        .document_source_scan_diff("s1", &[scanned("/tmp/docs/x.pdf", "x.pdf", "pdf", 5)])
        .await
        .unwrap();
    assert_eq!(diff.to_import, vec!["/tmp/docs/x.pdf".to_string()]);
}

#[tokio::test]
async fn removing_source_drops_locations_not_documents() {
    let db = test_db().await;
    db.document_import_stored("sha-z", "z.md", "md", 5, None, Some("/m/z.md"))
        .await
        .unwrap();
    db.document_source_add("s1", "/tmp/docs", "", "").await.unwrap();
    db.document_source_scan_diff("s1", &[scanned("/tmp/docs/z.md", "z.md", "md", 5)])
        .await
        .unwrap();
    db.document_location_set_import_state("s1", "/tmp/docs/z.md", Some("sha-z"), "imported", None)
        .await
        .unwrap();

    db.document_source_remove("s1").await.unwrap();
    assert!(db.document_location_list("s1", 10).await.unwrap().is_empty());
    // The managed document itself survives.
    assert!(db.document_get("sha-z").await.unwrap().is_some());
}

#[tokio::test]
async fn missing_mark_writes_timestamp_not_path() {
    // Regression: the mark-missing UPDATE once bound the path into
    // `updated_at = ?2`, corrupting the timestamp column.
    let db = test_db().await;
    db.document_source_add("s1", "/tmp/docs", "", "").await.unwrap();
    db.document_source_scan_diff("s1", &[scanned("/tmp/docs/gone.md", "gone.md", "md", 7)])
        .await
        .unwrap();

    let diff = db.document_source_scan_diff("s1", &[]).await.unwrap();
    assert_eq!(diff.missing, vec!["/tmp/docs/gone.md".to_string()]);

    let row = db.document_location_list("s1", 10).await.unwrap().remove(0);
    assert_eq!(row.state, "missing");
    assert!(row.updated_at.contains('T') && row.updated_at.ends_with('Z'),
        "updated_at must be a timestamp, got {:?}", row.updated_at);
    assert!(!row.updated_at.contains("gone.md"),
        "updated_at must never hold the path, got {:?}", row.updated_at);
}

#[tokio::test]
async fn file_backed_document_source_writes_use_write_pool() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let db_path = temp_dir.path().join("standalone.sqlite");
    let db = DatabaseManager::new(db_path.to_str().unwrap(), Default::default())
        .await
        .expect("file-backed db init");

    assert!(db
        .document_source_add("s1", "/tmp/docs", "md", "node_modules")
        .await
        .expect("source add must write through the writer pool"));
    db.document_source_update("s1", Some(false), None, Some("build"))
        .await
        .expect("source update must write through the writer pool");
    db.document_source_scan_diff(
        "s1",
        &[scanned("/tmp/docs/readme.md", "readme.md", "md", 5)],
    )
    .await
    .expect("location scan must write through the writer pool");
    db.document_location_set_import_state(
        "s1",
        "/tmp/docs/readme.md",
        Some("sha-readme"),
        "imported",
        None,
    )
    .await
    .expect("location state must write through the writer pool");

    let source = db.document_source_list().await.unwrap().remove(0);
    assert!(!source.enabled);
    assert_eq!(source.exclude_globs, "build");
    assert_eq!(db.document_location_list("s1", 10).await.unwrap().len(), 1);
}

#[tokio::test]
async fn same_size_edit_reimports_via_modified_time() {
    let db = test_db().await;
    db.document_source_add("s1", "/tmp/docs", "", "").await.unwrap();
    db.document_source_scan_diff("s1", &[scanned_modified("/tmp/docs/a.md", "a.md", "md", 100, 1_000)])
        .await
        .unwrap();
    db.document_location_set_import_state("s1", "/tmp/docs/a.md", Some("sha-a"), "imported", None)
        .await
        .unwrap();

    // Unchanged mtime + size: no re-import.
    let diff = db
        .document_source_scan_diff("s1", &[scanned_modified("/tmp/docs/a.md", "a.md", "md", 100, 1_000)])
        .await
        .unwrap();
    assert_eq!(diff.to_import, Vec::<String>::new());

    // Same size, newer mtime: edited in place, must re-import.
    let diff = db
        .document_source_scan_diff("s1", &[scanned_modified("/tmp/docs/a.md", "a.md", "md", 100, 2_000)])
        .await
        .unwrap();
    assert_eq!(diff.to_import, vec!["/tmp/docs/a.md".to_string()]);
}
