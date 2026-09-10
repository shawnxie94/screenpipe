// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com
// Review-derived Local Knowledge correctness probes. Synthetic SQLite only.

use std::{collections::HashMap, sync::Arc};

use chrono::Utc;
use screenpipe_db::{
    KnowledgeSearchDocInput, KnowledgeSourceInput, DatabaseManager, DeletionCause, OfficeSourceMeta,
    SourceKind, SourceLocator,
};
use screenpipe_engine::knowledge::{
    registry::{validate, RegistryContext},
    search::{search, KnowledgeSearchArgs},
    types::{AnswerFilters, KnowledgeType, RouteStatus},
    KnowledgeShared,
};
use serde_json::json;

async fn db() -> Arc<DatabaseManager> {
    Arc::new(
        DatabaseManager::new("sqlite::memory:", Default::default())
            .await
            .unwrap(),
    )
}

fn source(office: bool, locator_id: i64, captured_at: chrono::DateTime<Utc>) -> KnowledgeSourceInput {
    KnowledgeSourceInput {
        kind: if office {
            SourceKind::OfficeMessage
        } else {
            SourceKind::Frame
        },
        locator: SourceLocator::new(
            if office {
                "brain_office_objects"
            } else {
                "frames"
            },
            locator_id,
        ),
        revision: if office {
            "office-fixture-rev"
        } else {
            "frame-fixture-rev"
        }
        .into(),
        fingerprint_inputs: json!({"synthetic": true}),
        captured_at,
        app: Some("fixture-app".into()),
        window_name: None,
        evidence_method: "review_fixture".into(),
        media_available: false,
        office: office.then(|| OfficeSourceMeta {
            provider: "feishu".into(),
            account_namespace: "review-account".into(),
            object_id: "om_review".into(),
            object_revision: Some("office-fixture-rev".into()),
            event_at: Some(captured_at),
            fetched_at: captured_at,
            source_url: None,
            completeness: "full".into(),
            activity_anchor: None,
            platform_generated: false,
        }),
        excerpt: Some("reviewmarker synthetic body".into()),
    }
}

/// Insert a real event-driven frame through the shared write transaction so
/// source fixtures cannot pass only because their locator is fictional.
async fn frame_fixture(db: &Arc<DatabaseManager>, captured_at: chrono::DateTime<Utc>) -> i64 {
    let mut tx = db.begin_immediate_with_retry().await.unwrap();
    let id = sqlx::query(
        "INSERT INTO frames (timestamp, app_name, window_name, focused, device_name, full_text, text_source) \
         VALUES (?1, ?2, ?3, 1, ?4, ?5, 'ocr')",
    )
    .bind(captured_at)
    .bind("fixture-app")
    .bind("fixture-window")
    .bind("f01-device")
    .bind("reviewmarker synthetic body")
    .execute(&mut **tx.conn())
    .await
    .unwrap()
    .last_insert_rowid();
    tx.commit().await.unwrap();
    id
}

async fn indexed_source(db: &Arc<DatabaseManager>, office: bool) -> String {
    let captured_at = Utc::now();
    let locator_id = if office {
        0
    } else {
        frame_fixture(db, captured_at).await
    };
    let input = source(office, locator_id, captured_at);
    let registered = db.knowledge_register_source(&input).await.unwrap();
    if office {
        db.knowledge_upsert_office_object(
            "feishu",
            "review-account",
            "message",
            "om_review",
            &registered.source_uid,
            Some(&registered.revision),
            "full",
            Some(captured_at),
            captured_at,
            Some("reviewmarker"),
            None,
        )
        .await
        .unwrap();
        assert!(db
            .knowledge_office_get_object("feishu", "review-account", "message", "om_review")
            .await
            .unwrap()
            .is_some());
    }
    db.knowledge_search_upsert_doc(&KnowledgeSearchDocInput {
        doc_id: format!("source:{}", registered.source_uid),
        kind: "source".into(),
        ref_uid: registered.source_uid.clone(),
        ref_revision: registered.revision.clone(),
        body: "reviewmarker synthetic body".into(),
        app: Some("fixture-app".into()),
        event_at: Some(captured_at.to_rfc3339()),
        ..Default::default()
    })
    .await
    .unwrap();
    registered.source_uid
}

async fn candidate(db: &Arc<DatabaseManager>, scope: &str) -> String {
    db.knowledge_create_knowledge_candidate(
        "sop",
        scope,
        "reviewmarker",
        r#"{"title":"reviewmarker"}"#,
        scope,
        &[],
    )
    .await
    .unwrap()
    .0
}

async fn publish(
    db: &Arc<DatabaseManager>,
    id: &str,
    version: i64,
    expected: Option<i64>,
    due: &str,
) {
    let version_row = db.knowledge_get_version(id, version).await.unwrap().unwrap();
    assert!(db
        .knowledge_publish_version(id, version, &version_row.revision, expected, due)
        .await
        .unwrap());
}

async fn indexed_knowledge(db: &Arc<DatabaseManager>, due: &str) -> String {
    let source_uid = indexed_source(db, false).await;
    let source = db.knowledge_get_source(&source_uid).await.unwrap().unwrap();
    let work_unit = db
        .knowledge_save_work_unit(
            "fixture-scope",
            Some("fixture-task"),
            Some("2026-09-08T00:00:00Z"),
            Some("2026-09-08T00:01:00Z"),
            "fixture-input-hash",
            "fixture-extractor-v1",
            "fixture-prompt-v1",
            &json!({
                "schema_version": 1,
                "task": {
                    "value": "reviewmarker fixture observation",
                    "evidence_refs": ["u1"]
                },
                "inputs": [{
                    "value": {
                        "source_uid": source.source_uid,
                        "revision": source.revision,
                        "locator": {
                            "table": source.locator_table,
                            "id": source.locator_id
                        },
                        "captured_at": source.captured_at
                    },
                    "evidence_refs": ["u1"]
                }],
                "time_range": {
                    "start": "2026-09-08T00:00:00Z",
                    "end": "2026-09-08T00:01:00Z"
                },
                "actions": [],
                "decisions": [],
                "exceptions": [],
                "outputs": [],
                "result": null,
                "evidence_refs": ["u1"],
                "confidence": {
                    "value": "observed",
                    "evidence_refs": ["u1"]
                }
            })
            .to_string(),
        )
        .await
        .unwrap();
    db.knowledge_register_dependency(
        "work_unit",
        &work_unit,
        None,
        None,
        &source_uid,
        Some(&source.revision),
    )
    .await
    .unwrap();
    let knowledge_body = json!({
        "schema_version": 1,
        "title": "reviewmarker decision rule",
        "condition": "fixture-app contains the reviewmarker observation",
        "rule": "Treat the observation as local evidence for this fixture",
        "boundary": "Applies only to the cited fixture source and its captured interval",
        "single_observation": true,
        "evidence_refs": ["u1"]
    });
    let refs = HashMap::from([(String::from("u1"), work_unit.clone())]);
    assert!(
        validate(
            KnowledgeType::DecisionRule,
            &knowledge_body,
            &RegistryContext {
                work_unit_refs: &refs,
            },
        )
        .is_ok(),
        "knowledge fixture must satisfy the current registry before indexing"
    );
    let id = db
        .knowledge_create_knowledge_candidate(
            "decision_rule",
            "fixture-scope",
            "reviewmarker decision rule",
            &knowledge_body.to_string(),
            "fixture-knowledge-input",
            std::slice::from_ref(&work_unit),
        )
        .await
        .unwrap()
        .0;
    publish(db, &id, 1, None, due).await;
    db.knowledge_search_upsert_doc(&KnowledgeSearchDocInput {
        doc_id: format!("knowledge:{id}:1"),
        kind: "knowledge".into(),
        ref_uid: id.clone(),
        ref_revision: "1".into(),
        body: "reviewmarker".into(),
        app: Some("fixture-app".into()),
        ..Default::default()
    })
    .await
    .unwrap();
    id
}

#[tokio::test]
async fn deletion_p01_explicit_erase_must_clear_derived_body_and_index() {
    let db = db().await;
    let uid = indexed_source(&db, false).await;
    let work_unit = db
        .knowledge_save_work_unit(
            "scope",
            None,
            None,
            None,
            "ih",
            "v1",
            "p1",
            "reviewmarker private derivative",
        )
        .await
        .unwrap();
    db.knowledge_register_dependency(
        "work_unit",
        &work_unit,
        None,
        None,
        &uid,
        Some("frame-fixture-rev"),
    )
    .await
    .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let shared = KnowledgeShared::new(db.clone(), temp.path().to_path_buf());
    shared
        .deletion()
        .delete_sources(vec![uid], DeletionCause::UserErase)
        .await
        .unwrap();
    let residual: i64 = sqlx::query_scalar(
        "SELECT (SELECT COUNT(*) FROM brain_search_documents WHERE body LIKE '%reviewmarker%') + (SELECT COUNT(*) FROM brain_work_unit_revisions WHERE body LIKE '%reviewmarker%')",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(
        residual, 0,
        "erasure reported success but copied bodies survive"
    );
}

#[tokio::test]
async fn migration_runtime_p02_journal_ahead_of_db_must_replay_after_restore() {
    let db = db().await;
    let uid = indexed_source(&db, false).await;
    let temp = tempfile::tempdir().unwrap();
    let shared = KnowledgeShared::new(db.clone(), temp.path().to_path_buf());
    std::fs::write(
        shared.knowledge_dir.join("deletion-journal.jsonl"),
        format!(
            "{}\n",
            json!({"seq": 1, "cause": "user_erase", "scope": {"kind": "sources", "uids": [uid]}, "ts": Utc::now().to_rfc3339()})
        ),
    )
    .unwrap();
    shared.deletion().recover_incomplete().await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM brain_sources WHERE source_uid = ?")
        .bind(&uid)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "journal-only deletion was not replayed");
}

#[tokio::test]
async fn retrieval_p03_disabled_source_must_not_be_retrieved() {
    let db = db().await;
    let uid = indexed_source(&db, false).await;
    db.knowledge_set_source_state(&[uid], "disabled").await.unwrap();
    let filters = AnswerFilters::default();
    let result = search(KnowledgeSearchArgs {
        db: &db,
        question: "reviewmarker",
        filters: &filters,
    })
    .await;
    assert!(
        result.evidence.is_empty(),
        "disabled source was returned as answer evidence"
    );
}

#[tokio::test]
async fn retrieval_answer_p04_expired_knowledge_must_not_be_quoted() {
    let db = db().await;
    let id = indexed_knowledge(&db, "2099-01-01T00:00:00Z").await;
    let filters = AnswerFilters::default();
    let result = search(KnowledgeSearchArgs {
        db: &db,
        question: "reviewmarker",
        filters: &filters,
    })
    .await;
    assert_eq!(
        result.knowledge.len(),
        1,
        "valid knowledge fixture was not quotable"
    );

    let mut tx = db.begin_immediate_with_retry().await.unwrap();
    sqlx::query(
        "UPDATE brain_knowledge_versions SET review_due_at = '2000-01-01T00:00:00Z' \
         WHERE knowledge_id = ?1 AND version = 1",
    )
    .bind(&id)
    .execute(&mut **tx.conn())
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let result = search(KnowledgeSearchArgs {
        db: &db,
        question: "reviewmarker",
        filters: &filters,
    })
    .await;
    assert!(
        result.knowledge.is_empty(),
        "past-due knowledge is still quotable"
    );
}

#[tokio::test]
async fn retrieval_p05_knowledge_must_obey_app_filters() {
    let db = db().await;
    indexed_knowledge(&db, "2099-01-01T00:00:00Z").await;
    let matching_filters = AnswerFilters {
        apps: vec!["fixture-app".into()],
        ..Default::default()
    };
    let matched = search(KnowledgeSearchArgs {
        db: &db,
        question: "reviewmarker",
        filters: &matching_filters,
    })
    .await;
    assert_eq!(
        matched.knowledge.len(),
        1,
        "matching-app knowledge fixture was not quotable"
    );
    let filters = AnswerFilters {
        apps: vec!["different-app".into()],
        ..Default::default()
    };
    let result = search(KnowledgeSearchArgs {
        db: &db,
        question: "reviewmarker",
        filters: &filters,
    })
    .await;
    assert!(
        result.knowledge.is_empty(),
        "knowledge route ignored app filter"
    );
}

#[tokio::test]
async fn retrieval_p06_broken_index_must_report_failed_not_no_hits() {
    let db = db().await;
    let mut tx = db.begin_immediate_with_retry().await.unwrap();
    sqlx::query("DROP TABLE brain_search_fts")
        .execute(&mut **tx.conn())
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let filters = AnswerFilters::default();
    let result = search(KnowledgeSearchArgs {
        db: &db,
        question: "reviewmarker",
        filters: &filters,
    })
    .await;
    assert_eq!(
        result.report.routes[0].status,
        RouteStatus::Failed,
        "SQL error was swallowed as empty retrieval"
    );
}

#[tokio::test]
async fn review_publication_p07_second_knowledge_v2_must_supersede_its_v1() {
    let db = db().await;
    let _first = candidate(&db, "first").await;
    let id = candidate(&db, "second").await;
    publish(&db, &id, 1, None, "2099-01-01T00:00:00Z").await;
    db.knowledge_add_knowledge_candidate_version(&id, "v2", "{}", "new-input", &[])
        .await
        .unwrap();
    publish(&db, &id, 2, Some(1), "2099-01-01T00:00:00Z").await;
    let version_one = db.knowledge_get_version(&id, 1).await.unwrap().unwrap();
    assert_eq!(
        version_one.state, "superseded",
        "row id and version number were mixed"
    );
}

#[tokio::test]
async fn review_publication_p08_failed_publish_cas_must_preserve_current_version() {
    let db = db().await;
    let id = candidate(&db, "first").await;
    publish(&db, &id, 1, None, "2099-01-01T00:00:00Z").await;
    db.knowledge_add_knowledge_candidate_version(&id, "v2", "{}", "new-input", &[])
        .await
        .unwrap();
    assert!(!db
        .knowledge_publish_version(&id, 2, "wrong-revision", Some(1), "2099-01-01T00:00:00Z")
        .await
        .unwrap());
    let version_one = db.knowledge_get_version(&id, 1).await.unwrap().unwrap();
    assert_eq!(
        version_one.state, "published",
        "failed CAS mutated the old published version"
    );
}

#[tokio::test]
async fn office_control_office_evidence_p09_erased_office_object_must_not_reimport() {
    let db = db().await;
    let uid = indexed_source(&db, true).await;
    let temp = tempfile::tempdir().unwrap();
    let shared = KnowledgeShared::new(db.clone(), temp.path().to_path_buf());
    shared
        .deletion()
        .delete_sources(vec![uid], DeletionCause::UserErase)
        .await
        .unwrap();
    let registration = db
        .knowledge_register_source(&source(true, 0, Utc::now()))
        .await
        .unwrap();
    assert!(
        registration.suppressed,
        "office tombstone namespace does not match registration"
    );
}

#[test]
fn extraction_p10_sop_session_count_must_be_evidence_derived() {
    let refs = HashMap::from([("u1".into(), "one-real-session".into())]);
    let body = json!({
        "schema_version": 1,
        "title": "synthetic SOP",
        "session_count": 3,
        "applicability": ["synthetic"],
        "steps": [{"name": "step", "evidence_refs": ["u1"]}]
    });
    assert!(
        validate(
            KnowledgeType::Sop,
            &body,
            &RegistryContext {
                work_unit_refs: &refs
            }
        )
        .is_err(),
        "one session plus model-declared 3 was accepted"
    );
}

#[tokio::test]
async fn retrieval_answer_p11_positive_control_valid_source_is_retrievable() {
    let db = db().await;
    let uid = indexed_source(&db, false).await;
    let registered = db.knowledge_get_source(&uid).await.unwrap().unwrap();
    assert_eq!(registered.locator_table, "frames");
    let (full_text, app_name, frame_timestamp): (Option<String>, Option<String>, String) =
        sqlx::query_as("SELECT full_text, app_name, timestamp FROM frames WHERE id = ?1")
            .bind(registered.locator_id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(full_text.as_deref(), Some("reviewmarker synthetic body"));
    assert_eq!(app_name.as_deref(), Some("fixture-app"));
    assert_eq!(
        chrono::DateTime::parse_from_rfc3339(&frame_timestamp)
            .unwrap()
            .with_timezone(&Utc),
        chrono::DateTime::parse_from_rfc3339(&registered.captured_at)
            .unwrap()
            .with_timezone(&Utc),
        "source captured_at must identify the fixture frame timestamp"
    );
    let source_text = db
        .knowledge_source_text(&uid, &registered.revision)
        .await
        .unwrap()
        .expect("registered source revision must be readable");
    assert_eq!(source_text.0, "reviewmarker synthetic body");
    assert!(!source_text.1, "live fixture source should not be archived");
    let filters = AnswerFilters::default();
    let result = search(KnowledgeSearchArgs {
        db: &db,
        question: "reviewmarker",
        filters: &filters,
    })
    .await;
    assert_eq!(
        result.evidence.len(),
        1,
        "positive control must retrieve the synthetic marker"
    );
    assert_eq!(result.evidence[0].source_uid, uid);
}

#[tokio::test]
async fn deletion_concurrent_waves_have_unique_durable_sequences() {
    let db = db().await;
    let first = indexed_source(&db, false).await;
    let second = indexed_source(&db, false).await;
    let temp = tempfile::tempdir().unwrap();
    let shared = KnowledgeShared::new(db.clone(), temp.path().to_path_buf());
    let service = shared.deletion();
    let (left, right) = tokio::join!(
        service.delete_sources(vec![first], DeletionCause::UserErase),
        service.delete_sources(vec![second], DeletionCause::UserErase),
    );
    left.unwrap();
    right.unwrap();
    let seqs: Vec<i64> =
        sqlx::query_scalar("SELECT journal_seq FROM brain_deletions ORDER BY journal_seq")
            .fetch_all(&db.pool)
            .await
            .unwrap();
    assert_eq!(
        seqs,
        vec![1, 2],
        "journal sequence allocation was not serialized"
    );
    let journal = tokio::fs::read_to_string(temp.path().join("knowledge/deletion-journal.jsonl"))
        .await
        .unwrap();
    assert_eq!(journal.lines().count(), 2);
}

#[tokio::test]
async fn deletion_corrupt_tail_fails_closed_and_keeps_source() {
    let db = db().await;
    let uid = indexed_source(&db, false).await;
    let temp = tempfile::tempdir().unwrap();
    let shared = KnowledgeShared::new(db.clone(), temp.path().to_path_buf());
    std::fs::write(
        shared.knowledge_dir.join("deletion-journal.jsonl"),
        format!(
            "{}\nnot-json\n",
            json!({"seq": 1, "cause": "user_erase", "scope": {"kind": "sources", "uids": [uid]}, "ts": Utc::now().to_rfc3339()})
        ),
    )
    .unwrap();
    let error = shared.deletion().recover_incomplete().await.unwrap_err();
    assert_eq!(error.code, "journal_corrupt");
    assert!(db.knowledge_get_source(&uid).await.unwrap().is_some());
}
