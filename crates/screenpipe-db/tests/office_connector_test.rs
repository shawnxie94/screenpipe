// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

//! Office connector persistence tests: object upsert idempotency, FTS search,
//! disable/erase lifecycle, cursor bookkeeping.

use chrono::{Duration, Utc};
use screenpipe_db::{
    ConnectorObjectDraft, DatabaseManager, OfficeObjectDraft, ConnectorUpsertOutcome,
};

async fn test_db() -> DatabaseManager {
    DatabaseManager::new("sqlite::memory:", Default::default())
        .await
        .expect("db init")
}

fn draft(kind: &str, id: &str, body: &str) -> OfficeObjectDraft {
    OfficeObjectDraft {
        provider: "feishu".to_string(),
        account_namespace: "acct-1".to_string(),
        object_kind: kind.to_string(),
        object_id: id.to_string(),
        revision: None,
        title: Some(format!("title {id}")),
        body_text: body.to_string(),
        completeness: "full".to_string(),
        event_at: Some(Utc::now() - Duration::hours(1)),
        fetched_at: Utc::now(),
        source_url: None,
        activity_anchor: None,
        platform_generated: false,
    }
}

#[tokio::test]
async fn upsert_is_idempotent_and_fts_is_searchable() {
    let db = test_db().await;
    let d = draft("message", "chat-1/msg-1", "项目复盘讨论核心结论");
    let first = db.office_upsert_object(&d).await.unwrap();
    assert!(first, "first import should create");
    let second = db.office_upsert_object(&d).await.unwrap();
    assert!(!second, "re-import must not re-create (idempotent)");

    let hits = db
        .office_search(Some("feishu"), "项目复盘", 10)
        .await
        .unwrap();
    assert!(!hits.is_empty(), "fts must find the object");
    assert_eq!(hits[0].0.object_id, "chat-1/msg-1");
}

#[tokio::test]
async fn disable_and_erase_lifecycle() {
    let db = test_db().await;
    db.office_upsert_object(&draft("message", "m1", "keep this"))
        .await
        .unwrap();
    db.office_upsert_object(&draft("message", "m2", "drop this"))
        .await
        .unwrap();

    // Disable everything except m1.
    let disabled = db
        .office_disable_out_of_scope(
            "feishu",
            "acct-1",
            &[("message".to_string(), "m1".to_string())],
        )
        .await
        .unwrap();
    assert_eq!(disabled.len(), 1);
    assert_eq!(disabled[0].1, "m2");
    let hits = db.office_search(Some("feishu"), "drop", 10).await.unwrap();
    assert!(hits.is_empty(), "disabled object must not surface in fts");
    // Active count reflects only m1.
    assert_eq!(db.office_imported_object_count("feishu").await.unwrap(), 1);

    // Erase everything: state -> deleted, fts dropped, re-import suppressed.
    let erased = db.office_erase("feishu").await.unwrap();
    assert_eq!(erased, 2);
    let hits = db.office_search(Some("feishu"), "keep", 10).await.unwrap();
    assert!(hits.is_empty(), "erased objects must not surface");
    db.office_upsert_object(&draft("message", "m1", "keep this"))
        .await
        .unwrap();
    let hits = db.office_search(Some("feishu"), "keep", 10).await.unwrap();
    assert!(
        hits.is_empty(),
        "re-import after erase must stay suppressed"
    );
}

#[tokio::test]
async fn connector_search_deduplicates_repeated_fts_rows_by_primary_key() {
    let db = test_db().await;
    let fetched_at = Utc::now() - Duration::hours(1);
    let draft = ConnectorObjectDraft {
        connector: "rss".into(),
        namespace: "feed-a".into(),
        object_kind: "item".into(),
        object_id: "item-1".into(),
        title: Some("stable search object".into()),
        body_text: "stable-token body".into(),
        content_hash: Some("content-1".into()),
        fetched_at,
        ..Default::default()
    };
    assert_eq!(
        db.connector_upsert_object(&draft).await.unwrap(),
        ConnectorUpsertOutcome::Created
    );
    // Simulate an already-corrupted/rebuilt FTS index with duplicate rows for
    // the same connector primary key. Page and count must still agree.
    sqlx::query(
        "INSERT INTO connector_objects_fts (body, title, connector, namespace, object_kind, object_id) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind("stable-token body")
    .bind("stable search object")
    .bind("rss")
    .bind("feed-a")
    .bind("item")
    .bind("item-1")
    .execute(&db.pool)
    .await
    .unwrap();

    let (page, total) = db
        .connector_search_page("stable-token", None, None, 10, 0)
        .await
        .unwrap();
    assert_eq!(total, 1);
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].object_id, "item-1");

    let time_page = db
        .connector_search_time_page("stable-token", None, None, 10)
        .await
        .unwrap();
    assert_eq!(time_page.len(), 1);
    assert_eq!(
        db.connector_search_time_count("stable-token", None, None)
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn cursors_roundtrip() {
    let db = test_db().await;
    assert!(db
        .office_get_cursor("feishu", "chat:c1:messages")
        .await
        .unwrap()
        .is_none());
    db.office_set_cursor("feishu", "chat:c1:messages", "2026-09-14T10:00:00Z")
        .await
        .unwrap();
    assert_eq!(
        db.office_get_cursor("feishu", "chat:c1:messages")
            .await
            .unwrap()
            .as_deref(),
        Some("2026-09-14T10:00:00Z")
    );
    db.office_clear_cursors("feishu").await.unwrap();
    assert!(db
        .office_get_cursor("feishu", "chat:c1:messages")
        .await
        .unwrap()
        .is_none());
}
#[tokio::test]
async fn save_scope_bumps_connection_scope_revision() {
    // start_sync compares expected_revision against the CONNECTION row's
    // scope_revision; saving a scope must advance that row (regression guard
    // for the connector-merge rewrite).
    let db = DatabaseManager::new("sqlite::memory:", Default::default())
        .await
        .unwrap();
    let r1 = db.office_save_scope("feishu", "{}").await.unwrap();
    assert_eq!(r1, 1);
    let r2 = db.office_save_scope("feishu", "{}").await.unwrap();
    assert_eq!(r2, 2);

    let row = db.office_get_connection("feishu").await.unwrap().unwrap();
    assert_eq!(row.scope_revision, 2);

    // office_update_connection returning the same counter stays consistent.
    let returned = db
        .office_update_connection(
            "feishu",
            screenpipe_db::OfficeConnectionUpdate {
                bump_scope_revision: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(returned, 3);
}

#[tokio::test]
async fn connector_search_page_matches_browses_and_counts() {
    let db = test_db().await;
    db.office_upsert_object(&draft("message", "m1", "季度目标对齐会议"))
        .await
        .unwrap();
    let mut old = draft("document", "d1", "季度总结文档");
    old.event_at = Some(Utc::now() - Duration::hours(48));
    db.office_upsert_object(&old).await.unwrap();

    // Keyword search: relevance across channels with an accurate total.
    let (rows, total) = db
        .connector_search_page("季度", None, None, 10, 0)
        .await
        .unwrap();
    assert_eq!(total, 2);
    assert_eq!(rows.len(), 2);

    // Pagination honors offset and reports a stable total.
    let (page2, total) = db
        .connector_search_page("季度", None, None, 1, 1)
        .await
        .unwrap();
    assert_eq!(total, 2);
    assert_eq!(page2.len(), 1);

    // Browse (empty query) returns recent items newest-first.
    let (rows, total) = db
        .connector_search_page("", None, None, 10, 0)
        .await
        .unwrap();
    assert_eq!(total, 2);
    assert_eq!(rows[0].object_id, "m1");

    // Time filter excludes the 48h-old document.
    let since = Utc::now() - Duration::hours(24);
    let (rows, total) = db
        .connector_search_page("季度", Some(since), None, 10, 0)
        .await
        .unwrap();
    assert_eq!(total, 1);
    assert_eq!(rows[0].object_id, "m1");

    // Disabled rows never surface.
    db.office_disable_out_of_scope("feishu", "acct-1", &[])
        .await
        .unwrap();
    let (_, total) = db
        .connector_search_page("季度", None, None, 10, 0)
        .await
        .unwrap();
    assert_eq!(total, 0);
}

#[tokio::test]
async fn connector_fts_survives_query_syntax_characters() {
    // Raw user text reaches connector FTS from the ⌘K 全部/接入 scopes and the
    // office provider search. FTS5 operators in that text (quotes, parens,
    // AND/OR/NOT, bare minus) used to raise `fts5: syntax error` and take the
    // whole unified search down with a 500 even though the other legs were
    // fine. Sanitizing must turn such input into a literal-token query.
    let db = test_db().await;
    let draft = ConnectorObjectDraft {
        connector: "office:feishu".into(),
        namespace: "acct-1".into(),
        object_kind: "message".into(),
        object_id: "m-1".into(),
        title: Some("pi agent".into()),
        body_text: "pi agent 发布会".into(),
        content_hash: Some("hash-1".into()),
        fetched_at: Utc::now(),
        ..Default::default()
    };
    db.connector_upsert_object(&draft).await.unwrap();

    let poison = [
        "pi \"agent\" OR (",
        "agent NOT",
        "(unbalanced",
        "a-b \"c",
        "C++",
    ];
    for q in poison {
        let (page, total) = db
            .connector_search_page(q, None, None, 10, 0)
            .await
            .unwrap_or_else(|e| panic!("connector_search_page poisoned by {q:?}: {e}"));
        assert_eq!(
            total as usize,
            page.len(),
            "page/count disagree for {q:?}"
        );
        db.connector_search_time_page(q, None, None, 10)
            .await
            .unwrap_or_else(|e| panic!("connector_search_time_page poisoned by {q:?}: {e}"));
        db.connector_search_time_count(q, None, None)
            .await
            .unwrap_or_else(|e| panic!("connector_search_time_count poisoned by {q:?}: {e}"));
    }

    // A clean query still matches after sanitization.
    let (page, total) = db
        .connector_search_page("pi agent", None, None, 10, 0)
        .await
        .unwrap();
    assert_eq!(total, 1);
    assert_eq!(page[0].object_id, "m-1");
}

#[tokio::test]
async fn office_search_survives_query_syntax_characters() {
    let db = test_db().await;
    db.office_upsert_object(&draft("message", "m-1", "pi agent 发布会"))
        .await
        .unwrap();
    for q in ["pi \"agent\" OR (", "agent NOT", "(unbalanced"] {
        db.office_search(Some("feishu"), q, 10)
            .await
            .unwrap_or_else(|e| panic!("office_search poisoned by {q:?}: {e}"));
    }
    let hits = db
        .office_search(Some("feishu"), "pi agent", 10)
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].0.object_id, "m-1");
}
