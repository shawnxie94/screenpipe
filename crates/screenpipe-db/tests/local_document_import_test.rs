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

    // Empty query browses ready documents.
    let browse = db.document_search("", 10).await.unwrap();
    assert_eq!(browse.len(), 1);

    // FTS syntax in user input stays literal instead of erroring.
    assert!(db.document_search("core AND (tech)", 10).await.unwrap().is_empty());
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
